use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use image::{DynamicImage, GenericImage};
use infer::{app::is_exe, archive::is_pdf, image::is_gif, is_video};
use lzma_rust2::XzReader;
use markdownify::MarkdownifyInput;
use pelite::PeFile;
use rasteroid::{
    Frame, RasterEncoder,
    image_extended::InlineImage,
    term_misc::{SizeDirection, Wininfo},
};
use reqwest::Url;
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};
use std::{
    fs::{self},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

use tracing::{debug, info};

use crate::{
    cdp::ChromeHeadless,
    config::{McatConfig, Theme},
    fetch_manager, markdown_viewer,
};

#[derive(Clone, Default, Debug, PartialEq)]
pub enum McatKind {
    #[default]
    PreMarkdown, // is the most common ones, just something that is passed into markdownify
    Markdown,
    Html,

    Video,
    Gif, // have different logic on iterm

    Image,
    Svg, // svg is handled manually, since its not supported by the image crate

    Url,
    Exe,
    Lnk,

    // has some manual handling
    Pdf,
    Tex,
    Typst,
}

type Checker = fn(&[u8]) -> bool;

pub struct McatFile {
    pub bytes: Vec<u8>,

    pub kind: McatKind,
    pub path: Option<PathBuf>,
}

impl McatFile {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let ext = path.extension().map(|v| v.to_string_lossy().to_string());
        let bytes = fs::read(&path)?;

        let mut s = Self::from_bytes(bytes, ext.as_deref())?;
        info!(path = %path.display(), kind = ?s.kind, "loaded file");
        s.path = Some(path);
        Ok(s)
    }

    pub fn from_bytes(bytes: Vec<u8>, ext: Option<&str>) -> Result<Self> {
        let bytes: Vec<u8> = if infer::archive::is_gz(&bytes) {
            let mut decoder = GzDecoder::new(bytes.as_slice());
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else if infer::archive::is_xz(&bytes) {
            let mut decoder = XzReader::new(bytes.as_slice(), true);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else {
            bytes
        };
        let kind = Self::detect_kind(&bytes, ext);

        Ok(Self {
            bytes,
            path: None,
            kind,
        })
    }

    fn detect_kind(bytes: &[u8], ext: Option<&str>) -> McatKind {
        let ext = ext.unwrap_or("");
        let handlers: &[(Checker, &str, McatKind)] = &[
            (is_pdf, "", McatKind::Pdf),
            (is_gif, "", McatKind::Gif), // gif most be before video check.
            (|b| image::guess_format(b).is_ok(), "", McatKind::Image),
            (is_video, "", McatKind::Video),
            (is_exe, "", McatKind::Exe),
            (|_| false, "svg", McatKind::Svg),
            (|_| false, "html", McatKind::Html),
            (|_| false, "htm", McatKind::Html),
            (|_| false, "md", McatKind::Markdown),
            (|_| false, "tex", McatKind::Tex),
            (|_| false, "typ", McatKind::Typst),
            (|_| false, "lnk", McatKind::Lnk),
            (|_| false, "url", McatKind::Url),
        ];

        handlers
            .iter()
            .find(|(check, e, _)| check(bytes) || (!e.is_empty() && ext == *e))
            .map(|(_, _, kind)| kind.clone())
            .unwrap_or_default()
    }

    pub fn to_html(&self, theme_for_style: Option<Theme>) -> Result<String> {
        let md = self.to_markdown_input(false)?.convert()?;
        let should_style = theme_for_style.is_some();
        let html =
            markdown_viewer::md_to_html(&md, &theme_for_style.unwrap_or_default(), should_style);

        Ok(html)
    }

    pub fn to_image(
        &self,
        config: &McatConfig,
        pad: bool,
        resize: bool,
    ) -> Result<(Vec<u8>, u32, u32)> {
        debug!(kind = ?self.kind, pad, resize, "converting to image");
        let wininfo = config
            .wininfo
            .as_ref()
            .context("this is likely a bug, tried to convert to image and wininfo is None")?;
        let width: Option<&str> = Some(&config.img_width);
        let height: Option<&str> = Some(&config.img_height);
        let is_ascii = config
            .encoder
            .map(|v| v == RasterEncoder::Ascii)
            .unwrap_or(false);

        let img: DynamicImage = match self.kind {
            McatKind::PreMarkdown | McatKind::Markdown => {
                let theme = config.theme.clone();
                let html = self.to_html(Some(theme))?;
                let file = McatFile::from_bytes(html.into_bytes(), Some("html"))?;
                html_to_image(&file)?
            }
            McatKind::Html => html_to_image(self)?,
            McatKind::Video => anyhow::bail!(
                "Cannot turn video format to image, this is most likely a bug and should not reach here."
            ),
            McatKind::Gif => image::load_from_memory(&self.bytes)?,
            McatKind::Image => image::load_from_memory(&self.bytes)?,
            McatKind::Svg => {
                return svg_to_image(&self.bytes, wininfo, width, height, is_ascii, pad, resize);
            }
            McatKind::Url => url_to_image(&self.bytes)?,
            McatKind::Exe => exe_to_image(&self.bytes)?,
            McatKind::Lnk => lnk_to_image(&self.bytes)?,
            McatKind::Pdf => todo!(),
            McatKind::Tex => todo!(),
            McatKind::Typst => todo!(),
        };

        if resize {
            let (img, width, height) = img.resize_plus(wininfo, width, height, is_ascii, pad)?;
            Ok((img, width, height))
        } else {
            let mut buf = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)?;
            Ok((buf, 0, 0))
        }
    }

    pub fn to_markdown_input(&self, inline_images: bool) -> Result<MarkdownifyInput> {
        let mut input = MarkdownifyInput::from_bytes(
            self.bytes.clone(),
            self.path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        )?;
        input.allow_inline_images = inline_images;
        input.path = self.path.clone();
        input.ext = self
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        Ok(input)
    }

    pub fn to_frames(&self) -> Result<Box<dyn Iterator<Item = VideoFrames>>> {
        let input = if let Some(path) = &self.path {
            path.to_string_lossy().to_string()
        } else {
            let mut tmp_file = NamedTempFile::with_suffix(".mp4")?;
            tmp_file.write_all(&self.bytes)?;
            tmp_file.path().to_string_lossy().to_string()
        };

        let mut command = fetch_manager::get_ffmpeg().context(
            "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`",
        )?;

        command.hwaccel("auto").input(&input).rawvideo();
        let mut child = command.spawn()?;
        let frames = child.iter()?.filter_frames().map(|f| VideoFrames {
            timestamp: f.timestamp,
            img: f.data,
            width: f.width as u16,
            height: f.height as u16,
        });

        Ok(Box::new(frames))
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
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }
}

// converting methods.

pub fn svg_to_image(
    bytes: &[u8],
    wininfo: &Wininfo,
    width: Option<&str>,
    height: Option<&str>,
    is_ascii: bool,
    pad: bool,
    needs_resize: bool,
) -> Result<(Vec<u8>, u32, u32)> {
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
        Some(w) if needs_resize => match is_ascii {
            true => wininfo.dim_to_cells(w, SizeDirection::Width)?,
            false => wininfo.dim_to_px(w, SizeDirection::Width)?,
        },
        _ => src_width as u32,
    };
    let height = match height {
        Some(h) if needs_resize => match is_ascii {
            true => wininfo.dim_to_cells(h, SizeDirection::Height)? * 2,
            false => wininfo.dim_to_px(h, SizeDirection::Height)?,
        },
        _ => src_height as u32,
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

    let img = image::RgbaImage::from_raw(target_width, target_height, pixmap.data().to_vec())
        .context("Failed to create image buffer from svg pixmap")?;
    let mut buf = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)?;

    if pad && (target_width != width || target_height != height) {
        let img = image::load_from_memory(&buf)?;
        let mut new_img = DynamicImage::new_rgba8(width, height);
        let x_offset = if width == target_width {
            0
        } else {
            (width - target_width) / 2
        };
        let y_offset = if height == target_height {
            0
        } else {
            (height - target_height) / 2
        };
        new_img.copy_from(&img, x_offset, y_offset)?;
        let mut cursor = std::io::Cursor::new(Vec::new());
        new_img.write_to(&mut cursor, image::ImageFormat::Png)?;
        return Ok((cursor.into_inner(), width, height));
    }

    Ok((buf, target_width, target_height))
}

pub fn exe_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    let pe = PeFile::from_bytes(bytes)?;
    let resources = pe.resources()?;

    let (_name, icon_group) = resources
        .icons()
        .next()
        .context("no icons found in exe")??;

    let best_entry = icon_group
        .entries()
        .iter()
        .max_by_key(|e| {
            let width = if e.bWidth == 0 { 256 } else { e.bWidth as u32 };
            let height = if e.bHeight == 0 {
                256
            } else {
                e.bHeight as u32
            };
            (width * height, e.wBitCount as u32)
        })
        .context("no icon entries found")?;

    let icon_data = icon_group.image(best_entry.nId)?;

    let mut ico_file = Vec::new();
    // ICO header
    ico_file.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico_file.push(best_entry.bWidth);
    ico_file.push(best_entry.bHeight);
    ico_file.push(best_entry.bColorCount);
    ico_file.push(0);
    ico_file.extend_from_slice(&best_entry.wPlanes.to_le_bytes());
    ico_file.extend_from_slice(&best_entry.wBitCount.to_le_bytes());
    ico_file.extend_from_slice(&(icon_data.len() as u32).to_le_bytes());
    ico_file.extend_from_slice(&22u32.to_le_bytes());
    ico_file.extend_from_slice(icon_data);

    Ok(image::load_from_memory(&ico_file)?)
}

pub fn lnk_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    // Rather lazy tbh, just checking for target and not to icon if set.
    // Most will likely just target an exe which we can take the icon from.

    let link_flags = u32::from_le_bytes([bytes[0x14], bytes[0x15], bytes[0x16], bytes[0x17]]);
    anyhow::ensure!(link_flags & 0x02 != 0, "lnk has no link info");

    let mut offset = 0x4C;
    if link_flags & 0x01 != 0 {
        let id_list_size = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2 + id_list_size as usize;
    }

    let local_base_path_offset = u32::from_le_bytes([
        bytes[offset + 0x10],
        bytes[offset + 0x11],
        bytes[offset + 0x12],
        bytes[offset + 0x13],
    ]) as usize;

    anyhow::ensure!(local_base_path_offset != 0, "lnk has no local base path");

    let path_offset = offset + local_base_path_offset;
    let end = bytes[path_offset..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(260);

    let target = String::from_utf8(bytes[path_offset..path_offset + end].to_vec())?;
    let target = Path::new(&target);

    anyhow::ensure!(target.exists(), "lnk target does not exist");
    anyhow::ensure!(
        target
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref()
            == Some("exe"),
        "lnk target is not an exe"
    );

    let exe_bytes = fs::read(target)?;
    exe_to_image(&exe_bytes)
}

pub fn url_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    let content = std::str::from_utf8(bytes)?;
    let icon_path = content
        .lines()
        .find_map(|line| line.strip_prefix("IconFile="))
        .map(|s| s.trim())
        .context("no IconFile entry in url file")?;

    let icon_path = Path::new(icon_path);
    anyhow::ensure!(icon_path.exists(), "icon path does not exist");

    let icon_file = McatFile::from_path(icon_path)?;
    match icon_file.kind {
        McatKind::Image => Ok(image::load_from_memory(&icon_file.bytes)?),
        McatKind::Exe => exe_to_image(&icon_file.bytes),
        _ => anyhow::bail!("unsupported icon format: {:?}", icon_file.kind),
    }
}

pub fn html_to_image(source: &McatFile) -> Result<DynamicImage> {
    let (_tmp_file, url) = if let Some(path) = &source.path {
        let url = Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("failed to create url for chromium"))?;
        (None, url)
    } else {
        let html = std::str::from_utf8(&source.bytes)?;
        let mut tmp_file = NamedTempFile::with_suffix(".html")?;
        tmp_file.write_all(html.as_bytes())?;
        let url = Url::from_file_path(tmp_file.path())
            .map_err(|_| anyhow::anyhow!("failed to create url for chromium"))?;
        (Some(tmp_file), url)
    };

    // TODO: do something about that, we don't want to recreate runtime everytime..
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let img_bytes: Vec<u8> = rt.block_on(async {
        let browser = ChromeHeadless::new(url.as_str()).await?;
        browser.capture_screenshot().await
    })?;

    Ok(image::load_from_memory(&img_bytes)?)
}
