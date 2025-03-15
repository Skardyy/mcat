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

pub struct InlineImage {
    buffer: Vec<u8>,
    offset: u16,
}
impl InlineImage {
    pub fn encode_base64(&self) -> Cow<'_, str> {
        Cow::Owned(general_purpose::STANDARD.encode(&self.buffer))
    }

    pub fn from_raw(buffer: Vec<u8>, offset: u16) -> Self {
        InlineImage { buffer, offset }
    }

    pub fn center(&self) -> Option<String> {
        if self.offset != 0 {
            return Some(format!("\x1b[{}C", self.offset));
        }
        None
    }

    pub fn into_dyn_img(&self) -> ImageResult<DynamicImage> {
        image::load_from_memory(&self.buffer)
    }
}
