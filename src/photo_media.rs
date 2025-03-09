use std::io::Cursor;

use base64::{engine::general_purpose, Engine};
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, ImageReader, Rgb};

use crate::media_encoder::{MediaTrait, ResizeMode};

pub struct PhotoMedia {
    img: DynamicImage,
    resized_img: Vec<u8>,
}
impl PhotoMedia {
    pub fn new(input: &str) -> Self {
        let img = ImageReader::open(input).unwrap().decode().unwrap();

        PhotoMedia {
            img,
            resized_img: vec![],
        }
    }
}
impl MediaTrait for PhotoMedia {
    fn resize_and_collect(&mut self, width: u32, height: u32, _resize_mode: ResizeMode) {
        let mut dst_image = Image::new(width, height, self.img.pixel_type().unwrap());
        let mut resizer = Resizer::new();
        resizer.resize(&self.img, &mut dst_image, None).unwrap();

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
            .unwrap();

        self.resized_img = buffer;
    }
    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let img = image::load_from_memory(&self.resized_img).unwrap();
        img.to_rgb8()
    }

    fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.resized_img)
    }
}
