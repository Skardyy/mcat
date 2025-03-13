use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::{env, fs};

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, ImageFormat, ImageReader, ImageResult, Rgba};
use resvg::tiny_skia;
use resvg::usvg::{Options, Tree};
use std::process::Command;
use which::which;

use crate::term_misc::center_image;

#[derive(Clone)]
pub enum ResizeMode {
    Fit,
    Crop,
    Strech,
}
pub fn parse_resize_mode(resize_mode: &str) -> Option<ResizeMode> {
    match resize_mode {
        "fit" => Some(ResizeMode::Fit),
        "crop" => Some(ResizeMode::Crop),
        "strech" => Some(ResizeMode::Strech),
        _ => None,
    }
}

fn calc_fit(src_width: u16, src_height: u16, dst_width: u16, dst_height: u16) -> (u16, u16) {
    let src_ar = src_width as f32 / src_height as f32;
    let dst_ar = dst_width as f32 / dst_height as f32;

    if src_ar > dst_ar {
        // Image is wider than target: scale by width
        let scaled_height = (dst_width as f32 / src_ar).round() as u16;
        (dst_width, scaled_height)
    } else {
        // Image is taller than target: scale by height
        let scaled_width = (dst_height as f32 * src_ar).round() as u16;
        (scaled_width, dst_height)
    }
}

pub fn is_document(input: &PathBuf) -> bool {
    let supported_extensions = ["docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt"];

    match input.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

pub fn is_image(input: &PathBuf) -> bool {
    if let Some(ext) = input.extension() {
        return ImageFormat::from_extension(ext).is_some();
    }
    false
}

struct ImgCache {
    pub id: String,
}
impl ImgCache {
    pub fn new(id: String) -> Self {
        ImgCache { id }
    }
    pub fn put_cache(&self, img: &DynamicImage) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.get_cache_path();
        img.save(path)?;
        Ok(())
    }
    fn get_cache(&self) -> Result<DynamicImage, Box<dyn std::error::Error>> {
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
        let path = tmp_dir.join(&digested_name);

        path
    }
}

fn libreoffice_convert(
    input: &PathBuf,
    cache: bool,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let office_path = find_libreoffice_path().ok_or("libreoffice isn't installed or visible")?;

    let img_cache = ImgCache::new(input.to_string_lossy().to_string());
    if cache {
        if let Ok(img) = img_cache.get_cache() {
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

    Command::new(office_path)
        .arg("--headless")
        .arg("--convert-to")
        .arg("png")
        .arg(input)
        .arg("--outdir")
        .arg(tmp_dir)
        .output()?;

    if !path.exists() {
        return Err(From::from("failed to convert using libreoffice"));
    }

    //renaming for the caching
    fs::rename(path, img_cache.get_cache_path())?;
    let img = img_cache.get_cache()?;

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

pub trait DocumentReader {
    fn open_document(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>>;
    fn open_inline_image(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>>;
    fn from_png(img: &PNGImage) -> ImageResult<DynamicImage>;
}
impl DocumentReader for ImageReader<std::fs::File> {
    fn open_document(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        libreoffice_convert(input, cache)
    }

    fn open_inline_image(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        if is_image(input) {
            let img = ImageReader::open(input)?.decode()?;
            return Ok(img);
        }

        if is_document(input) {
            let img = ImageReader::open_document(input, cache)?;
            return Ok(img);
        }

        if input.extension().ok_or("file doesn't contain ext")? == "svg" {
            let img = open_svg(input)?;
            return Ok(img);
        }

        Err(From::from("file type isn't supported"))
    }

    fn from_png(img: &PNGImage) -> ImageResult<DynamicImage> {
        image::load_from_memory(&img.buffer)
    }
}

fn open_svg(path: &PathBuf) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let mut svg_file = File::open(path)?;
    let mut svg_data = Vec::new();
    svg_file.read_to_end(&mut svg_data)?;

    // Create options for parsing SVG
    let opt = Options::default();

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

pub trait InlineImage {
    fn resize_into_png(
        &self,
        width: u16,
        height: u16,
        resize_mode: &ResizeMode,
        center: bool,
    ) -> Result<(PNGImage, u16), Box<dyn std::error::Error>>;
}

impl InlineImage for DynamicImage {
    fn resize_into_png(
        &self,
        width: u16,
        height: u16,
        resize_mode: &ResizeMode,
        center: bool,
    ) -> Result<(PNGImage, u16), Box<dyn std::error::Error>> {
        let crop_opts = &ResizeOptions::new().fit_into_destination(Some((1.0 as f64, 1.0 as f64)));
        let (new_width, new_height, opts) = match resize_mode {
            ResizeMode::Fit => {
                let size = calc_fit(self.width() as u16, self.height() as u16, width, height);
                (size.0, size.1, None::<&ResizeOptions>)
            }
            ResizeMode::Crop => (width, height, Some(crop_opts)),
            ResizeMode::Strech => (width, height, None),
        };

        let offset = match center {
            true => center_image(new_width),
            false => 0,
        };

        let mut dst_image = Image::new(
            new_width.into(),
            new_height.into(),
            self.pixel_type().ok_or("image is invalid")?,
        );
        let mut resizer = Resizer::new();
        resizer.resize(self, &mut dst_image, opts)?;

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let encoder = PngEncoder::new(&mut cursor);
        encoder.write_image(
            dst_image.buffer(),
            dst_image.width(),
            dst_image.height(),
            self.color().into(),
        )?;

        let img = PNGImage { buffer };
        Ok((img, offset))
    }
}

pub struct PNGImage {
    buffer: Vec<u8>,
}
impl PNGImage {
    pub fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.buffer)
    }
}
