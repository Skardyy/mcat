use crate::{
    media_encoder::{Media, MediaTrait},
    term_misc::{parse_resize_mode, EnvIdentifiers},
};

pub fn encode(image_path: &str, width: u32, height: u32, resize_mode: &str) -> String {
    let mut media = Media::new(image_path);
    let resize_mode = parse_resize_mode(resize_mode);
    media.resize_and_collect(width, height, resize_mode);
    let base64_encoded = media.encode_base64();

    let mut iterm_sequence = String::with_capacity(base64_encoded.len() + 50);

    iterm_sequence.push_str("\x1b]1337;File=inline=1;size=");
    iterm_sequence.push_str(&base64_encoded.len().to_string());
    iterm_sequence.push(':');
    iterm_sequence.push_str(&base64_encoded);
    iterm_sequence.push('\x07');

    iterm_sequence
}

pub fn is_iterm_capable() -> bool {
    let env = EnvIdentifiers::new();

    env.term_contains("mintty")
        || env.term_contains("wezterm")
        || env.term_contains("iterm2")
        || env.term_contains("rio")
        || env.has_key("KONSOLE_VERSION")
}
