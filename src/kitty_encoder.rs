use std::cmp::min;

use crate::{
    media_encoder::{Media, MediaTrait},
    term_misc::{parse_resize_mode, EnvIdentifiers},
};

fn chunk_base64(base64: String, size: usize) -> String {
    let total_bytes = base64.len();
    let mut start = 0;
    let mut chunked_result = String::with_capacity(total_bytes);
    let mut first_opts = "f=100,a=T,";

    while start < total_bytes {
        let end = min(start + size, total_bytes);
        let chunk_data = &base64[start..end];
        let more_chunks = !(end == total_bytes) as u8;

        let chunk = format!("\x1b_G{}m={};{}\x1b\\", first_opts, more_chunks, chunk_data);
        chunked_result.push_str(&chunk);

        if start == 0 {
            first_opts = "";
        }
        start = end;
    }

    chunked_result
}
pub fn encode(
    image_path: &str,
    width: u32,
    height: u32,
    resize_mode: &str,
    center: bool,
) -> String {
    let mut media = Media::new(image_path, true);
    let resize_mode = parse_resize_mode(resize_mode);
    let offset = media.resize_and_collect(width, height, resize_mode, center);
    let base64_encoded = media.encode_base64();

    let offset = match offset != 0 {
        true => format!("\x1b[{}C", offset),
        false => "".to_string(),
    };
    let base64_encoded = offset + &chunk_base64(base64_encoded, 4096);

    base64_encoded
}

pub fn is_kitty_capable(env: &EnvIdentifiers) -> bool {
    env.has_key("KITTY_WINDOW_ID")
        || env.term_contains("kitty")
        || (env.term_contains("wezterm") && !env.contains("OS", "windows"))
        || env.term_contains("ghostty")
        || env.has_key("KONSOLE_VERSION")
}
