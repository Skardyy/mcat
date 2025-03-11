use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::{env, fs};

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, ImageReader, Rgb};
use std::process::Command;
use which::which;

use crate::media_encoder::{calc_fit, MediaTrait, ResizeMode};
use crate::term_misc::center_image;

pub fn is_image(input: &str) -> bool {
    let supported_extensions = [
        "avif", "bmp", "dds", "farbfeld", "gif", "hdr", "ico", "jpeg", "jpg", "exr", "png", "pnm",
        "qoi", "tga", "tiff", "webp", "svg",
    ];

    let path = Path::new(input);
    match path.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}
pub fn is_document(input: &str) -> bool {
    let supported_extensions = [
        "docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt", "html",
    ];

    let path = Path::new(input);
    match path.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

pub fn find_libreoffice_path() -> Option<PathBuf> {
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

pub fn get_document(input: &str, cache: bool) -> Option<DynamicImage> {
    let office_path = find_libreoffice_path()?;

    // for caching we'll rename and check if still exists in the tmp dir.
    let mut cache_name = env::temp_dir().join(Path::new(input).file_name().unwrap());
    let digested_stem =
        general_purpose::URL_SAFE.encode(cache_name.to_string_lossy().as_bytes()) + ".png";
    cache_name.set_file_name(digested_stem.clone());
    if cache && cache_name.exists() {
        return Some(
            ImageReader::open(cache_name)
                .expect(&format!("failed to open: {}", input))
                .decode()
                .expect("failed to parse the image"),
        );
    }

    // where the file will be located
    let tmp_dir = env::temp_dir();
    let input_path = Path::new(input);
    let base_name = input_path
        .with_extension("png")
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let img_path = tmp_dir.join(base_name);
    let path = img_path.to_string_lossy();

    // html sadly requires extra step to pdf before png
    let extra_step = input.contains(".html");
    if extra_step {
        Command::new(office_path.clone())
            .arg("--headless")
            .arg("--convert-to")
            .arg("pdf")
            .arg(input)
            .arg("--outdir")
            .arg(tmp_dir.clone())
            .output()
            .unwrap();
    }
    let input = match extra_step {
        true => &path.replace(".png", ".pdf"),
        false => input,
    };

    let output = Command::new(office_path)
        .arg("--headless")
        .arg("--convert-to")
        .arg("png")
        .arg(input)
        .arg("--outdir")
        .arg(tmp_dir)
        .output()
        .unwrap();
    // stderr contains something, means failed
    if output.stderr.len() > 0 {
        let msg = String::from_utf8(output.stderr)
            .unwrap_or("failed to convert using libreoffice".to_string());
        panic!("{}", msg);
    }

    //renaming for the caching
    fs::rename(path.to_string(), cache_name.clone()).expect("failed caching libreoffice convert");
    let img = ImageReader::open(cache_name)
        .expect(&format!("failed to open: {}", input))
        .decode()
        .expect("failed to parse the image");

    Some(img)
}

pub struct PhotoMedia {
    img: DynamicImage,
    resized_img: Vec<u8>,
}
impl PhotoMedia {
    pub fn new(input: &str, cache: bool) -> Self {
        let img = match is_document(input) {
            true => get_document(input, cache).expect("libreoffice is required for doucment files"),
            false => ImageReader::open(input)
                .expect(&format!("failed to open: {}", input))
                .decode()
                .expect("failed to parse the image"),
        };

        PhotoMedia {
            img,
            resized_img: vec![],
        }
    }
}
impl MediaTrait for PhotoMedia {
    fn resize_and_collect(
        &mut self,
        width: u32,
        height: u32,
        resize_mode: ResizeMode,
        center: bool,
    ) -> u32 {
        let crop_opts = &ResizeOptions::new().fit_into_destination(Some((1.0 as f64, 1.0 as f64)));
        let (new_width, new_height, opts) = match resize_mode {
            ResizeMode::Fit => {
                let size = calc_fit(self.img.width(), self.img.height(), width, height);
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
            new_width,
            new_height,
            self.img.pixel_type().expect("image is invalid"),
        );
        let mut resizer = Resizer::new();
        resizer
            .resize(&self.img, &mut dst_image, opts)
            .expect("failed to resize image");

        // converting to vec
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let encoder = PngEncoder::new(&mut cursor);
        encoder
            .write_image(
                dst_image.buffer(),
                dst_image.width(),
                dst_image.height(),
                self.img.color().into(),
            )
            .expect("failed to encode the resized image");

        self.resized_img = buffer;

        offset
    }
    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let img =
            image::load_from_memory(&self.resized_img).expect("failed to read image from memory");
        img.to_rgb8()
    }

    fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.resized_img)
    }
}
