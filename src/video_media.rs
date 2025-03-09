use image::{ImageBuffer, Rgb};

use crate::media_encoder::{MediaTrait, ResizeMode};

pub struct VideoMedia {}
impl MediaTrait for VideoMedia {
    fn encode_base64(&self) -> String {
        todo!()
    }

    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        todo!()
    }

    fn resize_and_collect(&mut self, _width: u32, _height: u32, _resize_mode: ResizeMode) {
        todo!()
    }
}
