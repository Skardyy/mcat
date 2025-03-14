use std::{borrow::Cow, cmp::min};

use crate::{inline_image::InlineImage, term_misc::EnvIdentifiers};

fn chunk_base64(base64: &str, size: usize) -> Cow<'_, str> {
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

    Cow::Owned(chunked_result)
}
pub fn encode_image(img: &InlineImage) -> Result<String, Box<dyn std::error::Error>> {
    let base64_encoded = img.encode_base64();

    let mut kitty_sequence = String::with_capacity(base64_encoded.len());

    if let Some(center) = img.center() {
        kitty_sequence.push_str(&center);
    }

    let chunked_base64 = chunk_base64(&base64_encoded, 4096);
    kitty_sequence.push_str(&chunked_base64);

    Ok(kitty_sequence)
}

pub fn is_kitty_capable(env: &EnvIdentifiers) -> bool {
    env.has_key("KITTY_WINDOW_ID")
        || env.term_contains("kitty")
        || (env.term_contains("wezterm") && !env.contains("OS", "windows"))
        || env.term_contains("ghostty")
        || env.has_key("KONSOLE_VERSION")
}
