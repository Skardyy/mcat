use std::{
    borrow::Cow,
    cmp::min,
    collections::HashMap,
    io::{Error, Write},
};

use base64::{engine::general_purpose, Engine};
use flate2::{write::ZlibEncoder, Compression};
use image::{Frames, Pixel, RgbaImage};

use crate::{
    inline_image::{InlineImage, InlineImageFormat},
    term_misc::EnvIdentifiers,
    video::InlineVideo,
};

fn chunk_base64<'a>(
    base64: Cow<'a, str>,
    size: usize,
    first_opts: HashMap<String, String>,
    sub_opts: HashMap<String, String>,
) -> Cow<'a, str> {
    // first block
    let mut first_opts_string = String::with_capacity(first_opts.len() * 8);
    for (key, value) in first_opts {
        if first_opts_string != "" {
            first_opts_string.push_str(",");
        }
        first_opts_string.push_str(&format!("{}={}", key, value));
    }
    if first_opts_string != "" {
        first_opts_string.push_str(",");
    }

    // all other blocks
    let mut sub_opts_string = String::with_capacity(sub_opts.len() * 8);
    for (key, value) in sub_opts {
        if sub_opts_string != "" {
            sub_opts_string.push_str(",");
        }
        sub_opts_string.push_str(&format!("{}={}", key, value));
    }
    if sub_opts_string != "" {
        sub_opts_string.push_str(",");
    }

    let total_bytes = base64.len();
    let mut start = 0;
    let mut chunked_result = String::with_capacity(total_bytes + (total_bytes / size) * 30);

    while start < total_bytes {
        let end = min(start + size, total_bytes);
        let chunk_data = &base64[start..end];
        let more_chunks = !(end == total_bytes) as u8;

        let opts = if start == 0 {
            &first_opts_string
        } else {
            &sub_opts_string
        };

        let chunk = format!("\x1b_G{}m={};{}\x1b\\", opts, more_chunks, chunk_data);
        chunked_result.push_str(&chunk);

        start = end;
    }

    Cow::Owned(chunked_result)
}

fn process_frame(
    frame: &RgbaImage,
    first_opts: HashMap<String, String>,
    sub_opts: HashMap<String, String>,
) -> Result<Cow<'_, str>, Error> {
    let width = frame.width() as usize;
    let height = frame.height() as usize;
    let rgb_size = width * height * 3;
    let mut rgb_data = Vec::with_capacity(rgb_size);

    for pixel in frame.pixels() {
        let channels = pixel.channels();
        rgb_data.push(channels[0]);
        rgb_data.push(channels[1]);
        rgb_data.push(channels[2]);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&rgb_data)?;
    let compressed = encoder.finish()?;

    let base64 = Cow::Owned(general_purpose::STANDARD.encode(compressed));
    let encoded_data = chunk_base64(base64, 4096, first_opts, sub_opts);

    Ok(encoded_data)
}

pub fn encode_frames(frames: Frames<'_>, id: u32) -> Cow<'_, str> {
    let mut frames = frames.into_iter();

    // getting the first frame
    let first = frames.next().unwrap_or_else(|| {
        eprintln!("video is empty");
        std::process::exit(1);
    });
    let first = first.unwrap_or_else(|_| {
        eprintln!("video is invalid");
        std::process::exit(1);
    });
    let img = first.buffer();

    // not accurate cuz there is deflating and base64 encoding (can't allocate something close)
    let frame_count = frames.size_hint().0 + 1;
    let mut full_data =
        String::with_capacity(frame_count * img.width() as usize * img.height() as usize * 3);

    // adding the root image
    let i = id.to_string();
    let s = img.width().to_string();
    let v = img.height().to_string();
    let f = "24".to_string();
    let o = "z".to_string();
    let q = "2".to_string();
    let root_img = process_frame(
        img,
        HashMap::from([
            ("a".to_string(), "T".to_string()),
            ("f".to_string(), f),
            ("o".to_string(), o),
            ("I".to_string(), i),
            ("s".to_string(), s),
            ("v".to_string(), v),
            ("q".to_string(), q),
        ]),
        HashMap::new(),
    )
    .unwrap_or_else(|_| {
        eprintln!("video is invalid");
        std::process::exit(1);
    });
    full_data.push_str(&root_img);

    // starting the animation
    let (z, _) = first.delay().numer_denom_ms();
    full_data.push_str(&format!("\x1b_Ga=a,s=2,v=1,r=1,I={},z={}\x1b\\", id, z));

    for (c, frame) in frames.enumerate() {
        if let Ok(frame) = frame {
            let buffer = frame.buffer();
            let s = buffer.width().to_string();
            let v = buffer.height().to_string();
            let i = id.to_string();
            let f = "24".to_string();
            let o = "z".to_string();
            let (z, _) = frame.delay().numer_denom_ms();

            let first_opts = HashMap::from([
                ("a".to_string(), "f".to_string()),
                ("f".to_string(), f),
                ("o".to_string(), o),
                ("I".to_string(), i),
                ("c".to_string(), c.to_string()),
                ("s".to_string(), s),
                ("v".to_string(), v),
                ("z".to_string(), z.to_string()),
            ]);
            let sub_opts = HashMap::from([("a".to_string(), "f".to_string())]);

            if let Ok(f) = process_frame(buffer, first_opts, sub_opts) {
                full_data.push_str(&f);
            }
        }
    }

    full_data.push_str(&format!("\x1b_Ga=a,s=3,v=1,r=1,I={},z={}\x1b\\", id, z));
    Cow::Owned(full_data)
}

pub fn encode_image(img: &InlineImage) -> Result<String, Box<dyn std::error::Error>> {
    let id: u32 = rand::random();
    let encoded_data = match img.format {
        InlineImageFormat::GIF => encode_frames(InlineVideo::into_frames(&img.buffer)?, id),
        InlineImageFormat::PNG => chunk_base64(
            img.encode_base64(),
            4096,
            HashMap::from([
                ("f".to_string(), "100".to_string()),
                ("a".to_string(), "T".to_string()),
            ]),
            HashMap::new(),
        ),
    };
    let mut kitty_sequence = String::with_capacity(encoded_data.len() + 10);

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
