use ffmpeg_sidecar::{command::FfmpegCommand, event::OutputVideoFrame};
use image::ImageResult;
use std::{
    io::{Read, Write},
    path::Path,
};

use crate::term_misc;

pub fn is_video(input: &Path) -> bool {
    let supported_extensions = [
        "mp4", "mov", "avi", "mkv", "webm", "wmv", "flv", "m4v", "ts", "gif",
    ];

    input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| supported_extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub struct InlineStream {
    frames: Box<dyn Iterator<Item = OutputVideoFrame>>,
}

impl InlineStream {
    pub fn open(path: &Path, fps: Option<u16>) -> Result<Self, Box<dyn std::error::Error>> {
        ffmpeg_sidecar::download::auto_download()?;

        let mut command = FfmpegCommand::new();
        command.hwaccel("auto").input(path.to_string_lossy());
        if let Some(fps) = fps {
            command.filter(format!("fps={}", fps));
        }
        command.rawvideo();

        let mut child = command.spawn()?;
        let frames = child.iter()?.filter_frames();

        Ok(InlineStream {
            frames: Box::new(frames),
        })
    }

    pub fn from_raw(
        data: impl AsRef<[u8]>,
        fps: Option<u16>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        ffmpeg_sidecar::download::auto_download()?;
        let mut command = FfmpegCommand::new();

        command.hwaccel("auto").input("-");
        if let Some(fps) = fps {
            command.filter(format!("fps={}", fps));
        }
        command.rawvideo();

        let mut child = command.spawn()?;
        let mut stdin = child.take_stdin().ok_or("couldn't take stdin for ffmpeg")?;

        stdin.write_all(data.as_ref())?;
        stdin.flush()?;

        let frames = child.iter()?.filter_frames();

        Ok(InlineStream {
            frames: Box::new(frames),
        })
    }
}

pub struct InlineVideo {
    pub buffer: Vec<u8>,
}

impl InlineVideo {
    /// must be gif bytes
    pub fn from_raw(buffer: Vec<u8>) -> Self {
        InlineVideo { buffer }
    }

    pub fn open(path: &Path, fps: Option<u16>) -> Result<Self, Box<dyn std::error::Error>> {
        ffmpeg_sidecar::download::auto_download()?;

        let mut command = FfmpegCommand::new();
        command.hwaccel("auto").input(path.to_string_lossy());
        if let Some(fps) = fps {
            command.filter(format!("fps={}", fps));
        }
        command.format("gif").output("-");

        let mut child = command.spawn()?;

        let mut stdout = child
            .take_stdout()
            .ok_or("failed to capture ffmpeg stdout")?;
        let stderr = child.take_stderr();

        let mut output_bytes = Vec::new();
        stdout.read_to_end(&mut output_bytes)?;

        let status = child.wait()?;

        if status.success() {
            Ok(InlineVideo {
                buffer: output_bytes,
            })
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
