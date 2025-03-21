use std::io::Cursor;

use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions, Resizer};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder};

use crate::inline_image::{InlineImage, InlineImageFormat, InlineImgOpts};
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

pub trait PNGImage {
    fn into_inline_img(
        self,
        opts: InlineImgOpts,
    ) -> Result<InlineImage, Box<dyn std::error::Error>>;
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    img.write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)?;

    Ok(buffer)
}

impl PNGImage for DynamicImage {
    fn into_inline_img(
        self,
        opts: InlineImgOpts,
    ) -> Result<InlineImage, Box<dyn std::error::Error>> {
        // without resizing
        let resize_opts = match opts.resize_opts {
            Some(opts) => opts,
            None => {
                let buf = encode_png(&self)?;
                let offset = match opts.center {
                    true => center_image(self.width() as u16),
                    false => 0,
                };
                let img = InlineImage::from_raw(buf, InlineImageFormat::Png, Some(offset));
                return Ok(img);
            }
        };

        //with resizing
        let crop_opts = &ResizeOptions::new().fit_into_destination(Some((1.0_f64, 1.0_f64)));
        let (new_width, new_height, resize_opts) = match resize_opts.resize_mode {
            ResizeMode::Fit => {
                let size = calc_fit(
                    self.width() as u16,
                    self.height() as u16,
                    resize_opts.width,
                    resize_opts.height,
                );
                (size.0, size.1, None::<&ResizeOptions>)
            }
            ResizeMode::Crop => (resize_opts.width, resize_opts.height, Some(crop_opts)),
            ResizeMode::Strech => (resize_opts.width, resize_opts.height, None),
        };

        let offset = match opts.center {
            true => center_image(new_width),
            false => 0,
        };

        let mut dst_image = Image::new(
            new_width.into(),
            new_height.into(),
            self.pixel_type().ok_or("image is invalid")?,
        );
        let mut resizer = Resizer::new();
        resizer.resize(&self, &mut dst_image, resize_opts)?;

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let encoder = PngEncoder::new(&mut cursor);
        encoder.write_image(
            dst_image.buffer(),
            dst_image.width(),
            dst_image.height(),
            self.color().into(),
        )?;

        let img = InlineImage::from_raw(buffer, InlineImageFormat::Png, Some(offset));
        Ok(img)
    }
}
