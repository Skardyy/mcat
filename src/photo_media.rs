use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, ImageReader, Rgb};

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

pub struct PhotoMedia {
    img: DynamicImage,
    resized_img: Vec<u8>,
}
impl PhotoMedia {
    pub fn new(input: &str) -> Self {
        let img = ImageReader::open(input)
            .expect(&format!("failed to open: {}", input))
            .decode()
            .expect("failed to parse the image");

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
