use std::{collections::HashMap, env, f32, sync::OnceLock};

use crossterm::terminal::{size, window_size};

pub struct Winsize {
    pub sc_width: u16,
    pub sc_height: u16,
    pub spx_width: u16,
    pub spx_height: u16,
}

lazy_static! {
    static ref WINSIZE: OnceLock<Winsize> = OnceLock::new();
}

#[derive(Clone)]
pub struct Size {
    pub width: u16,
    pub height: u16,
    force: bool,
    scale: f32,
}

pub fn break_size_string(s: &str) -> Result<Size, Box<dyn std::error::Error>> {
    let mut parts = s.split("x");
    let width = parts.next().ok_or("missing width")?.parse::<u16>()?;
    let height = parts.next().ok_or("missing height")?.parse::<u16>()?;
    let scale = parts.next().unwrap_or("1").parse::<f32>().unwrap_or(1.0);
    let force = s.contains("force");

    Ok(Size {
        width,
        height,
        force,
        scale,
    })
}
impl Winsize {
    fn new(spx_fallback: &Size, sc_fallback: &Size) -> Self {
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

        Winsize {
            sc_height,
            sc_width,
            spx_height,
            spx_width,
        }
    }
}

pub fn init_winsize(spx: &Size, sc: &Size) -> Result<(), &'static str> {
    WINSIZE
        .set(Winsize::new(spx, sc))
        .map_err(|_| "Winsize already initialized")?;
    Ok(())
}

pub enum SizeDirection {
    WIDTH,
    HEIGHT,
}

/// call init_winsize before it if you need to
/// if not going to use 1920x1080, 100x20 fallback for when failing to query sizes
pub fn get_winsize() -> &'static Winsize {
    WINSIZE.get_or_init(|| {
        let spx = Size {
            width: 1920,
            height: 1080,
            force: false,
            scale: 1.0,
        };
        let sc = Size {
            width: 100,
            height: 20,
            force: false,
            scale: 1.0,
        };
        Winsize::new(&spx, &sc)
    })
}

/// returns a the offset needed to center the image
pub fn center_image(image_width: u16) -> u16 {
    let winsize = get_winsize();
    let offset_x = (winsize.spx_width as f32 - image_width as f32) / 2.0;
    let offset_x = offset_x / (winsize.spx_width as f32 / winsize.sc_width as f32);

    offset_x.round() as u16
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
    let (width, height) = if not_px {
        let winsize = get_winsize();
        match direction {
            SizeDirection::WIDTH => (winsize.spx_width, winsize.sc_width),
            SizeDirection::HEIGHT => (winsize.spx_height, winsize.sc_height),
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
            let value = width / height * num;
            return Ok(value.into());
        }
    } else if dim.ends_with("%") {
        if let Ok(num) = dim.trim_end_matches("%").parse::<f32>() {
            let normalized_percent = num / 100.0;
            let value = (width as f32 * normalized_percent).round() as u32;
            return Ok(value);
        }
    }

    Err(format!("Invalid dimension format: {}", dim))
}

// gross estimation winsize for windows..
#[cfg(windows)]
fn get_size_windows() -> Option<(u16, u16)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, GetWindowLongW, GWL_STYLE, WINDOW_STYLE,
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
        let _ = AdjustWindowRect(&mut frame_rect, WINDOW_STYLE(style as u32), false.into());
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
        ];
        let mut result = HashMap::new();

        for &key in &keys {
            if let Ok(value) = env::var(key) {
                result.insert(key.to_string(), value.to_lowercase());
            }
        }

        result.insert("OS".to_string(), env::consts::OS.to_string());

        EnvIdentifiers { data: result }
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    pub fn contains(&self, key: &str, substr: &str) -> bool {
        if self.has_key(key) {
            return self.data[key]
                .to_lowercase()
                .contains(&substr.to_lowercase());
        }
        false
    }

    pub fn term_contains(&self, term: &str) -> bool {
        self.contains("TERM_PROGRAM", term)
            || self.contains("TERM", term)
            || self.contains("LC_TERMINAL", term)
    }
}
