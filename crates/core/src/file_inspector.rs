use anyhow::{Context, Ok, Result};
use infer::Type;
use mime::Mime;
use rasteroid::{Frame, image_extended::InlineImage, term_misc::Wininfo};
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct McatFile {
    bytes: Vec<u8>,
    path: Option<PathBuf>,
    mime: Option<Mime>,
    infer_type: Option<Type>,
}

impl McatFile {
    // ## Constructors ##

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path)?;
        let infer_type = infer::get(&bytes);

        Ok(Self {
            bytes,
            path: Some(path),
            mime: None,
            infer_type,
        })
    }

    pub fn from_bytes(bytes: Vec<u8>, mime: Option<Mime>) -> Self {
        let infer_type = infer::get(&bytes);
        Self {
            bytes,
            path: None,
            mime,
            infer_type,
        }
    }

    // ## Type detection ##

    pub fn is_image(&self) -> bool {
        image::guess_format(&self.bytes).is_ok()
    }

    pub fn is_video(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Video)
            .unwrap_or(false)
    }

    pub fn is_audio(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Audio)
            .unwrap_or(false)
    }

    pub fn is_archive(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Archive)
            .unwrap_or(false)
    }

    pub fn is_book(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Book)
            .unwrap_or(false)
    }

    pub fn is_document(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Doc)
            .unwrap_or(false)
    }

    pub fn is_font(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::Font)
            .unwrap_or(false)
    }

    pub fn is_app(&self) -> bool {
        self.infer_type
            .map(|t| t.matcher_type() == infer::MatcherType::App)
            .unwrap_or(false)
    }

    pub fn is_pdf(&self) -> bool {
        self.infer_type
            .map(|t| t.mime_type() == mime::APPLICATION_PDF.as_ref())
            .unwrap_or(false)
    }

    pub fn is_svg(&self) -> bool {
        self.extension().as_deref() == Some("svg")
            || self.bytes.starts_with(b"<svg")
            || self.mime.as_ref() == Some(&mime::IMAGE_SVG)
    }

    pub fn is_html(&self) -> bool {
        self.extension().as_deref() == Some("html")
            || self.extension().as_deref() == Some("htm")
            || self.mime.as_ref() == Some(&mime::TEXT_HTML)
            || self.bytes.starts_with(b"<!DOCTYPE html")
            || self.bytes.starts_with(b"<html")
    }

    // ## Raw access ##

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_string(self) -> Result<String> {
        Ok(String::from_utf8(self.bytes)?)
    }

    pub fn path(&self) -> Result<&PathBuf> {
        self.path
            .as_ref()
            .context("no path available for byte source")
    }

    // ## Conversions ##

    /// Supports: images, pdf, svg, tex, typ, exe, lnk
    pub fn to_image(
        &self,
        wininfo: &Wininfo,
        width: Option<&str>,
        height: Option<&str>,
        is_ascii: bool,
    ) -> Result<(Vec<u8>, u32, u32)> {
        let img = if self.is_image() {
            image::load_from_memory(&self.bytes)?
        } else if self.is_svg() {
            return svg_to_image(&self.bytes, wininfo, width, height);
        } else {
            todo!()
        };

        let (img, width, height) = img.resize_plus(wininfo, width, height, is_ascii, false)?;

        Ok((img, width, height))
    }

    pub fn to_markdown(&self) {
        todo!()
    }

    /// Supports: video formats via ffmpeg
    // pub fn to_frames(&self) -> Result<impl Iterator<Item = VideoFrames>> {
    //     todo!()
    // }

    // ## Internal ##

    fn extension(&self) -> Option<String> {
        self.path
            .as_ref()?
            .extension()?
            .to_str()
            .map(|e| e.to_lowercase())
    }
}

pub struct VideoFrames {
    timestamp: f32,
    img: Vec<u8>,
    width: u16,
    height: u16,
}
impl Frame for VideoFrames {
    fn timestamp(&self) -> f32 {
        self.timestamp
    }
    fn data(&self) -> &[u8] {
        &self.img
    }
    fn width(&self) -> u16 {
        self.width as u16
    }
    fn height(&self) -> u16 {
        self.height as u16
    }
}

// converting methods.

pub fn svg_to_image(
    bytes: &[u8],
    wininfo: &Wininfo,
    width: Option<&str>,
    height: Option<&str>,
) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let mut opt = Options::default();

    // allowing text
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    opt.fontdb = std::sync::Arc::new(fontdb);
    opt.text_rendering = usvg::TextRendering::OptimizeLegibility;

    let tree = Tree::from_data(bytes, &opt)?;
    let pixmap_size = tree.size();
    let src_width = pixmap_size.width();
    let src_height = pixmap_size.height();

    let width = match width {
        Some(w) => wininfo.dim_to_px(w, rasteroid::term_misc::SizeDirection::Width)?,
        None => src_width as u32,
    };
    let height = match height {
        Some(h) => wininfo.dim_to_px(h, rasteroid::term_misc::SizeDirection::Height)?,
        None => src_height as u32,
    };
    let (target_width, target_height) =
        rasteroid::image_extended::calc_fit(src_width as u32, src_height as u32, width, height);
    let scale_x = target_width as f32 / src_width;
    let scale_y = target_height as f32 / src_height;
    let scale = scale_x.min(scale_y);

    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .context("Failed to create pixmap for svg")?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok((pixmap.data().to_vec(), target_width, target_height))
}
