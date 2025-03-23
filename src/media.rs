use std::{error::Error, fs, path::Path};

use crate::{
    inline_image::InlineImage,
    inline_video::{InlineStream, InlineVideo},
};

pub enum Media {
    Video(InlineVideo),
    Stream(InlineStream),
    Image(InlineImage),
}

impl Media {
    pub fn save(self, path: &Path) -> Result<(), Box<dyn Error>> {
        match self {
            Media::Video(inline_video) => {
                if path.extension().is_some_and(|f| f == "gif") {
                    fs::write(path, inline_video.buffer)?
                } else {
                    return Err("videos must be saved into a .gif file".into());
                }
            }
            Media::Stream(inline_stream) => todo!(),
            Media::Image(inline_image) => {
                if path.extension().is_some_and(|f| f == "png") {
                    fs::write(path, inline_image.buffer)?
                } else {
                    return Err("images must be saved into a .png file".into());
                }
            }
        };

        Ok(())
    }
}
