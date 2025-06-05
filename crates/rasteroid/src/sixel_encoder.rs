use crate::term_misc::{self, EnvIdentifiers, loc_to_terminal, offset_to_terminal};
use color_quant::NeuQuant;
use image::{ImageBuffer, Rgb};
use std::{
    error::Error,
    io::{self, Write},
};

const SIXEL_MIN: u8 = 0x3f; // '?'

/// encode an image into inline image ()
/// works with all the formats that the image crate supports
/// # example:
/// ```
/// use std::path::Path;
/// use std::io::Write;
/// use rasteroid::sixel_encoder::encode_image;
///
/// let path = Path::new("image.png");
/// let bytes = match std::fs::read(path) {
///     Ok(bytes) => bytes,
///     Err(e) => return,
/// };
/// let mut stdout = std::io::stdout();
/// encode_image(&bytes, &stdout, None, None).unwrap();
/// stdout.flush().unwrap();
/// ```
/// the option offset just offsets the image to the right by the amount of cells you specify
pub fn encode_image(
    img: &[u8],
    mut out: impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dyn_img = image::load_from_memory(img)?;
    let rgb_img = dyn_img.to_rgb8();

    let center = offset_to_terminal(offset);
    let print_at_string = loc_to_terminal(print_at);
    out.write_all(print_at_string.as_ref())?;
    out.write_all(center.as_ref())?;

    encode_sixel(&rgb_img, out)?;

    Ok(())
}

/// checks if the current terminal supports Sixel's graphic protocol
/// # example:
/// ```
/// use rasteroid::sixel_encoder::is_sixel_capable;
///
/// let env = rasteroid::term_misc::EnvIdentifiers::new();
/// let is_capable = is_sixel_capable(&env);
/// println!("Sixel: {}", is_capable);
/// ```
pub fn is_sixel_capable(env: &mut EnvIdentifiers) -> bool {
    // has way more support, i just think sixel is bad
    env.term_contains("foot") 
        || env.has_key("WT_PROFILE_ID") // windows-terminal
        || env.term_contains("sixel-tmux")
}

fn encode_sixel(
    img: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    mut out: impl Write,
) -> Result<(), Box<dyn Error>> {
    let width = img.width() as usize;
    let height = img.height() as usize;

    if width == 0 || height == 0 {
        return Err("image is empty".into());
    }

    write_sixel(&mut out, img)?;
    Ok(())
}

fn write_sixel<W: Write>(out: &mut W, img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> io::Result<()> {
    let width = img.width() as usize;
    let height = img.height() as usize;

    let tmux = term_misc::get_wininfo().is_tmux;
    let prefix = if tmux { "\x1bPtmux;\x1b\x1b" } else { "\x1b" };
    let suffix = if tmux { "\x1b\x1b\\\x1b\\" } else { "\x07" };

    // DECSIXEL introducer and raster attributes
    write!(out, "{prefix}P0;1q\"1;1;{};{}", width, height)?;

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

    out.write_all(suffix.as_bytes())?;

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
    if repeat_count == 0 {
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
