use crate::{inline_image::InlineImage, term_misc::EnvIdentifiers};

pub fn encode_image(img: &InlineImage) -> Result<String, Box<dyn std::error::Error>> {
    let base64_encoded = img.encode_base64();

    let mut iterm_sequence = String::with_capacity(base64_encoded.len() + 50);

    if let Some(center) = img.center() {
        iterm_sequence.push_str(&center);
    }
    iterm_sequence.push_str("\x1b]1337;File=inline=1;size=");
    iterm_sequence.push_str(&base64_encoded.len().to_string());
    iterm_sequence.push(':');
    iterm_sequence.push_str(&base64_encoded);
    iterm_sequence.push('\x07');

    Ok(iterm_sequence)
}

pub fn is_iterm_capable(env: &EnvIdentifiers) -> bool {
    env.term_contains("mintty")
        || env.term_contains("wezterm")
        || env.term_contains("iterm2")
        || env.term_contains("rio")
        || env.term_contains("Warp")
        || env.has_key("KONSOLE_VERSION")
}
