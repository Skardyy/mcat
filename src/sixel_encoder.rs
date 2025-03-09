use crate::{
    media_encoder::{Media, MediaTrait},
    term_misc::{parse_resize_mode, EnvIdentifiers},
};
use color_quant::NeuQuant;
use image::{ImageBuffer, Rgb};
use std::io::{self, Write};

const SIXEL_MIN: u8 = 0x3f; // '?'

pub fn encode(image_path: &str, width: u32, height: u32, resize_mode: &str) -> String {
    let mut media = Media::new(image_path, false);
    let resize_mode = parse_resize_mode(resize_mode);
    media.resize_and_collect(width, height, resize_mode);
    let rgb_img = media.to_rgb8();
    let result = encode_sixel(&rgb_img);

    result
}

pub fn is_sixel_capable(env: &EnvIdentifiers) -> bool {
    // has way more support, i just think sixel is bad
    env.term_contains("foot") 
        || env.has_key("WT_PROFILE_ID") // windows-terminal
        || env.term_contains("sixel-tmux")
}

pub fn encode_sixel(img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> String {
    let width = img.width() as usize;
    let height = img.height() as usize;

    if width == 0 || height == 0 {
        return String::new();
    }

    let mut output = Vec::new();
    write_sixel(&mut output, img).expect("failed to write sixel");
    String::from_utf8_lossy(&output).to_string()
}

fn write_sixel<W: Write>(out: &mut W, img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> io::Result<()> {
    let width = img.width() as usize;
    let height = img.height() as usize;

    // DECSIXEL introducer and raster attributes
    write!(out, "\x1bP0;1q\"1;1;{};{}", width, height)?;

    // median quant works the best through testing
    let pixels: Vec<u8> = img.pixels().flat_map(|p| p.0[..3].to_vec()).collect();
    let nq = NeuQuant::new(10, 256, &pixels);
    let palette_vec: Vec<(u8, u8, u8)> = nq
        .color_map_rgb()
        .chunks(3)
        .map(|c| (c[0], c[1], c[2]))
        .collect();
    let palette = &palette_vec;
    let color_indices = map_to_palette(img, palette);

    // Write palette
    for (i, &(r, g, b)) in palette.iter().enumerate() {
        // Convert RGB to percentages (0-100)
        let r_pct = (r as f32 / 255.0 * 100.0) as u8;
        let g_pct = (g as f32 / 255.0 * 100.0) as u8;
        let b_pct = (b as f32 / 255.0 * 100.0) as u8;

        write!(out, "#{};2;{};{};{}", i, r_pct, g_pct, b_pct)?;
    }
    let palette_size = palette.len();
    let mut color_used = vec![false; palette_size];
    let mut sixel_data = vec![0u8; width * palette_size];

    // Process the image in 6-pixel strips
    let sixel_rows = (height + 5) / 6;
    for row in 0..sixel_rows {
        // Graphics NL (new sixel line)
        if row > 0 {
            write!(out, "-")?;
        }

        // Reset color usage flags and sixel data
        color_used.fill(false);
        sixel_data.fill(0);

        // Buffer sixel row, track used colors
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

        // Render sixel row for each palette entry
        let mut first_color_written = false;
        for n in 0..palette_size {
            if !color_used[n] {
                continue;
            }

            // Graphics CR
            if first_color_written {
                write!(out, "$")?;
            }

            // Color Introducer
            write!(out, "#{}", n)?;

            let mut rle_count = 0;
            let mut prev_sixel = 255; // Sentinel value

            for x in 0..width {
                let next_sixel = sixel_data[(n * width) + x];

                // RLE encode, write on value change
                if prev_sixel != 255 && next_sixel != prev_sixel {
                    write_gri(out, rle_count, prev_sixel)?;
                    rle_count = 0;
                }

                prev_sixel = next_sixel;
                rle_count += 1;
            }

            // Write last sixel in line
            write_gri(out, rle_count, prev_sixel)?;

            first_color_written = true;
        }
    }

    // SIXEL terminator
    write!(out, "\x1b\\")?;

    Ok(())
}

// Map image pixels to the fixed palette
fn map_to_palette(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let mut indices = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let rgb = (pixel[0], pixel[1], pixel[2]);

            // Find closest color in palette
            let idx = find_closest_color(palette, &rgb);
            indices.push(idx);
        }
    }

    indices
}

// Graphics Repeat Introducer encoding
fn write_gri<W: Write>(out: &mut W, repeat_count: usize, sixel: u8) -> io::Result<()> {
    if repeat_count <= 0 {
        return Ok(());
    }

    // Mask with valid sixel bits, apply offset
    let sixel = SIXEL_MIN + (sixel & 0b111111);

    if repeat_count > 3 {
        // Graphics Repeat Introducer
        write!(out, "!{}{}", repeat_count, sixel as char)?;
    } else {
        // Just repeat the character
        for _ in 0..repeat_count {
            write!(out, "{}", sixel as char)?;
        }
    }

    Ok(())
}

// Find the closest color in the palette
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
