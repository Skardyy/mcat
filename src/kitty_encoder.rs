use crate::media_encoder::Media;

pub fn encode(image_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let media = Media::new(image_path, 800, 400)?;
    let base64_encoded = media.encode_base64();

    let mut kitty_sequence = String::with_capacity(base64_encoded.len() + media.path.len() + 50);

    kitty_sequence.push_str("\x1b_Gf=100;");
    kitty_sequence.push_str(&base64_encoded);
    kitty_sequence.push_str("\x1b\\");

    Ok(kitty_sequence)
}
