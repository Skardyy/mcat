use crate::{
    Frame,
    error::RasterError,
    term_misc::{self, EnvIdentifiers, Wininfo, loc_to_terminal, offset_to_terminal},
};
use color_quant::NeuQuant;
use image::{ImageBuffer, Rgb};
use std::{io::Write, sync::atomic::Ordering, time::Duration};

const SIXEL_MIN: u8 = 0x3f;

pub fn encode_image(
    img: &[u8],
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
    wininfo: &Wininfo,
) -> Result<(), RasterError> {
    let dyn_img = image::load_from_memory(img)?;
    let rgb_img = dyn_img.to_rgb8();

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
    let suffix = if is_tmux { "\x1b\x1b\\\x1b\\" } else { "\x07" };

    write!(out, "{prefix}P0;1q\"1;1;{};{}", width, height)?;

    let pixels: Vec<u8> = img.pixels().flat_map(|p| p.0[..3].to_vec()).collect();
    let nq = NeuQuant::new(10, 256, &pixels);
    let palette_vec: Vec<(u8, u8, u8)> = nq
        .color_map_rgb()
        .chunks(3)
        .map(|c| (c[0], c[1], c[2]))
        .collect();
    let palette = &palette_vec;
    let color_indices = map_to_palette(img, palette);

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

fn map_to_palette(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let mut indices = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let rgb = (pixel[0], pixel[1], pixel[2]);

            let idx = find_closest_color(palette, &rgb);
            indices.push(idx);
        }
    }

    indices
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

fn find_closest_color(palette: &[(u8, u8, u8)], color: &(u8, u8, u8)) -> u8 {
    let mut closest = 0;
    let mut min_dist = u32::MAX;

    for (i, pal_color) in palette.iter().enumerate() {
        let dr = color.0 as i32 - pal_color.0 as i32;
        let dg = color.1 as i32 - pal_color.1 as i32;
        let db = color.2 as i32 - pal_color.2 as i32;

        let dist = (dr * dr + dg * dg + db * db) as u32;
        if dist < min_dist {
            min_dist = dist;
            closest = i;
        }
    }

    closest as u8
}

pub fn encode_frames(
    frames: &mut dyn Iterator<Item = impl Frame>,
    out: &mut impl Write,
    wininfo: &Wininfo,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    let shutdown = term_misc::setup_signal_handler();
    let mut last_timestamp: Option<f32> = None;
    let mut frame_cache: Vec<(Vec<u8>, Duration)> = Vec::new();
    let first_frame = frames.next().ok_or(RasterError::EmptyVideo)?;

    let at = print_at.unwrap_or((0, 0));

    // pre-encode first frame and ensure space
    let mut first_buf = Vec::new();
    encode_image(
        first_frame.data(),
        &mut first_buf,
        offset,
        Some(at),
        wininfo,
    )?;
    term_misc::ensure_space(out, first_frame.height())?;
    out.write_all(&first_buf)?;
    out.flush()?;

    let delay = Duration::from_millis(33);
    frame_cache.push((first_buf, delay));

    for frame in frames {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }

        let data = frame.data();
        if data.is_empty() {
            continue;
        }

        let delay = match (frame.timestamp(), last_timestamp) {
            (ts, Some(last)) if ts > last => Duration::from_secs_f32(ts - last),
            _ => Duration::from_millis(33),
        };
        last_timestamp = Some(frame.timestamp());

        let mut buf = Vec::new();
        encode_image(data, &mut buf, offset, Some(at), wininfo)?;

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
