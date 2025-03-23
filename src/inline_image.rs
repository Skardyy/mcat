use std::{error::Error, fs, path::Path};

use base64::{engine::general_purpose, Engine};

use crate::image_extended::ResizeMode;

pub struct ResizeOpts {
    pub width: u16,
    pub height: u16,
    pub resize_mode: ResizeMode,
}

pub struct InlineImage {
    pub buffer: Vec<u8>,
    offset: Option<u16>,
}

impl InlineImage {
    pub fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.buffer)
    }

    /// the bytes must be of a png image.
    pub fn from_raw(buffer: Vec<u8>, offset: Option<u16>) -> Self {
        InlineImage { buffer, offset }
    }

    pub fn center(&self) -> Option<String> {
        if let Some(offset) = self.offset {
            if offset != 0 {
                return Some(format!("\x1b[{}C", offset));
            }
        }
        None
    }
}
