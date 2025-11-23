use std::{
    collections::HashMap,
    env, f32,
    io::Write,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool},
};

use base64::{Engine, engine::general_purpose};
use crossterm::terminal::{size, window_size};
use signal_hook::consts::signal::*;
use signal_hook::flag;

use crate::get_tmux_terminal_name;

pub struct Wininfo {
    pub sc_width: u16,
    pub sc_height: u16,
    pub spx_width: u16,
    pub spx_height: u16,
    pub is_tmux: bool,
    pub needs_inline: bool,
}

/// converts image bytse into base64
pub fn image_to_base64(img: &[u8]) -> String {
    general_purpose::STANDARD.encode(img)
}

/// turns offset into terminal escape characters that move the cursor
pub fn offset_to_terminal(offset: Option<u16>) -> String {
    match offset {
        Some(offset) => format!("\x1b[{}C", offset),
        None => "".to_string(),
    }
}

/// turns offset into terminal escape characters that move the cursor
pub fn loc_to_terminal(at: Option<(u16, u16)>) -> String {
    match at {
        Some((x, y)) => format!("\x1b[{y};{x}H"),
        None => "".to_string(),
    }
}

#[derive(Clone)]
struct WininfoParams {
    spx: Size,
    sc: Size,
    scalex: Option<f32>,
    scaley: Option<f32>,
    is_tmux: bool,
    needs_inline: bool,
}

static WININFO_PARAMS: OnceLock<Mutex<WininfoParams>> = OnceLock::new();
static WININFO: OnceLock<Wininfo> = OnceLock::new();

#[derive(Clone)]
pub struct Size {
    pub width: u16,
    pub height: u16,
    pub force: bool,
}

impl Wininfo {
    fn new(
        spx_fallback: &Size,
        sc_fallback: &Size,
        scalex: Option<f32>,
        scaley: Option<f32>,
        is_tmux: bool,
        needs_inline: bool,
    ) -> Self {
        let mut spx_width = 0;
        let mut spx_height = 0;
        if let Ok(res) = window_size() {
            // ioctl for unix
            spx_width = res.width;
            spx_height = res.height;
        } else {
            // do windows api here
            #[cfg(windows)]
            if let Some(size) = get_size_windows() {
                spx_width = size.0;
                spx_height = size.1;
            }
        }
        let (mut sc_width, mut sc_height) = size().unwrap_or((0, 0));

        // fallback or forcing
        if spx_fallback.force || spx_width == 0 || spx_height == 0 {
            spx_width = spx_fallback.width;
            spx_height = spx_fallback.height;
        }
        if sc_fallback.force || sc_width == 0 || sc_height == 0 {
            sc_width = sc_fallback.width;
            sc_height = sc_fallback.height;
        }

        let scalex = scalex.unwrap_or(1.0);
        let scaley = scaley.unwrap_or(1.0);

        Wininfo {
            sc_height: (sc_height as f32 * scaley) as u16,
            sc_width: (sc_width as f32 * scalex) as u16,
            spx_height: (spx_height as f32 * scaley) as u16,
            spx_width: (spx_width as f32 * scalex) as u16,
            is_tmux,
            needs_inline,
        }
    }
}

/// setting a fallback for when fails to query spx and sc.
/// scale is for scaling while maintaining center. (scale the box not the image)
/// # example:
/// ```
/// use rasteroid::term_misc::init_wininfo;
/// use rasteroid::term_misc::Size;
///
/// let spx = Size {
///     width: 1920,  // width in pixels
///     height: 1080, // height in pixels
///     force: false, // use that instead of checking
/// };
/// let sc = Size {
///     width: 100,   // width in cells
///     height: 20,   // height in cells
///     force: false, // use that instead of checking
/// };
/// let mut env = rasteroid::term_misc::EnvIdentifiers::new();
/// let is_tmux = env.is_tmux();
/// // inline is for kitty to put a placeholder for images / videos so they can be placed in apps
/// // that don't understand kitty gp and have them scroll with the buffer; e.g vim, tmux
/// let inline = false;
/// init_wininfo(&spx, &sc, None, None, is_tmux, inline).unwrap(); // going to error if you called it before already.
/// ```
pub fn init_wininfo(
    spx: &Size,
    sc: &Size,
    scalex: Option<f32>,
    scaley: Option<f32>,
    is_tmux: bool,
    needs_inline: bool,
) -> Result<(), &'static str> {
    let params = WininfoParams {
        spx: spx.clone(),
        sc: sc.clone(),
        scalex,
        scaley,
        is_tmux,
        needs_inline,
    };

    if let Some(existing_params) = WININFO_PARAMS.get() {
        // Already have params, update them (only if Wininfo not created yet)
        if WININFO.get().is_none() {
            *existing_params.lock().unwrap() = params;
            Ok(())
        } else {
            Err("Wininfo already in use, cannot update parameters")
        }
    } else {
        // First time setting params
        WININFO_PARAMS
            .set(Mutex::new(params))
            .map_err(|_| "Failed to set parameters")?;
        Ok(())
    }
}

pub enum SizeDirection {
    Width,
    Height,
}

/// call init_winsize before it if you need to;
/// if not going to use 1920x1080, 100x20 fallback for when failing to query sizes
pub fn get_wininfo() -> &'static Wininfo {
    WININFO.get_or_init(|| {
        let params = if let Some(params_mutex) = WININFO_PARAMS.get() {
            params_mutex.lock().unwrap().clone()
        } else {
            // Default fallback
            WininfoParams {
                spx: Size {
                    width: 1920,
                    height: 1080,
                    force: false,
                },
                sc: Size {
                    width: 100,
                    height: 20,
                    force: false,
                },
                scalex: None,
                scaley: None,
                is_tmux: false,
                needs_inline: false,
            }
        };

        Wininfo::new(
            &params.spx,
            &params.sc,
            params.scalex,
            params.scaley,
            params.is_tmux,
            params.needs_inline,
        )
    })
}

/// Returns the horizontal offset (in cells) needed to center the image in the terminal.
/// If `is_ascii` is true, `image_width` is already in cells. Otherwise, it's in pixels.
pub fn center_image(image_width: u16, is_ascii: bool) -> u16 {
    let winsize = get_wininfo();

    let offset = if is_ascii {
        (winsize.sc_width as f32 - image_width as f32) / 2.0
    } else {
        let offset_x = (winsize.spx_width as f32 - image_width as f32) / 2.0;
        offset_x / (winsize.spx_width as f32 / winsize.sc_width as f32)
    };

    offset.round() as u16
}

/// convert any format of width / height into pixels.
/// for instance 80% would be converted to the size of screen in the direction specified * 0.8.
/// accepted formats are % (percent) / c (cells) / px (pixels) / or just number
pub fn dim_to_px(dim: &str, direction: SizeDirection) -> Result<u32, String> {
    if let Ok(num) = dim.parse::<u32>() {
        return Ok(num);
    }

    // only call it if needed
    let not_px = dim.ends_with("c") || dim.ends_with("%");
    let (spx, sc) = if not_px {
        let winsize = get_wininfo();
        match direction {
            SizeDirection::Width => (winsize.spx_width, winsize.sc_width),
            SizeDirection::Height => (winsize.spx_height, winsize.sc_height),
        }
    } else {
        (1, 1)
    };

    if dim.ends_with("px") {
        if let Ok(num) = dim.trim_end_matches("px").parse::<u32>() {
            return Ok(num);
        }
    } else if dim.ends_with("c") {
        if let Ok(num) = dim.trim_end_matches("c").parse::<u16>() {
            let value = (spx as f32 / sc as f32 * num as f32).ceil() as u32;
            return Ok(value.into());
        }
    } else if dim.ends_with("%") {
        if let Ok(num) = dim.trim_end_matches("%").parse::<f32>() {
            let normalized_percent = num / 100.0;
            let value = (spx as f32 * normalized_percent).ceil() as u32;
            return Ok(value);
        }
    }

    Err(format!("Invalid dimension format: {}", dim))
}

/// Convert any format of width / height into cells.
/// Accepted formats: % (percent), px (pixels), c (cells), or just a number (assumed cells).
pub fn dim_to_cells(dim: &str, direction: SizeDirection) -> Result<u32, String> {
    if let Ok(num) = dim.parse::<u32>() {
        return Ok(num);
    }

    // only call it if needed
    let needs_calc = dim.ends_with("px") || dim.ends_with("%");
    let (spx, sc) = if needs_calc {
        let winsize = get_wininfo();
        match direction {
            SizeDirection::Width => (winsize.spx_width, winsize.sc_width),
            SizeDirection::Height => (winsize.spx_height, winsize.sc_height),
        }
    } else {
        (1, 1) // dummy values, won’t be used
    };

    if dim.ends_with("c") {
        if let Ok(num) = dim.trim_end_matches("c").parse::<u32>() {
            return Ok(num);
        }
    } else if dim.ends_with("px") {
        if let Ok(px) = dim.trim_end_matches("px").parse::<u32>() {
            if sc == 0 || spx == 0 {
                return Err("Invalid screen size for px to cell conversion".into());
            }
            let value = (px as f32 / (spx as f32 / sc as f32)).ceil() as u32;
            return Ok(value);
        }
    } else if dim.ends_with("%") {
        if let Ok(percent) = dim.trim_end_matches("%").parse::<f32>() {
            let normalized = percent / 100.0;
            let value = (sc as f32 * normalized).ceil() as u32;
            return Ok(value);
        }
    }

    Err(format!("Invalid dimension format: {}", dim))
}

// reports the size of the logic units in cells.
pub fn report_size(width: &str, height: &str) {
    let w = dim_to_cells(width, SizeDirection::Width).unwrap_or_default();
    let h = dim_to_cells(height, SizeDirection::Height).unwrap_or_default();
    eprintln!("|width: {}, height: {}|", w, h);
}

// gross estimation winsize for windows..
#[cfg(windows)]
fn get_size_windows() -> Option<(u16, u16)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, GWL_STYLE, GetWindowLongW, WINDOW_STYLE,
    };
    use windows::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{GetClientRect, GetForegroundWindow},
    };

    let foreground_window: HWND = unsafe { GetForegroundWindow() };
    if foreground_window.is_invalid() {
        return None;
    }

    let mut client_rect = RECT::default();
    unsafe { GetClientRect(foreground_window, &mut client_rect) }.ok()?;

    let style = unsafe { GetWindowLongW(foreground_window, GWL_STYLE) };
    let mut frame_rect = RECT {
        left: 0,
        right: 0,
        bottom: 0,
        top: 0,
    };
    unsafe {
        let _ = AdjustWindowRect(&mut frame_rect, WINDOW_STYLE(style as u32), false);
    }
    let frame_width = frame_rect.right - frame_rect.left;
    let frame_height = frame_rect.bottom - frame_rect.top;

    let width = (client_rect.right - client_rect.left - frame_width) as u16;
    let height = (client_rect.bottom - client_rect.top - frame_height) as u16;

    Some((width, height))
}

pub struct EnvIdentifiers {
    pub data: HashMap<String, String>,
}

impl EnvIdentifiers {
    pub fn new() -> Self {
        let keys = vec![
            "TERM",
            "TERM_PROGRAM",
            "LC_TERMINAL",
            "VIM_TERMINAL",
            "KITTY_WINDOW_ID",
            "KONSOLE_VERSION",
            "WT_PROFILE_ID",
            "TMUX",
        ];
        let mut result = HashMap::new();

        for &key in &keys {
            if let Ok(value) = env::var(key) {
                result.insert(key.to_string(), value.to_lowercase());
            }
        }

        result.insert("OS".to_string(), env::consts::OS.to_string());

        let mut env = EnvIdentifiers { data: result };
        env.check_tmux_term();
        env
    }

    pub fn check_tmux_term(&mut self) {
        if self.is_tmux() {
            let (term_type, term_name) = get_tmux_terminal_name().unwrap_or_default();
            self.data
                .insert("TMUX_ORIGINAL_TERM".into(), term_name.to_lowercase());
            self.data
                .insert("TMUX_ORIGINAL_SPEC".into(), term_type.to_lowercase());
        }
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// all values are normalized into lowercase
    /// pass the substr as lowercase
    pub fn contains(&self, key: &str, substr: &str) -> bool {
        if self.has_key(key) {
            return self.data.get(key).is_some_and(|f| f.contains(substr));
        }
        false
    }

    /// all values are normalized into lowercase
    /// pass the term as lowercase
    pub fn term_contains(&mut self, term: &str) -> bool {
        [
            "TERM_PROGRAM",
            "TERM",
            "LC_TERMINAL",
            "TMUX_ORIGINAL_TERM",
            "TMUX_ORIGINAL_SPEC",
        ]
        .iter()
        .any(|key| self.contains(key, term))
    }

    /// checks if the current terminal is a tmux terminal
    pub fn is_tmux(&mut self) -> bool {
        self.term_contains("tmux") || self.has_key("TMUX")
    }
}

/// makes sure you have enough lines below your cursor to at least insert a `height`
pub fn ensure_space(out: &mut impl Write, height: u16) -> Result<(), Box<dyn std::error::Error>> {
    write!(out, "{}", "\n".repeat(height as usize))?;
    write!(out, "\x1B[{height}A")?;
    Ok(())
}

pub fn break_size_string(s: &str) -> Result<Size, Box<dyn std::error::Error>> {
    let mut parts = s.split("x");
    let width = parts.next().ok_or("missing width")?.parse::<u16>()?;
    let height = parts.next().ok_or("missing height")?.parse::<u16>()?;
    let force = s.contains("force");

    Ok(Size {
        width,
        height,
        force,
    })
}

/// get a handle to when the program is killed (will override so kill the program shortly after)
pub fn setup_signal_handler() -> Arc<AtomicBool> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // Register signal handlers
    flag::register(SIGINT, Arc::clone(&shutdown)).unwrap();
    flag::register(SIGTERM, Arc::clone(&shutdown)).unwrap();
    #[cfg(windows)]
    {
        flag::register(SIGBREAK, Arc::clone(&shutdown)).unwrap();
    }
    #[cfg(unix)]
    {
        flag::register(SIGHUP, Arc::clone(&shutdown)).unwrap();
        flag::register(SIGQUIT, Arc::clone(&shutdown)).unwrap();
    }

    shutdown
}
