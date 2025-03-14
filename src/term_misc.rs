use std::{collections::HashMap, env, f32};

use crossterm::terminal::{size, window_size};

lazy_static! {
    static ref WINSIZE: Winsize = Winsize::new();
}

pub enum SizeDirection {
    WIDTH,
    HEIGHT,
}

pub fn center_image(image_width: u16) -> u16 {
    let offset_x = (WINSIZE.spx_width as f32 - image_width as f32) / 2.0;
    let offset_x = offset_x / (WINSIZE.spx_width as f32 / WINSIZE.sc_width as f32);

    offset_x.round() as u16
}

pub fn dim_to_px(dim: &str, direction: SizeDirection) -> Result<u32, String> {
    if let Ok(num) = dim.parse::<u32>() {
        return Ok(num);
    }

    // only call it if needed
    let not_px = dim.ends_with("c") || dim.ends_with("%");
    let (width, height) = if not_px {
        match direction {
            SizeDirection::WIDTH => (WINSIZE.spx_width, WINSIZE.sc_width),
            SizeDirection::HEIGHT => (WINSIZE.spx_height, WINSIZE.sc_height),
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

pub struct Winsize {
    pub sc_width: u16,
    pub sc_height: u16,
    pub spx_width: u16,
    pub spx_height: u16,
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

impl Winsize {
    pub fn new() -> Self {
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
        let cells = size().unwrap_or((0, 0));
        Winsize {
            sc_height: cells.1,
            sc_width: cells.0,
            spx_height,
            spx_width,
        }
    }
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
