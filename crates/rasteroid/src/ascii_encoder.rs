use image::DynamicImage;

use crate::{
    VideoFrame,
    error::RasterError,
    image_extended::InlineImage,
    term_misc::{self, Wininfo},
};
use std::io::{BufRead, Write};

pub fn encode_image(
    img: &DynamicImage,
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    let rgba_image = img.to_rgba8();

    let w = rgba_image.width() as usize;
    let h = rgba_image.height() as usize;
    let h_adjusted = if h % 2 == 1 { h - 1 } else { h };

    // Luminance threshold: tweak this to suppress small sparkles
    const LUM_THRESHOLD: f32 = 35.0;

    let mut last_max_height = 0;
    for y in (0..h_adjusted).step_by(2) {
        if let Some(at) = print_at {
            let at = (at.0, at.1 + (y / 2) as u16);
            last_max_height = y;
            let loc = term_misc::loc_to_terminal(Some(at));
            out.write_all(loc.as_ref())?;
        }
        if let Some(off) = offset {
            let center = term_misc::offset_to_terminal(Some(off));
            out.write_all(center.as_ref())?;
        }

        for x in 0..w {
            let upper = rgba_image.get_pixel(x as u32, y as u32);
            let lower = rgba_image.get_pixel(x as u32, (y + 1) as u32);

            let (ru, gu, bu, au) = (upper[0], upper[1], upper[2], upper[3]);
            let (rl, gl, bl, al) = (lower[0], lower[1], lower[2], lower[3]);

            const MIN_VISUAL_WEIGHT: f32 = 25.0; // tweak for strictness

            let upper_w = visual_weight(ru, gu, bu, au);
            let lower_w = visual_weight(rl, gl, bl, al);

            let upper_visible = upper_w > MIN_VISUAL_WEIGHT;
            let lower_visible = lower_w > MIN_VISUAL_WEIGHT;

            match (upper_visible, lower_visible) {
                (false, false) => {
                    out.write_all(b" ")?;
                }
                (true, false) => {
                    write!(out, "\x1b[38;2;{};{};{}m▀\x1b[0m", ru, gu, bu)?;
                }
                (false, true) => {
                    write!(out, "\x1b[38;2;{};{};{}m▄\x1b[0m", rl, gl, bl)?;
                }
                (true, true) => {
                    // Use dual-color upper/lower block
                    write!(
                        out,
                        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀\x1b[0m",
                        ru, gu, bu, rl, gl, bl
                    )?;
                }
            }
        }

        out.write_all(b"\n")?;
    }

    if h % 2 == 1 {
        if let Some(at) = print_at {
            let add_y = (last_max_height + 2) / 2;
            let at = (at.0, at.1 + add_y as u16);
            let loc = term_misc::loc_to_terminal(Some(at));
            out.write_all(loc.as_ref())?;
        }
        if let Some(off) = offset {
            let center = term_misc::offset_to_terminal(Some(off));
            out.write_all(center.as_ref())?;
        }

        for x in 0..w {
            let p = rgba_image.get_pixel(x as u32, (h - 1) as u32);
            let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
            let lum = luminance(r, g, b);

            if a == 0 || lum < LUM_THRESHOLD {
                out.write_all(b" ")?;
            } else {
                write!(out, "\x1b[38;2;{};{};{}m▀\x1b[0m", r, g, b)?;
            }
        }

        out.write_all(b"\n")?;
    }

    out.write_all(b"\x1b[0m")?;
    Ok(())
}

fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

fn visual_weight(r: u8, g: u8, b: u8, a: u8) -> f32 {
    if a == 0 {
        0.0
    } else {
        luminance(r, g, b) * (a as f32 / 255.0)
    }
}

pub fn encode_frames(
    frames: &mut dyn Iterator<Item = VideoFrame>,
    mut out: impl Write,
    wininfo: &Wininfo,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    crate::redraw_frames(frames, &mut out, |img, buf| {
        let resized = img.resize_plus(wininfo, Some("80%"), Some("40%"), true, false)?;
        encode_image(&resized, buf, offset, print_at)?;
        Ok(buf.as_slice().lines().count() as u16)
    })
}
