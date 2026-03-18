use std::{
    io::{self, Write},
    process::Command,
};

use image::load_from_memory;
use term_misc::{EnvIdentifiers, ensure_space};

pub mod ascii_encoder;
pub mod image_extended;
pub mod iterm_encoder;
pub mod kitty_encoder;
pub mod sixel_encoder;
pub mod term_misc;

/// encode an image bytes into inline image using the given encoder
/// # example:
/// ```
/// use std::path::Path;
/// use rasteroid::InlineEncoder;
/// use rasteroid::inline_an_image;
/// use std::io::Write;
/// use rasteroid::term_misc::EnvIdentifiers;
///
/// let path = Path::new("image.png");
/// let bytes = match std::fs::read(path) {
///     Ok(bytes) => bytes,
///     Err(e) => return,
/// };
/// let mut stdout = std::io::stdout();
/// let mut env = EnvIdentifiers::new();
/// let encoder = InlineEncoder::auto_detect(true, false, false, false, &mut env); // force kitty as fallback
/// inline_an_image(&bytes, &mut stdout, None, None, &encoder).unwrap();
/// stdout.flush().unwrap();
/// ```
/// MENTION: it should work for Iterm Gifs too.
pub fn inline_an_image(
    img: &[u8],
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
    inline_encoder: &InlineEncoder,
) -> Result<(), Box<dyn std::error::Error>> {
    let is_tmux = term_misc::get_wininfo().is_tmux;
    let self_handle = match inline_encoder {
        InlineEncoder::Iterm | InlineEncoder::Sixel => true,
        InlineEncoder::Kitty | InlineEncoder::Ascii => false,
    } && is_tmux;
    let mut img_cells = 0;
    if self_handle {
        let img_px = load_from_memory(img)?.height();
        img_cells =
            term_misc::dim_to_cells(&format!("{img_px}px"), term_misc::SizeDirection::Height)?;
        ensure_space(out, img_cells as u16)?;
    }
    match inline_encoder {
        InlineEncoder::Kitty => kitty_encoder::encode_image(img, out, offset, print_at),
        InlineEncoder::Iterm => iterm_encoder::encode_image(img, out, offset, print_at),
        InlineEncoder::Sixel => sixel_encoder::encode_image(img, out, offset, print_at),
        InlineEncoder::Ascii => ascii_encoder::encode_image(img, out, offset, print_at),
    }?;
    if self_handle {
        write!(out, "\x1B[{img_cells}B")?;
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum InlineEncoder {
    Kitty,
    Iterm,
    Sixel,
    #[default]
    Ascii,
}
impl InlineEncoder {
    /// auto detect which Encoder works for the current terminal
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
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to parse tmux output",
        ))
    }
}

pub trait Frame {
    fn timestamp(&self) -> f32;
    fn data(&self) -> &[u8];
    fn width(&self) -> u16;
    fn height(&self) -> u16;
}
