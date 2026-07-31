use image::{DynamicImage, codecs::jpeg::JpegEncoder};

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
        || (env.term_contains("warp") && !env.contains("OS", "windows"))
        || env.has_key("KONSOLE_VERSION")
}

fn park_cursor_below(out: &mut impl Write, rows: u16) -> Result<(), RasterError> {
    write!(out, "\x1b[u\x1b[{rows}B\r")?;
    out.flush()?;
    Ok(())
}

pub fn encode_frames(
    frames: &mut dyn Iterator<Item = VideoFrame>,
    out: &mut impl Write,
    wininfo: &Wininfo,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), RasterError> {
    // floor on the effective frame rate, so a machine that can't keep up
    // degrades to a lower rate instead of dropping every frame
    const MIN_INTERVAL: Duration = Duration::from_millis(100);

    let shutdown = term_misc::setup_signal_handler();
    let mut first = true;
    let mut reserved_rows: u16 = 0;
    let mut start = std::time::Instant::now();
    let mut last_shown = start;

    for (img, timestamp) in frames {
        if shutdown.load(Ordering::SeqCst) {
            park_cursor_below(out, reserved_rows)?;
            return Ok(());
        }

        if first {
            let mut buf = Vec::new();
            encode_image(&img, &mut buf, offset, print_at, wininfo)?;

            reserved_rows = wininfo.dim_to_cells(
                &format!("{}px", img.height()),
                term_misc::SizeDirection::Height,
            )? as u16;
            term_misc::ensure_space(out, reserved_rows)?;
            write!(out, "\x1b[s")?;
            out.write_all(&buf)?;
            out.flush()?;

            // frame 0 is on screen now; every later frame is scheduled against this
            start = std::time::Instant::now();
            last_shown = start;
            first = false;
            continue;
        }

        let target = start + Duration::from_secs_f32(timestamp.max(0.0));
        let now = std::time::Instant::now();

        // already overdue, and we've shown something recently enough
        if now > target && now.duration_since(last_shown) < MIN_INTERVAL {
            continue;
        }

        let mut buf = Vec::new();
        encode_image(&img, &mut buf, offset, print_at, wininfo)?;

        // sleep only the time left until this frame is due, not the full delay.
        // encoding already consumed part of the budget.
        if let Some(left) = target.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(left);
        }

        write!(out, "\x1b[u")?;
        out.write_all(&buf)?;
        out.flush()?;
        last_shown = std::time::Instant::now();
    }

    if first {
        return Err(RasterError::EmptyVideo);
    }

    park_cursor_below(out, reserved_rows)?;
    Ok(())
}
