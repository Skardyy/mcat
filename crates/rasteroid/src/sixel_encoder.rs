use crate::{
    VideoFrame,
    error::RasterError,
    term_misc::{self, EnvIdentifiers, Wininfo, loc_to_terminal, offset_to_terminal},
};
use color_quant::NeuQuant;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::{io::Write, sync::atomic::Ordering, time::Duration};

const SIXEL_MIN: u8 = 0x3f;

pub fn encode_image(
    img: &DynamicImage,
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
    wininfo: &Wininfo,
) -> Result<(), RasterError> {
    let rgb_img = img.to_rgb8();

    let center = offset_to_terminal(offset);
    let print_at_string = loc_to_terminal(print_at);
    out.write_all(print_at_string.as_ref())?;
    out.write_all(center.as_ref())?;

    encode_sixel(out, &rgb_img, wininfo.is_tmux)?;

    Ok(())
}

pub fn is_sixel_capable(env: &EnvIdentifiers) -> bool {
    // has way more support, i just think sixel is bad
    env.term_contains("foot") || env.has_key("WT_PROFILE_ID") || env.term_contains("sixel-tmux")
}

fn encode_sixel(
    out: &mut impl Write,
    img: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    is_tmux: bool,
) -> Result<(), RasterError> {
    let width = img.width() as usize;
    let height = img.height() as usize;

    if width == 0 || height == 0 {
        return Err(RasterError::EmptyImage);
    }

    let prefix = if is_tmux {
        "\x1bPtmux;\x1b\x1b"
    } else {
        "\x1b"
    };
    let suffix = if is_tmux {
        "\x1b\x1b\\\x1b\\"
    } else {
        "\x1b\\"
    };

    write!(out, "{prefix}P0;1q\"1;1;{};{}", width, height)?;

    let pixels: Vec<u8> = img
        .pixels()
        .flat_map(|p| [p.0[0], p.0[1], p.0[2], 255])
        .collect();
    let nq = NeuQuant::new(10, 256, &pixels);
    let palette_vec: Vec<(u8, u8, u8)> = nq
        .color_map_rgb()
        .chunks(3)
        .map(|c| (c[0], c[1], c[2]))
        .collect();
    let palette = &palette_vec;
    let color_indices = map_to_palette(img, &nq);

    for (i, &(r, g, b)) in palette.iter().enumerate() {
        let r_pct = (r as f32 / 255.0 * 100.0) as u8;
        let g_pct = (g as f32 / 255.0 * 100.0) as u8;
        let b_pct = (b as f32 / 255.0 * 100.0) as u8;

        write!(out, "#{};2;{};{};{}", i, r_pct, g_pct, b_pct)?;
    }
    let palette_size = palette.len();
    let mut color_used = vec![false; palette_size];
    let mut sixel_data = vec![0u8; width * palette_size];

    let sixel_rows = height.div_ceil(6);
    for row in 0..sixel_rows {
        if row > 0 {
            write!(out, "-")?;
        }

        color_used.fill(false);
        sixel_data.fill(0);

        for p in 0..6 {
            let y = (row * 6) + p;
            if y >= height {
                break;
            }

            for x in 0..width {
                let color_idx = color_indices[y * width + x] as usize;
                color_used[color_idx] = true;
                sixel_data[(width * color_idx) + x] |= 1 << p;
            }
        }

        let mut first_color_written = false;
        for n in 0..palette_size {
            if !color_used[n] {
                continue;
            }

            if first_color_written {
                write!(out, "$")?;
            }

            write!(out, "#{}", n)?;

            let mut rle_count = 0;
            let mut prev_sixel = 255;

            for x in 0..width {
                let next_sixel = sixel_data[(n * width) + x];

                if prev_sixel != 255 && next_sixel != prev_sixel {
                    write_gri(out, rle_count, prev_sixel)?;
                    rle_count = 0;
                }

                prev_sixel = next_sixel;
                rle_count += 1;
            }

            write_gri(out, rle_count, prev_sixel)?;

            first_color_written = true;
        }
    }

    out.write_all(suffix.as_bytes())?;

    Ok(())
}

fn map_to_palette(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, nq: &NeuQuant) -> Vec<u8> {
    img.pixels()
        .map(|p| nq.index_of(&[p.0[0], p.0[1], p.0[2], 255]) as u8)
        .collect()
}

fn write_gri<W: Write>(out: &mut W, repeat_count: usize, sixel: u8) -> Result<(), RasterError> {
    if repeat_count == 0 {
        return Ok(());
    }

    let sixel = SIXEL_MIN + (sixel & 0b111111);

    if repeat_count > 3 {
        write!(out, "!{}{}", repeat_count, sixel as char)?;
    } else {
        for _ in 0..repeat_count {
            write!(out, "{}", sixel as char)?;
        }
    }

    Ok(())
}

pub fn encode_frames(
    frames: &mut dyn Iterator<Item = VideoFrame>,
    out: &mut impl Write,
    wininfo: &Wininfo,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    let shutdown = term_misc::setup_signal_handler();
    let mut last_timestamp: Option<f32> = None;
    let mut frame_cache: Vec<(Vec<u8>, Duration)> = Vec::new();
    let (first_img, _) = frames.next().ok_or(RasterError::EmptyVideo)?;

    let at = print_at.unwrap_or((0, 0));

    // pre-encode first frame and ensure space
    let mut first_buf = Vec::new();
    encode_image(&first_img, &mut first_buf, offset, Some(at), wininfo)?;
    term_misc::ensure_space(out, first_img.height() as u16)?;
    out.write_all(&first_buf)?;
    out.flush()?;

    let delay = Duration::from_millis(33);
    frame_cache.push((first_buf, delay));

    for (img, timestamp) in frames {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }

        let delay = match (timestamp, last_timestamp) {
            (ts, Some(last)) if ts > last => Duration::from_secs_f32(ts - last),
            _ => Duration::from_millis(33),
        };
        last_timestamp = Some(timestamp);

        let mut buf = Vec::new();
        encode_image(&img, &mut buf, offset, Some(at), wininfo)?;

        out.write_all(&buf)?;
        out.flush()?;
        frame_cache.push((buf, delay));
        std::thread::sleep(delay);
    }

    if frame_cache.is_empty() {
        return Err(RasterError::EmptyVideo);
    }

    // loop cached frames
    loop {
        for (buf, delay) in &frame_cache {
            if shutdown.load(Ordering::SeqCst) {
                return Ok(());
            }
            out.write_all(buf)?;
            out.flush()?;
            std::thread::sleep(*delay);
        }
    }
}
