use image::{DynamicImage, codecs::jpeg::JpegEncoder};

use crate::{
    VideoFrame,
    error::RasterError,
    term_misc::{self, EnvIdentifiers, Wininfo},
};
use std::io::{Cursor, Write};

pub fn encode_image(
    img: &DynamicImage,
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
    wininfo: &Wininfo,
) -> Result<(), RasterError> {
    let mut buf = Vec::new();
    if img.color().has_alpha() {
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
    } else {
        let mut cursor = Cursor::new(&mut buf);
        let mut enc = JpegEncoder::new_with_quality(&mut cursor, 90);
        enc.encode_image(img)?;
    }
    let base64_encoded = term_misc::image_to_base64(&buf);

    let center = term_misc::offset_to_terminal(offset);
    let at = term_misc::loc_to_terminal(print_at);
    out.write_all(at.as_ref())?;
    out.write_all(center.as_ref())?;

    let prefix = if wininfo.is_tmux {
        "\x1bPtmux;\x1b\x1b"
    } else {
        "\x1b"
    };
    let suffix = if wininfo.is_tmux {
        "\x07\x1b\\"
    } else {
        "\x07"
    };

    write!(out, "{prefix}]1337;File=inline=1;:{base64_encoded}{suffix}",)?;

    Ok(())
}

pub fn is_iterm_capable(env: &EnvIdentifiers) -> bool {
    env.term_contains("mintty")
        || env.term_contains("wezterm")
        || env.term_contains("iterm2")
        || env.term_contains("rio")
        || env.term_contains("konsole")
        || (env.term_contains("warp") && !env.contains("OS", "windows"))
        || env.has_key("KONSOLE_VERSION")
}

pub fn encode_frames(
    frames: &mut dyn Iterator<Item = VideoFrame>,
    out: &mut impl Write,
    wininfo: &Wininfo,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    crate::redraw_frames(frames, out, |img, buf| {
        encode_image(img, buf, offset, print_at, wininfo)?;
        Ok(wininfo.dim_to_cells(
            &format!("{}px", img.height()),
            term_misc::SizeDirection::Height,
        )? as u16)
    })
}
