use std::borrow::Cow;

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageResult};

use crate::{image_extended::ResizeMode, term_misc, video::InlineVideo};

pub struct InlineImgOpts {
    pub width: u16,
    pub height: u16,
    pub resize_mode: ResizeMode,
    pub center: bool,
    pub resize_video: bool,
}

pub enum InlineImageFormat {
    GIF,
    PNG,
}
pub struct InlineImage {
    buffer: Vec<u8>,
    offset: Option<u16>,
    format: InlineImageFormat,
}
impl InlineImage {
    pub fn encode_base64(&self) -> Cow<'_, str> {
        Cow::Owned(general_purpose::STANDARD.encode(&self.buffer))
    }

    pub fn from_raw(buffer: Vec<u8>, format: InlineImageFormat, offset: Option<u16>) -> Self {
        InlineImage {
            buffer,
            offset,
            format,
        }
    }

    pub fn add_offset(&mut self, offset: u16) {
        self.offset = Some(offset);
    }

    pub fn try_offset(&mut self) -> ImageResult<()> {
        if self.offset.is_some() {
            return Ok(());
        }

        match self.format {
            InlineImageFormat::GIF => {
                self.offset = {
                    let vid = InlineVideo::from_raw(std::mem::take(&mut self.buffer));
                    let img =
                        image::load_from_memory_with_format(&vid.data, image::ImageFormat::Gif)?;
                    let offset = term_misc::center_image(img.width() as u16);
                    self.buffer = vid.data;
                    Some(offset)
                }
            }
            InlineImageFormat::PNG => {
                self.offset = {
                    let img =
                        image::load_from_memory_with_format(&self.buffer, image::ImageFormat::Png)?;
                    let offset = term_misc::center_image(img.width() as u16);
                    Some(offset)
                }
            }
        }

        Ok(())
    }

    pub fn center(&self) -> Option<String> {
        if let Some(offset) = self.offset {
            if offset != 0 {
                return Some(format!("\x1b[{}C", offset));
            }
        }
        None
    }

    pub fn into_dyn_img(&self) -> ImageResult<DynamicImage> {
        image::load_from_memory(&self.buffer)
    }
}
