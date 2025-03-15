use std::borrow::Cow;

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageResult};

use crate::image_extended::ResizeMode;

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
    pub buffer: Vec<u8>,
    offset: Option<u16>,
    pub format: InlineImageFormat,
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
