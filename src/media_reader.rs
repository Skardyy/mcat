use std::error::Error;
use std::fs::File;
use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageReader};

use crate::converters;
use crate::image_extended::PNGImage;
use crate::inline_image::ResizeOpts;
use crate::inline_video::{is_video, InlineStream, InlineVideo};
use crate::media::Media;
use crate::term_misc::Filters;

fn is_image(input: &Path) -> bool {
    if let Some(ext) = input.extension() {
        return ImageFormat::from_extension(ext).is_some();
    }
    false
}

pub struct MediaReader {}

#[derive(PartialEq)]
pub enum Encoder {
    Kitty,
    Iterm,
    Sixel,
}

impl Encoder {
    pub fn from_string(value: &str) -> Option<Self> {
        match value {
            "kitty" => Some(Self::Kitty),
            "iterm" => Some(Self::Iterm),
            "sixel" => Some(Self::Sixel),
            _ => None,
        }
    }
}

impl MediaReader {
    pub fn open(
        path: &Path,
        cache: bool,
        encoder: Encoder,
        center: bool,
        resize: Option<ResizeOpts>,
        filter: Option<Filters>,
    ) -> Result<Media, Box<dyn Error>> {
        if !path.exists() {
            return Err(From::from("file doesn't exists"));
        }

        let mut img_opt: Option<DynamicImage> = None;

        // ffmpeg supported videos
        if is_video(path) {
            let media = match encoder {
                Encoder::Kitty => {
                    let stream = InlineStream::open(path, Some(24))?;
                    Some(Media::Stream(stream))
                }
                Encoder::Iterm => {
                    let vid = InlineVideo::open(path, Some(24))?;
                    Some(Media::Video(vid))
                }
                Encoder::Sixel => None,
            };

            if let Some(media) = media {
                return Ok(media);
            }
        }
        // image crate supported files
        if is_image(path) {
            img_opt = Some(ImageReader::open(path)?.decode()?);
        }
        // svg
        if path.extension().ok_or("file doesn't contain ext")? == "svg" {
            let file = File::open(path)?;
            img_opt = Some(converters::svg::load_svg(file)?);
        }
        // libreoffice documents
        if converters::libreoffice::is_document(path) {
            img_opt = Some(converters::libreoffice::open_document(path, cache)?);
        }

        let mut img = img_opt.ok_or("file type isn't supported")?;

        // applying filters
        if let Some(filter) = filter {
            img.apply_filters(filter);
        }

        let img = img.into_inline_img(resize, center)?;
        Ok(Media::Image(img))
    }

    // pub fn from_url(
    //     url: &str,
    //     try_video: bool,
    //     opts: InlineImgOpts,
    //     filter: Option<&Filters>,
    // ) -> Result<InlineImage, Box<dyn Error>> {
    //     let img = handle_url(url, opts, try_video, filter)?;
    //     Ok(img)
    // }
}
