use std::{error::Error, fs, path::Path};

use base64::{engine::general_purpose, Engine};

use crate::image_extended::ResizeMode;

pub struct ResizeOpts {
    pub width: u16,
    pub height: u16,
    pub resize_mode: ResizeMode,
}
pub struct InlineImgOpts {
    pub resize_opts: Option<ResizeOpts>,
    pub center: bool,
}

pub enum InlineImageFormat {
    Gif,
    Png,
}
pub struct InlineImage {
    pub buffer: Vec<u8>,
    pub format: InlineImageFormat,
    offset: Option<u16>,
}
impl InlineImage {
    pub fn encode_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.buffer)
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

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        match self.format {
            InlineImageFormat::Gif => {
                if path.extension().is_some_and(|f| f == "gif") {
                    fs::write(path, &self.buffer)?
                } else {
                    return Err("video must be saved into a .gif file".into());
                }
            }
            InlineImageFormat::Png => {
                if path.extension().is_some_and(|f| f == "png") {
                    fs::write(path, &self.buffer)?
                } else {
                    return Err("images must be saved into a .png file".into());
                }
            }
        };

        Ok(())
    }
}
