use crate::image_extended::ResizeMode;
use base64::{engine::general_purpose, Engine};
use ffmpeg_sidecar::command::FfmpegCommand;
use image::{codecs::gif::GifDecoder, AnimationDecoder, DynamicImage, Frame, ImageBuffer, Rgb};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

pub struct InlineVideo {
    frames: Vec<Frame>,
}

pub fn is_video(input: &PathBuf) -> bool {
    let supported_extensions = [
        "mp4", "mov", "avi", "mkv", "webm", "wmv", "flv", "m4v", "ts", "gif",
    ];
    match input.extension() {
        Some(ext) => supported_extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

impl InlineVideo {
    pub fn open(input: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(input)?;
        let reader = BufReader::new(file);

        let decoder = image::codecs::gif::GifDecoder::new(reader)?;
        let frames: Vec<Frame> = decoder.into_frames().collect_frames()?;
        Ok(InlineVideo { frames })
    }

    pub fn test(input: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
        let mut file = File::open(input)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let base64_encoded = general_purpose::STANDARD.encode(&buffer);
        Ok(base64_encoded)
    }

    pub fn encode_base64(self) -> Result<String, Box<dyn std::error::Error>> {
        let mut gif_bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif_bytes);
            encoder.encode_frames(self.frames)?;
        }

        let base64_encoded = general_purpose::STANDARD.encode(&gif_bytes);

        Ok(base64_encoded)
    }

    //fn convert_video_to_frames(&self) -> Result<Vec<InlineImage>, Box<dyn std::error::Error>> {
    //    if ffmpeg_sidecar::download::auto_download().is_err() {
    //        return Err(From::from(
    //            "ffmpeg isn't installed, and the platform isn't supported for auto download",
    //        ));
    //    }
    //
    //    let iter = FfmpegCommand::new()
    //        .input(self.input.to_string_lossy().to_string())
    //        .args(&["-ignore_loop", "0"])
    //        .args(&["-vf", "fps=24"])
    //        .rawvideo()
    //        .spawn()?
    //        .iter()?;
    //
    //    let mut images = Vec::new();
    //    for frame in iter.filter_frames() {
    //        println!("processing video frame");
    //        // Create an ImageBuffer from the raw pixel data
    //        let width = frame.width as u32;
    //        let height = frame.height as u32;
    //        // Create RGB image from frame data
    //        let img_buffer = ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, frame.data)
    //            .ok_or("Failed to create image buffer")?;
    //        // Convert to DynamicImage
    //        let dynamic_img = DynamicImage::ImageRgb8(img_buffer);
    //        let (inline_img, _) =
    //            dynamic_img.resize_into_inline_img(800, 400, &ResizeMode::Fit, false)?;
    //        // Add to our collection
    //        images.push(inline_img);
    //    }
    //
    //    Ok(images)
    //}
}
