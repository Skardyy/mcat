use std::io::Cursor;

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, Resizer};
use image::codecs::png::PngEncoder;
use image::{ImageBuffer, ImageEncoder, ImageReader, Rgb};

pub struct Media {
    pub path: String,
    img: Vec<u8>,
}

impl Media {
    pub fn new(path: &str, width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        // getting the img
        let img = ImageReader::open(path)?.decode()?;

        // resizing it
        let mut dst_image = Image::new(width, height, img.pixel_type().unwrap());
        let mut resizer = Resizer::new();
        resizer.resize(&img, &mut dst_image, None).unwrap();

        // converting to vec
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let encoder = PngEncoder::new(&mut cursor);
        encoder.write_image(
            dst_image.buffer(),
            dst_image.width(),
            dst_image.height(),
            img.color().into(),
        )?;

        Ok(Media {
            path: path.to_string(),
            img: buffer,
        })
    }

    pub fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let img = image::load_from_memory(&self.img).unwrap();
        img.to_rgb8()
    }

    pub fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.img)
    }
}
