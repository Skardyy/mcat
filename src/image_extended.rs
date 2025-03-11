use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::{env, fs};

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, ImageReader, ImageResult, Rgba};
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
    let supported_extensions = [
        "docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt", "html",
    ];

    match input.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

pub fn is_image(input: &PathBuf) -> bool {
    let supported_extensions = [
        "avif", "bmp", "dds", "farbfeld", "gif", "hdr", "ico", "jpeg", "jpg", "exr", "png", "pnm",
        "qoi", "tga", "tiff", "webp",
    ];

    let path = Path::new(input);
    match path.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
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
    fn from_png(img: PNGImage) -> ImageResult<DynamicImage>;
}
impl DocumentReader for ImageReader<std::fs::File> {
    fn open_document(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let office_path = find_libreoffice_path().ok_or("libreoffice isn't installed")?;

        // for caching we'll rename and check if still exists in the tmp dir.
        let mut cache_name =
            env::temp_dir().join(input.file_name().ok_or("failed to get tmp dir")?);
        let digested_stem =
            general_purpose::URL_SAFE.encode(cache_name.to_string_lossy().as_bytes()) + ".png";
        cache_name.set_file_name(digested_stem.clone());
        if cache && cache_name.exists() {
            if let Ok(img) = ImageReader::open(&cache_name) {
                if let Ok(img) = img.decode() {
                    return Ok(img);
                }
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

        // html sadly requires extra step to pdf before png
        let extra_step = input.extension().ok_or("file doesn't contain ext")? == "html";
        if extra_step {
            Command::new(office_path.clone())
                .arg("--headless")
                .arg("--convert-to")
                .arg("pdf")
                .arg(input.clone())
                .arg("--outdir")
                .arg(tmp_dir.clone())
                .output()?;
        }
        let input = match extra_step {
            true => &path.with_extension("pdf"),
            false => &input.clone(),
        };

        let output = Command::new(office_path)
            .arg("--headless")
            .arg("--convert-to")
            .arg("png")
            .arg(input)
            .arg("--outdir")
            .arg(tmp_dir)
            .output()?;
        // stderr contains something, means failed
        if output.stderr.len() > 0 {
            let msg = String::from_utf8(output.stderr)
                .unwrap_or("failed to convert using libreoffice".to_string());
            return Err(From::from(msg));
        }

        //renaming for the caching
        fs::rename(path, cache_name.clone()).expect("failed caching libreoffice convert");
        let img = ImageReader::open(cache_name)?.decode()?;

        Ok(img)
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

    fn from_png(img: PNGImage) -> ImageResult<DynamicImage> {
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
