use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{env, fs};

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageBuffer, ImageFormat, ImageReader, ImageResult, Rgba};
use resvg::tiny_skia;
use resvg::usvg::{Options, Tree};
use std::process::Command;
use which::which;

use crate::image_extended::InlineImage;

struct ImgCache {
    pub id: String,
}
impl ImgCache {
    pub fn new(id: String) -> Self {
        ImgCache { id }
    }
    //pub fn put_cache(&self, img: &DynamicImage) -> Result<(), Box<dyn std::error::Error>> {
    //    let path = self.get_cache_path();
    //    img.save(path)?;
    //    Ok(())
    //}
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

fn is_document(input: &PathBuf) -> bool {
    let supported_extensions = ["docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt"];

    match input.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

fn is_image(input: &PathBuf) -> bool {
    if let Some(ext) = input.extension() {
        return ImageFormat::from_extension(ext).is_some();
    }
    false
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

pub trait DocumentReader {
    fn open_inline_image(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>>;
    fn from_png(img: &InlineImage) -> ImageResult<DynamicImage>;
}

impl DocumentReader for ImageReader<std::fs::File> {
    fn open_inline_image(
        input: &PathBuf,
        cache: bool,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        if is_image(input) {
            let img = ImageReader::open(input)?.decode()?;
            return Ok(img);
        }

        if is_document(input) {
            let img = libreoffice_convert(input, cache)?;
            return Ok(img);
        }

        if input.extension().ok_or("file doesn't contain ext")? == "svg" {
            let img = open_svg(input)?;
            return Ok(img);
        }

        Err(From::from("file type isn't supported"))
    }

    fn from_png(img: &InlineImage) -> ImageResult<DynamicImage> {
        image::load_from_memory(&img.buffer)
    }
}
