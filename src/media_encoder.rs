use image::{ImageBuffer, Rgb};

use crate::photo_media::PhotoMedia;
use crate::video_media::VideoMedia;

pub enum Media {
    Video(VideoMedia),
    Photo(PhotoMedia),
}

pub enum ResizeMode {
    Fit,
    Crop,
    Strech,
}

pub trait MediaTrait {
    fn encode_base64(&self) -> String;
    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>>;
    fn resize_and_collect(&mut self, width: u32, height: u32, resize_mode: ResizeMode);
}

impl Media {
    pub fn new(input: &str) -> Self {
        Media::Photo(PhotoMedia::new(input))
    }
}

impl MediaTrait for Media {
    fn encode_base64(&self) -> String {
        match self {
            Media::Photo(pm) => pm.encode_base64(),
            Media::Video(vm) => vm.encode_base64(),
        }
    }

    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        match self {
            Media::Photo(pm) => pm.to_rgb8(),
            Media::Video(vm) => vm.to_rgb8(),
        }
    }

    fn resize_and_collect(&mut self, width: u32, height: u32, resize_mode: ResizeMode) {
        match self {
            Media::Photo(pm) => pm.resize_and_collect(width, height, resize_mode),
            Media::Video(vm) => vm.resize_and_collect(width, height, resize_mode),
        };
    }
}
