use std::path::Path;

use image::{ImageBuffer, Rgb};

use crate::media_encoder::{MediaTrait, ResizeMode};

pub fn is_video(input: &str) -> bool {
    let supported_extensions = ["gif", "mp4"];

    let path = Path::new(input);
    match path.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

pub struct VideoMedia {}
impl VideoMedia {
    pub fn new(_input: &str) -> Self {
        todo!()
    }
}
impl MediaTrait for VideoMedia {
    fn encode_base64(&self) -> String {
        todo!()
    }

    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        todo!()
    }

    fn resize_and_collect(
        &mut self,
        _width: u32,
        _height: u32,
        _resize_mode: ResizeMode,
        _center: bool,
    ) -> u32 {
        todo!()
    }
}
