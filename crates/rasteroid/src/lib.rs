use std::{
    io::{self, Write, stdout},
    process::Command,
};

use crossterm::tty::IsTty;
use image::DynamicImage;
use term_misc::{EnvIdentifiers, ensure_space};

use crate::{error::RasterError, term_misc::Wininfo};

pub mod ascii_encoder;
pub mod error;
pub mod image_extended;
pub mod iterm_encoder;
pub mod kitty_encoder;
pub mod sixel_encoder;
pub mod term_misc;

/// Writes images and video frames to a terminal using a specific graphics protocol.
pub trait Encoder {
    /// Returns `true` if this encoder's protocol is supported by the current terminal.
    fn is_capable(&self, env: &EnvIdentifiers) -> bool;
    /// Writes a single image to `out`.
    fn encode_image(
        &self,
        img: &DynamicImage,
        out: &mut impl Write,
        wininfo: &Wininfo,
        offset: Option<u16>,
        print_at: Option<(u16, u16)>,
    ) -> Result<(), RasterError>;
    /// Streams video frames to `out`. Loops forever after the first pass.
    fn encode_frames(
        &self,
        frames: &mut dyn Iterator<Item = VideoFrame>,
        out: &mut impl Write,
        wininfo: &Wininfo,
        offset: Option<u16>,
        print_at: Option<(u16, u16)>,
    ) -> Result<(), RasterError>;
}

impl Encoder for RasterEncoder {
    fn is_capable(&self, env: &EnvIdentifiers) -> bool {
        match self {
            RasterEncoder::Kitty => kitty_encoder::is_kitty_capable(env),
            RasterEncoder::Iterm => iterm_encoder::is_iterm_capable(env),
            RasterEncoder::Sixel => sixel_encoder::is_sixel_capable(env),
            RasterEncoder::Ascii => true,
        }
    }

    fn encode_image(
        &self,
        img: &DynamicImage,
        out: &mut impl Write,
        wininfo: &Wininfo,
        offset: Option<u16>,
        print_at: Option<(u16, u16)>,
    ) -> Result<(), RasterError> {
        let is_tmux = wininfo.is_tmux;
        let self_handle = match self {
            RasterEncoder::Iterm | RasterEncoder::Sixel => true,
            RasterEncoder::Kitty | RasterEncoder::Ascii => false,
        } && is_tmux;
        let mut img_cells = 0;
        if self_handle {
            img_cells = wininfo.dim_to_cells(
                &format!("{}px", img.height()),
                term_misc::SizeDirection::Height,
            )?;
            ensure_space(out, img_cells as u16)?;
        }
        match self {
            RasterEncoder::Kitty => {
                kitty_encoder::encode_image(img, out, offset, print_at, wininfo)
            }
            RasterEncoder::Iterm => {
                iterm_encoder::encode_image(img, out, offset, print_at, wininfo)
            }
            RasterEncoder::Sixel => {
                sixel_encoder::encode_image(img, out, offset, print_at, wininfo)
            }
            RasterEncoder::Ascii => ascii_encoder::encode_image(img, out, offset, print_at),
        }?;
        if self_handle {
            write!(out, "\x1B[{img_cells}B")?;
        }

        Ok(())
    }

    fn encode_frames(
        &self,
        frames: &mut dyn Iterator<Item = VideoFrame>,
        out: &mut impl Write,
        wininfo: &Wininfo,
        offset: Option<u16>,
        print_at: Option<(u16, u16)>,
    ) -> Result<(), RasterError> {
        match self {
            RasterEncoder::Kitty => match stdout().is_tty() {
                true => unsafe {
                    kitty_encoder::encode_frames_fast(frames, out, wininfo, offset, print_at)
                },
                false => kitty_encoder::encode_frames(frames, out, wininfo, offset, print_at),
            },
            // iterm gif rendering might be abit smarter, just requires to convert the frames into
            // gif, which takes time, time that now frames are rendered..
            RasterEncoder::Iterm => {
                iterm_encoder::encode_frames(frames, out, wininfo, offset, print_at)
            }
            // sixel is imo pretty bad, its slow, colors are bad too.
            // should be considered to just make it ascii frames instead.
            RasterEncoder::Sixel => {
                sixel_encoder::encode_frames(frames, out, wininfo, offset, print_at)
            }
            RasterEncoder::Ascii => {
                ascii_encoder::encode_frames(frames, out, wininfo, offset, print_at)
            }
        }
    }
}

/// Supported terminal image protocols.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RasterEncoder {
    Kitty,
    Iterm,
    Sixel,
    Ascii,
}
impl RasterEncoder {
    /// Picks the best protocol for the current terminal. Falls back to Ascii.
    pub fn auto_detect(env: &EnvIdentifiers) -> Self {
        if kitty_encoder::is_kitty_capable(env) {
            return Self::Kitty;
        }
        if iterm_encoder::is_iterm_capable(env) {
            return Self::Iterm;
        }
        if sixel_encoder::is_sixel_capable(env) {
            return Self::Sixel;
        }

        Self::Ascii
    }
}

/// Toggles tmux's `allow-passthrough` setting so graphics escapes reach the outer terminal.
pub fn set_tmux_passthrough(enabled: bool) {
    let status = if enabled { "on" } else { "off" };
    let _ = Command::new("tmux")
        .args(["set", "-g", "allow-passthrough", status])
        .status();
}

fn get_tmux_terminal_name() -> Result<(String, String), io::Error> {
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "#{client_termtype}|||#{client_termname}",
        ])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = output_str.trim().split("|||").collect();

    if parts.len() == 2 {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        Err(io::Error::other("Failed to parse tmux output"))
    }
}

/// A video frame: the image and its timestamp in seconds.
pub type VideoFrame = (DynamicImage, f32);
