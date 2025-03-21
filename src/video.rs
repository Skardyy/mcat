use crate::term_misc;
use ffmpeg_sidecar::command::FfmpegCommand;
use image::{codecs::gif::GifDecoder, AnimationDecoder, Frames, ImageResult};
use std::{fs::File, io::Read, path::PathBuf};

pub struct InlineVideo {
    pub data: Vec<u8>,
}

pub fn is_video(input: &PathBuf) -> bool {
    let supported_extensions = [
        "mp4", "mov", "avi", "mkv", "webm", "wmv", "flv", "m4v", "ts", "gif",
    ];

    input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| supported_extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

impl InlineVideo {
    pub fn new(data: Vec<u8>) -> Self {
        InlineVideo { data }
    }
    fn raw_gif(input: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = File::open(input)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        Ok(InlineVideo::new(buffer))
    }

    pub fn into_frames(data: &Vec<u8>) -> ImageResult<Frames> {
        let cursor = std::io::Cursor::new(data);
        let decoder = GifDecoder::new(cursor)?;
        let frames = decoder.into_frames();

        Ok(frames)
    }

    pub fn get_offset_for_center(&self, center: bool) -> ImageResult<u16> {
        let img = image::load_from_memory_with_format(&self.data, image::ImageFormat::Gif)?;
        let offset = match center {
            true => term_misc::center_image(img.width() as u16),
            false => 0,
        };

        Ok(offset)
    }

    pub fn open(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        // already a gif so no converting
        if path.extension().is_some_and(|f| f == "gif") {
            return InlineVideo::raw_gif(path);
        }

        ffmpeg_sidecar::download::auto_download()?;

        let mut command = FfmpegCommand::new();
        command
            .hwaccel("auto")
            .input(path.to_string_lossy())
            .filter("fps=24")
            .format("gif")
            .output("-");

        let mut child = command.spawn()?;

        let mut stdout = child
            .take_stdout()
            .ok_or("failed to capture ffmpeg stdout")?;
        let stderr = child.take_stderr();

        let mut output_bytes = Vec::new();
        stdout.read_to_end(&mut output_bytes)?;

        let status = child.wait()?;

        if status.success() {
            return Ok(InlineVideo { data: output_bytes });
        } else {
            let mut err_buffer = Vec::new();
            stderr
                .ok_or("failed to capture error from ffmpeg")?
                .read_to_end(&mut err_buffer)?;
            Err(From::from(format!(
                "failed ffmpeg with: <{}>\n{}",
                status,
                String::from_utf8(err_buffer)?
            )))
        }
    }
}
