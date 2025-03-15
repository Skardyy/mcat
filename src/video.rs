use crate::{image_extended::ResizeMode, inline_image::InlineImgOpts};
use ffmpeg_sidecar::command::FfmpegCommand;
use std::{fs::File, io::Read, path::PathBuf};

pub struct InlineVideo {
    pub data: Vec<u8>,
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
    fn raw_gif_no_resizing(input: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = File::open(input)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        Ok(InlineVideo { data: buffer })
    }

    pub fn from_raw(data: Vec<u8>) -> Self {
        InlineVideo { data }
    }

    pub fn open(path: &PathBuf, opts: &InlineImgOpts) -> Result<Self, Box<dyn std::error::Error>> {
        // no resizing and is gif already
        if !opts.resize_video && path.extension().is_some_and(|f| f == "gif") {
            return InlineVideo::raw_gif_no_resizing(path);
        }
        ffmpeg_sidecar::download::auto_download()?;

        let scale = &format!("scale={}:{}", opts.width, opts.height);
        let filter = match (opts.resize_mode.clone(), opts.resize_video) {
            // ignoring crop for videos
            (ResizeMode::Fit | ResizeMode::Crop, true) => {
                &format!("{}:force_original_aspect_ratio=decrease,", scale)
            }
            (ResizeMode::Strech, true) => &format!("{},", scale),
            (_, false) => "",
        };

        let mut command = FfmpegCommand::new();
        command
            .hwaccel("auto")
            .input(path.to_string_lossy())
            .filter(format!("{}fps=24", filter))
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
