use image::{ImageBuffer, Rgb};

use crate::photo_media::{is_image, PhotoMedia};
use crate::video_media::{is_video, VideoMedia};

pub enum Media {
    Video(VideoMedia),
    Photo(PhotoMedia),
}

pub enum ResizeMode {
    Fit,
    Crop,
    Strech,
}

pub fn calc_fit(src_width: u32, src_height: u32, dst_width: u32, dst_height: u32) -> (u32, u32) {
    let src_ar = src_width as f32 / src_height as f32;
    let dst_ar = dst_width as f32 / dst_height as f32;

    if src_ar > dst_ar {
        // Image is wider than target: scale by width
        let scaled_height = (dst_width as f32 / src_ar).round() as u32;
        (dst_width, scaled_height)
    } else {
        // Image is taller than target: scale by height
        let scaled_width = (dst_height as f32 * src_ar).round() as u32;
        (scaled_width, dst_height)
    }
}

pub trait MediaTrait {
    fn encode_base64(&self) -> String;
    fn to_rgb8(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>>;
    fn resize_and_collect(&mut self, width: u32, height: u32, resize_mode: ResizeMode);
}

impl Media {
    pub fn new(input: &str, video_capable: bool) -> Self {
        let is_photo = is_image(input);
        let is_vid = is_video(input);

        if !is_vid && !is_photo {
            panic!("{} either doesn't exists, or not supported", input)
        }
        if is_vid && video_capable {
            return Media::Video(VideoMedia::new(input));
        } else {
            return Media::Photo(PhotoMedia::new(input));
        }
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
