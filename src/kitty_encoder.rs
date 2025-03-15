use std::{borrow::Cow, cmp::min, collections::HashMap, io::Write};

use base64::{engine::general_purpose, Engine};
use flate2::{write::ZlibEncoder, Compression};
use image::{Frame, Frames, RgbaImage};

use crate::{
    inline_image::{InlineImage, InlineImageFormat},
    term_misc::EnvIdentifiers,
    video::InlineVideo,
};

fn chunk_base64<'a>(
    base64: Cow<'a, str>,
    size: usize,
    opts: HashMap<String, String>,
) -> Cow<'a, str> {
    // identifying attributes
    let mut opts_string = String::new();
    for (key, value) in opts {
        opts_string.push_str(&format!(",{}={}", key, value));
    }

    let total_bytes = base64.len();
    let mut start = 0;
    let mut chunked_result = String::with_capacity(total_bytes);
    let mut first_opts = format!("q=2{},", opts_string);

    while start < total_bytes {
        let end = min(start + size, total_bytes);
        let chunk_data = &base64[start..end];
        let more_chunks = !(end == total_bytes) as u8;

        let chunk = format!("\x1b_G{}m={};{}\x1b\\", first_opts, more_chunks, chunk_data);
        chunked_result.push_str(&chunk);

        if start == 0 {
            first_opts = "".to_string();
        }
        start = end;
    }

    Cow::Owned(chunked_result)
}

fn process_frame(
    frame: &RgbaImage,
    opts: HashMap<String, String>,
) -> Result<Cow<'_, str>, std::io::Error> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());

    let mut rgb_data = Vec::new();
    for pixel in frame.pixels() {
        rgb_data.push(pixel[0]);
        rgb_data.push(pixel[1]);
        rgb_data.push(pixel[2]);
    }

    encoder.write_all(&rgb_data)?;
    let data = encoder.finish()?;

    let base64 = Cow::Owned(general_purpose::STANDARD.encode(data));
    let encoded_data = chunk_base64(base64, 4096, opts);

    Ok(encoded_data)
}

// doesn't work currently
//first frame: like a normal image with sizes added.
//second frame: control animation a=a, i=id, r=1
//till the end frame: a=f (transmit frame), s, v (maybe), c={frame number}, z={delay}
//final frame: a=a, s=3 (idk why), r=1
pub fn encode_frames(frames: Frames<'_>, id: u32) -> Cow<'_, str> {
    let mut full_data = String::new();

    for (c, frame) in frames.into_iter().enumerate() {
        if let Ok(frame) = frame {
            let buffer = frame.buffer();
            let s = buffer.width().to_string();
            let v = buffer.height().to_string();
            let i = id.to_string();

            if c == 0 {
                if let Ok(f) = process_frame(
                    buffer,
                    HashMap::from([
                        ("s".to_string(), s),
                        ("v".to_string(), v),
                        ("f".to_string(), "24".to_string()),
                        ("o".to_string(), "z".to_string()),
                        ("I".to_string(), i),
                        ("a".to_string(), "T".to_string()),
                    ]),
                ) {
                    full_data.push_str(&f);
                    full_data.push_str(&format!("\x1b_Ga=a,r=1,I={}\x1b\\", id));
                }
                continue;
            }

            let (z, _) = frame.delay().numer_denom_ms();
            let opts = HashMap::from([
                ("s".to_string(), s),
                ("v".to_string(), v),
                ("z".to_string(), z.to_string()),
                ("o".to_string(), "z".to_string()),
                ("a".to_string(), "f".to_string()),
                ("c".to_string(), c.to_string()),
                ("X".to_string(), c.to_string()),
                ("c".to_string(), c.to_string()),
            ]);

            if let Ok(f) = process_frame(buffer, opts) {
                full_data.push_str(&f);
            }
        } else {
            full_data.push_str(&format!("\x1b_Ga=a,s=3,r=1,I={}\x1b\\", id));
        }
    }

    Cow::Owned(full_data)
}

pub fn encode_image(img: &InlineImage) -> Result<String, Box<dyn std::error::Error>> {
    let id: u32 = rand::random();
    let encoded_data = match img.format {
        InlineImageFormat::GIF => encode_frames(InlineVideo::into_frames(&img.buffer)?, id),
        InlineImageFormat::PNG => chunk_base64(
            img.encode_base64(),
            4096,
            HashMap::from([("f".to_string(), "100".to_string())]),
        ),
    };
    let mut kitty_sequence = String::with_capacity(encoded_data.len());

    if let Some(center) = img.center() {
        kitty_sequence.push_str(&center);
    }
    kitty_sequence.push_str(&encoded_data);

    Ok(kitty_sequence)
}

pub fn is_kitty_capable(env: &EnvIdentifiers) -> bool {
    env.has_key("KITTY_WINDOW_ID")
        || env.term_contains("kitty")
        || (env.term_contains("wezterm") && !env.contains("OS", "windows"))
        || env.term_contains("ghostty")
        || env.has_key("KONSOLE_VERSION")
}
