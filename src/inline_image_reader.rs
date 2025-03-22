use std::borrow::Cow;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{env, fs};

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageBuffer, ImageFormat, ImageReader, Rgba};
use pulldown_cmark::{html, Parser};
use resvg::usvg::{Options, Tree};
use resvg::{tiny_skia, usvg};
use std::process::Command;
use which::which;

use crate::image_extended::PNGImage;
use crate::inline_image::{self, InlineImage, InlineImgOpts};
use crate::term_misc::{Filters, RotateFilter};
use crate::url_query::handle_url;
use crate::video::{is_video, InlineVideo};

pub struct ImgCache<'a> {
    pub id: Cow<'a, str>,
}
impl ImgCache<'_> {
    pub fn new(id: Cow<'_, str>, is_path: bool) -> Self {
        let id = if is_path {
            let path = Path::new(&id); // Directly using &id, no need for `&str`
            match path.canonicalize() {
                Ok(path) => Cow::Owned(path.to_string_lossy().into_owned()), // Convert to Cow::Owned
                Err(_) => id,
            }
        } else {
            id
        };

        ImgCache { id }
    }
}
    pub fn get_png(&self) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let path = self.get_cache_path();
        if !path.exists() {
            return Err(From::from("cache doesn't exists"));
        }

        let img = ImageReader::open(path)?.decode()?;
        Ok(img)
    }
    pub fn get_cache_path(&self) -> PathBuf {
        let tmp_dir = env::temp_dir();
        let digested_name = general_purpose::URL_SAFE.encode(&self.id) + ".png";
        tmp_dir.join(&digested_name)
    }
}

fn is_document(input: &Path) -> bool {
    let supported_extensions = [
        "docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt", "html", "md",
    ];

    input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| supported_extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_image(input: &Path) -> bool {
    if let Some(ext) = input.extension() {
        return ImageFormat::from_extension(ext).is_some();
    }
    false
}

fn convert_md(markdown: &str, cache: bool) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let parser = Parser::new(markdown);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    let l = html_output.len().min(15);
    let img_cache = ImgCache::new(html_output[..15], false);

    todo!()
}

fn libreoffice_convert(
    input: &Path,
    cache: bool,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let office_path = find_libreoffice_path().ok_or("libreoffice isn't installed or visible")?;

    let img_cache = ImgCache::new(input.to_string_lossy(), true);
    if cache {
        if let Ok(img) = img_cache.get_png() {
            return Ok(img);
        }
    }

    // where the file will be located
    let tmp_dir = env::temp_dir();
    let base_name = input
        .with_extension("png")
        .file_name()
        .ok_or("failed to get filename")?
        .to_string_lossy()
        .to_string();
    let path = tmp_dir.join(base_name);

    let is_html = input.extension().is_some_and(|f| f == "html");
    let filter = if is_html {
        "png:writer_png_Export"
    } else {
        "png"
    };

    Command::new(office_path)
        .arg("--headless")
        .arg("--norestore")
        .arg("--convert-to")
        .arg(filter)
        .arg(input)
        .arg("--outdir")
        .arg(tmp_dir)
        .output()?;

    if !path.exists() {
        return Err(From::from("failed to convert using libreoffice"));
    }

    //renaming for the caching
    fs::rename(path, img_cache.get_cache_path())?;
    let img = img_cache.get_png()?;

    Ok(img)
}

fn find_libreoffice_path() -> Option<PathBuf> {
    let paths = [
        "C:\\Program Files\\LibreOffice\\program\\soffice.com",
        "/usr/bin/libreoffice",
    ];
    for path in paths {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    let names = ["soffice", "libreoffice"];
    for name in names {
        if let Ok(path) = which(name) {
            return Some(path);
        }
    }

    None
}

pub fn load_svg<R>(mut reader: R) -> Result<DynamicImage, Box<dyn std::error::Error>>
where
    R: Read,
{
    let mut svg_data = Vec::new();
    reader.read_to_end(&mut svg_data)?;

    // Create options for parsing SVG
    let mut opt = Options::default();

    // allowing text
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    opt.fontdb = std::sync::Arc::new(fontdb);
    opt.text_rendering = usvg::TextRendering::OptimizeLegibility;

    // Parse SVG
    let tree = Tree::from_data(&svg_data, &opt)?;

    // Get size of the SVG
    let pixmap_size = tree.size();
    let width = pixmap_size.width();
    let height = pixmap_size.height();

    // Create a Pixmap to render to
    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32)
        .ok_or("Failed to create pixmap for svg")?;

    // Render SVG to Pixmap
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // Convert Pixmap to ImageBuffer
    let image_buffer =
        ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, pixmap.data().to_vec())
            .ok_or("Failed to create image buffer for svg")?;

    // Convert ImageBuffer to DynamicImage
    Ok(DynamicImage::ImageRgba8(image_buffer))
}

pub struct InlineImgReader {}

pub fn apply_filters(img: &mut DynamicImage, filter: &Filters) {
    if let Some(contrast) = filter.contrast {
        *img = img.adjust_contrast(contrast);
    }

    if let Some(hue_degrees) = filter.hue_rotate {
        *img = img.huerotate(hue_degrees);
    }

    if let Some((sigma, threshold)) = filter.unsharpen {
        *img = img.unsharpen(sigma, threshold);
    }

    if let Some(brighten) = filter.brighten {
        *img = img.brighten(brighten);
    }

    if filter.grayscale {
        *img = img.grayscale();
    }

    if let Some(rotate_filter) = &filter.rotate {
        *img = match rotate_filter {
            RotateFilter::Rotate90 => img.rotate90(),
            RotateFilter::Rotate180 => img.rotate180(),
            RotateFilter::Rotate270 => img.rotate270(),
        };
    }

    if filter.invert_colors {
        img.invert();
    }

    if let Some(blur_sigma) = filter.blur {
        *img = img.fast_blur(blur_sigma);
    }
}

impl InlineImgReader {
    /// will return err when saving, string will be "file saved"
    /// can also return err for other things.
    pub fn open(
        path: &Path,
        cache: bool,
        try_video: bool,
        opts: InlineImgOpts,
        filter: Option<&Filters>,
    ) -> Result<InlineImage, Box<dyn Error>> {
        if !path.exists() {
            return Err(From::from("file doesn't exists"));
        }

        let mut img_opt: Option<DynamicImage> = None;

        // ffmpeg supported videos
        if try_video && is_video(path) {
            let vid = InlineVideo::open(path)?;
            let offset = vid.get_offset_for_center(opts.center)?;
            let inline_img =
                InlineImage::from_raw(vid.data, inline_image::InlineImageFormat::Gif, Some(offset));
            return Ok(inline_img);
        }
        // image crate supported files
        if is_image(path) {
            img_opt = Some(ImageReader::open(path)?.decode()?);
        }
        // svg
        if path.extension().ok_or("file doesn't contain ext")? == "svg" {
            let file = File::open(path)?;
            img_opt = Some(load_svg(file)?);
        }
        // libreoffice documents
        if is_document(path) {
            img_opt = Some(libreoffice_convert(path, cache)?);
        }

        let mut img = img_opt.ok_or("file type isn't supported")?;

        // applying filters
        if let Some(filter) = filter {
            apply_filters(&mut img, filter);
        }

        let img = img.into_inline_img(opts)?;
        Ok(img)
    }

    /// will return err when saving, string will be "file saved"
    /// can also return err for other things.
    pub fn from_url(
        url: &str,
        try_video: bool,
        opts: InlineImgOpts,
        filter: Option<&Filters>,
    ) -> Result<InlineImage, Box<dyn Error>> {
        let img = handle_url(url, opts, try_video, filter)?;
        Ok(img)
    }
}
