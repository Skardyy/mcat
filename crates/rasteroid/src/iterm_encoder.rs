use crate::term_misc::{self, EnvIdentifiers};
use std::io::Write;

/// encode an image bytes into inline image
/// should work with all formats Iterm, which include but not limited to GIF,PNG,JPEG..
/// # example:
/// ```
/// use std::path::Path;
/// use rasteroid::InlineEncoder;
/// use rasteroid::inline_an_image;
/// use rasteroid::iterm_encoder::encode_image;
/// use std::io::Write;
///
/// let path = Path::new("image.png");
/// let bytes = match std::fs::read(path) {
///     Ok(bytes) => bytes,
///     Err(e) => return,
/// };
/// let mut stdout = std::io::stdout();
/// encode_image(&bytes, &mut stdout, None, None).unwrap();
/// stdout.flush().unwrap();
/// ```
/// the option offset just offsets the image to the right by the amount of cells you specify
/// the print at is the same just absolute position
pub fn encode_image(
    img: &[u8],
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let base64_encoded = term_misc::image_to_base64(img);

    let center = term_misc::offset_to_terminal(offset);
    let at = term_misc::loc_to_terminal(print_at);
    out.write_all(at.as_ref())?;
    out.write_all(center.as_ref())?;

    let tmux = term_misc::get_wininfo().is_tmux;
    let prefix = if tmux { "\x1bPtmux;\x1b\x1b" } else { "\x1b" };
    let suffix = if tmux { "\x1b\x07\x1b\\" } else { "\x07" };

    write!(
        out,
        "{prefix}]1337;File=inline=1;size={}:{base64_encoded}{suffix}",
        base64_encoded.len()
    )?;

    Ok(())
}

/// checks if the current terminal supports Iterm graphic protocol
/// # example:
/// ```
/// use rasteroid::iterm_encoder::is_iterm_capable;
///
/// let mut env = rasteroid::term_misc::EnvIdentifiers::new();
/// let is_capable = is_iterm_capable(&mut env);
/// println!("Iterm: {}", is_capable);
/// ```
pub fn is_iterm_capable(env: &EnvIdentifiers) -> bool {
    env.term_contains("mintty")
        || env.term_contains("wezterm")
        || env.term_contains("iterm2")
        || env.term_contains("rio")
        || (env.term_contains("warp") && !env.contains("OS", "windows"))
        || env.has_key("KONSOLE_VERSION")
}
