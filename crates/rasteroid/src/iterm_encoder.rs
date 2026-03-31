use image::DynamicImage;

use crate::{
    VideoFrame,
    error::RasterError,
    term_misc::{self, EnvIdentifiers, Wininfo},
};
use std::{
    io::{Cursor, Write},
    sync::atomic::Ordering,
    time::Duration,
};

pub fn encode_image(
    img: &DynamicImage,
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
    wininfo: &Wininfo,
) -> Result<(), RasterError> {
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
    let base64_encoded = term_misc::image_to_base64(&png);

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
        "\x1b\x07\x1b\\"
    } else {
        "\x07"
    };

    write!(
        out,
        "{prefix}]1337;File=inline=1;size={}:{base64_encoded}{suffix}",
        base64_encoded.len()
    )?;

    Ok(())
}

pub fn is_iterm_capable(env: &EnvIdentifiers) -> bool {
    env.term_contains("mintty")
        || env.term_contains("wezterm")
        || env.term_contains("iterm2")
        || env.term_contains("rio")
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
    let shutdown = term_misc::setup_signal_handler();
    let mut last_timestamp: Option<f32> = None;
    let mut frame_cache: Vec<(Vec<u8>, Duration)> = Vec::new();
    let mut first = true;

    for (img, timestamp) in frames {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }

        let delay = match (timestamp, last_timestamp) {
            (ts, Some(last)) if ts > last => Duration::from_secs_f32(ts - last),
            _ => Duration::from_millis(33),
        };
        last_timestamp = Some(timestamp);

        let mut buf = Vec::new();
        encode_image(&img, &mut buf, offset, print_at, wininfo)?;

        if first {
            term_misc::ensure_space(out, img.height() as u16)?;
            write!(out, "\x1b[s")?;
            first = false;
        } else {
            write!(out, "\x1b[u\x1b[s")?;
        }

        out.write_all(&buf)?;
        out.flush()?;
        frame_cache.push((buf, delay));
        std::thread::sleep(delay);
    }

    if frame_cache.is_empty() {
        return Err(RasterError::EmptyVideo);
    }

    // loop cached frames
    loop {
        for (buf, delay) in &frame_cache {
            if shutdown.load(Ordering::SeqCst) {
                return Ok(());
            }
            write!(out, "\x1b[u\x1b[s")?;
            out.write_all(buf)?;
            out.flush()?;
            std::thread::sleep(*delay);
        }
    }
}
