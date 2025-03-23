use std::{cmp::min, collections::HashMap, error::Error, io::Write};

use base64::{engine::general_purpose, Engine};
use flate2::{write::ZlibEncoder, Compression};
use image::{Frames, Pixel, RgbaImage};

use crate::{
    inline_image::{InlineImage, InlineImageFormat},
    inline_video::InlineVideo,
    term_misc::EnvIdentifiers,
};

fn chunk_base64(
    base64: &str,
    buffer: &mut Vec<u8>,
    size: usize,
    first_opts: HashMap<String, String>,
    sub_opts: HashMap<String, String>,
) -> Result<(), std::io::Error> {
    // first block
    let mut first_opts_string = Vec::with_capacity(first_opts.len() * 8);
    for (key, value) in first_opts {
        if !first_opts_string.is_empty() {
            first_opts_string.push(b',');
        }
        write!(first_opts_string, "{}={}", key, value)?;
    }
    if !first_opts_string.is_empty() {
        first_opts_string.push(b',');
    }

    // all other blocks
    let mut sub_opts_string = Vec::with_capacity(sub_opts.len() * 8);
    for (key, value) in sub_opts {
        if !sub_opts_string.is_empty() {
            sub_opts_string.push(b',');
        }
        write!(sub_opts_string, "{}={}", key, value)?;
    }
    if !sub_opts_string.is_empty() {
        sub_opts_string.push(b',');
    }

    let total_bytes = base64.len();
    let mut start = 0;

    while start < total_bytes {
        let end = min(start + size, total_bytes);
        let chunk_data = &base64[start..end];
        let more_chunks = (end != total_bytes) as u8;

        let opts = if start == 0 {
            &first_opts_string
        } else {
            &sub_opts_string
        };

        buffer.extend_from_slice(b"\x1b_G");
        buffer.extend_from_slice(opts);
        write!(buffer, "m={};{}", more_chunks, chunk_data)?;
        buffer.extend_from_slice(b"\x1b\\");

        start = end;
    }

    Ok(())
}

fn process_frame(
    frame: &RgbaImage,
    buffer: &mut Vec<u8>,
    first_opts: HashMap<String, String>,
    sub_opts: HashMap<String, String>,
) -> Result<(), Box<dyn Error>> {
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

    let base64 = general_purpose::STANDARD.encode(compressed);
    chunk_base64(&base64, buffer, 4096, first_opts, sub_opts)?;

    Ok(())
}

pub fn encode_frames(
    frames: Frames<'_>,
    id: u32,
    pre_string: String,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut frames = frames.into_iter();

    // getting the first frame
    let first = frames.next().ok_or("video doesn't contain any frames")??;
    let img = first.buffer();

    // not accurate cuz there is deflating and base64 encoding (can't allocate something close)
    let frame_count = frames.size_hint().0 + 1;
    let mut buffer = Vec::with_capacity(
        frame_count * img.width() as usize * img.height() as usize * 3 + pre_string.len(),
    );
    buffer.extend_from_slice(pre_string.as_bytes());

    // adding the root image
    let i = id.to_string();
    let s = img.width().to_string();
    let v = img.height().to_string();
    let f = "24".to_string();
    let o = "z".to_string();
    let q = "2".to_string();
    process_frame(
        img,
        &mut buffer,
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
    )?;

    // starting the animation
    let (z, _) = first.delay().numer_denom_ms();
    write!(buffer, "\x1b_Ga=a,s=2,v=1,r=1,I={},z={}\x1b\\", id, z)?;

    for (c, frame) in frames.enumerate() {
        if let Ok(frame) = frame {
            let img = frame.buffer();
            let s = img.width().to_string();
            let v = img.height().to_string();
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

            process_frame(img, &mut buffer, first_opts, sub_opts)?;
        }
    }

    write!(buffer, "\x1b_Ga=a,s=3,v=1,r=1,I={},z={}\x1b\\", id, z)?;
    Ok(buffer)
}

pub fn encode_image(img: &InlineImage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let id: u32 = rand::random();
    let center_string = img.center().unwrap_or_default();
    let encoded_data = match img.format {
        InlineImageFormat::Gif => {
            encode_frames(InlineVideo::into_frames(&img.buffer)?, id, center_string)?
        }
        InlineImageFormat::Png => {
            let base64 = img.encode_base64();
            let mut buffer = Vec::with_capacity(base64.len() + 10);
            buffer.extend_from_slice(center_string.as_bytes());
            chunk_base64(
                &base64,
                &mut buffer,
                4096,
                HashMap::from([
                    ("f".to_string(), "100".to_string()),
                    ("a".to_string(), "T".to_string()),
                ]),
                HashMap::new(),
            )?;

            buffer
        }
    };

    Ok(encoded_data)
}

pub fn is_kitty_capable(env: &EnvIdentifiers) -> bool {
    env.has_key("KITTY_WINDOW_ID") || env.term_contains("kitty") || env.term_contains("ghostty")
}
