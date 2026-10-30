use std::{
    io::{self, Write},
    process::Command,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use image::DynamicImage;
use term_misc::{EnvIdentifiers, ensure_space};

use crate::{
    error::RasterError,
    term_misc::{Wininfo, setup_signal_handler},
};

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
            RasterEncoder::Kitty { .. } => kitty_encoder::is_kitty_capable(env).is_some(),
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
            RasterEncoder::Kitty {
                unicode_placeholder_ext,
                ..
            } => !*unicode_placeholder_ext,
            RasterEncoder::Ascii => false,
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
            RasterEncoder::Kitty {
                unicode_placeholder_ext,
                ..
            } => kitty_encoder::encode_image(
                img,
                out,
                offset,
                print_at,
                wininfo,
                *unicode_placeholder_ext,
                None,
            ),
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
            // macos used to fail on this, can't reproduce, will allow macos
            RasterEncoder::Kitty {
                animation_ext: true,
                ..
            } => {
                #[cfg(all(unix, not(target_os = "android")))]
                if crossterm::tty::IsTty::is_tty(&std::io::stdout()) {
                    return unsafe {
                        kitty_encoder::encode_frames_fast(frames, out, wininfo, offset, print_at)
                    };
                }
                kitty_encoder::encode_frames(frames, out, wininfo, offset, print_at)
            }
            RasterEncoder::Kitty {
                unicode_placeholder_ext,
                ..
            } => kitty_encoder::encode_frames_redraw(
                frames,
                out,
                wininfo,
                offset,
                print_at,
                *unicode_placeholder_ext,
            ),
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
    Kitty {
        animation_ext: bool,
        unicode_placeholder_ext: bool,
    },
    Iterm,
    Sixel,
    Ascii,
}
impl RasterEncoder {
    /// Picks the best protocol for the current terminal. Falls back to Ascii.
    pub fn auto_detect(env: &EnvIdentifiers) -> Self {
        let kitty = kitty_encoder::is_kitty_capable(env);
        let weak_kitty = env.is_tmux()
            && matches!(
                kitty,
                Some(Self::Kitty {
                    unicode_placeholder_ext: false,
                    ..
                })
            );

        // kitty, normal
        if let Some(k) = kitty
            && !weak_kitty
        {
            return k;
        }
        if iterm_encoder::is_iterm_capable(env) {
            return Self::Iterm;
        }
        if sixel_encoder::is_sixel_capable(env) {
            return Self::Sixel;
        }
        // kitty without unicode ext and in tmux
        // iterm and sixel seems to work better in this case
        if let Some(k) = kitty {
            return k;
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

fn park_cursor_below(out: &mut impl Write, rows: u16) -> Result<(), RasterError> {
    write!(out, "\x1b[u\x1b[{rows}B\r")?;
    out.flush()?;
    Ok(())
}

/// Plays video by redrawing a still image in place for every frame, for protocols with no
/// native animation (iTerm2, sixel, kitty without the animation extension).
fn redraw_frames(
    frames: &mut dyn Iterator<Item = VideoFrame>,
    out: &mut impl Write,
    mut encode: impl FnMut(&DynamicImage, &mut Vec<u8>) -> Result<u16, RasterError>,
) -> Result<(), RasterError> {
    // floor on the effective frame rate, so a machine that can't keep up
    // degrades to a lower rate instead of dropping every frame
    const MIN_INTERVAL: Duration = Duration::from_millis(100);

    let shutdown = setup_signal_handler();
    let mut first = true;
    let mut reserved_rows: u16 = 0;
    let mut start = Instant::now();
    let mut last_shown = start;

    for (img, timestamp) in frames {
        if shutdown.load(Ordering::SeqCst) {
            park_cursor_below(out, reserved_rows)?;
            return Ok(());
        }

        if first {
            let mut buf = Vec::new();
            reserved_rows = encode(&img, &mut buf)?;

            ensure_space(out, reserved_rows)?;
            write!(out, "\x1b[s")?;
            out.write_all(&buf)?;
            out.flush()?;

            // frame 0 is on screen now; every later frame is scheduled against this
            start = Instant::now();
            last_shown = start;
            first = false;
            continue;
        }

        let target = start + Duration::from_secs_f32(timestamp.max(0.0));
        let now = Instant::now();

        // already overdue, and we've shown something recently enough
        if now > target && now.duration_since(last_shown) < MIN_INTERVAL {
            continue;
        }

        let mut buf = Vec::new();
        encode(&img, &mut buf)?;

        // sleep only the time left until this frame is due, not the full delay.
        // encoding already consumed part of the budget.
        if let Some(left) = target.checked_duration_since(Instant::now()) {
            std::thread::sleep(left);
        }

        write!(out, "\x1b[u")?;
        out.write_all(&buf)?;
        out.flush()?;
        last_shown = Instant::now();
    }

    if first {
        return Err(RasterError::EmptyVideo);
    }

    park_cursor_below(out, reserved_rows)?;
    Ok(())
}
