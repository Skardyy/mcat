use crate::media_encoder::Media;

pub fn encode(image_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let media = Media::new(image_path, 800, 400)?;
    let base64_encoded = media.encode_base64();

    let mut iterm_sequence = String::with_capacity(base64_encoded.len() + media.path.len() + 50);

    iterm_sequence.push_str("\x1b]1337;File=inline=1;size=");
    iterm_sequence.push_str(&base64_encoded.len().to_string());
    iterm_sequence.push(':');
    iterm_sequence.push_str(&base64_encoded);
    iterm_sequence.push('\x07');

    Ok(iterm_sequence)
}
