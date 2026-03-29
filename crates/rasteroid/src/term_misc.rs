use std::{
    collections::HashMap,
    env, f32,
    io::Write,
    sync::{Arc, atomic::AtomicBool},
};

use base64::{Engine, engine::general_purpose};
use crossterm::terminal::{size, window_size};
use signal_hook::consts::signal::*;
use signal_hook::flag;

use crate::{error::RasterError, get_tmux_terminal_name};

#[derive(Clone, Debug)]
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

/// Converts a horizontal offset into a terminal escape sequence that moves the cursor right.
///
/// # Examples
/// ```
/// use rasteroid::term_misc::offset_to_terminal;
///
/// let esc = offset_to_terminal(Some(4));
/// assert_eq!(esc, "\x1b[4C");
///
/// let none = offset_to_terminal(None);
/// assert_eq!(none, "");
/// ```
pub fn offset_to_terminal(offset: Option<u16>) -> String {
    match offset {
        Some(offset) => format!("\x1b[{}C", offset),
        None => "".to_string(),
    }
}

/// Converts an (x, y) terminal position into a cursor positioning escape sequence.
///
/// # Examples
/// ```
/// use rasteroid::term_misc::loc_to_terminal;
///
/// let esc = loc_to_terminal(Some((10, 5)));
/// assert_eq!(esc, "\x1b[5;10H");
///
/// let none = loc_to_terminal(None);
/// assert_eq!(none, "");
/// ```
pub fn loc_to_terminal(at: Option<(u16, u16)>) -> String {
    match at {
        Some((x, y)) => format!("\x1b[{y};{x}H"),
        None => "".to_string(),
    }
}

fn parse_dimension(s: &str) -> Result<(Option<u16>, Option<u16>), RasterError> {
    let parts: Vec<&str> = s.splitn(2, 'x').collect();
    if parts.len() != 2 {
        return Err(RasterError::InvalidSizeFormat);
    }
    let parse = |p: &str| -> Result<Option<u16>, RasterError> {
        if p.eq_ignore_ascii_case("auto") {
            Ok(None)
        } else {
            p.parse::<u16>()
                .map(Some)
                .map_err(|_| RasterError::InvalidSizeFormat)
        }
    };
    Ok((parse(parts[0])?, parse(parts[1])?))
}

impl Wininfo {
    /// Creates a new `Wininfo` by auto-detecting terminal dimensions and applying any overrides.
    ///
    /// # Arguments
    /// * `spx` - Optional pixel bounding box override (e.g. `"1920x1080"`, `"autox1080"`, `"1920xauto"`)
    /// * `sc` - Optional column x row bounding box override (e.g. `"100x20"`, `"autox20"`, `"100xauto"`)
    /// * `scalex` - Optional scale multiplier applied over spx and sc
    /// * `scaley` - Optional scale multiplier applied over spx and sc
    /// * `env` - Terminal env identifiers used for auto detection
    ///
    /// # Examples
    /// ```
    /// use rasteroid::term_misc::{EnvIdentifiers, Wininfo};
    ///
    /// let env = EnvIdentifiers::new();
    ///
    /// // fully auto-detected
    /// let wininfo = Wininfo::new(None, None, None, None, &env).unwrap();
    ///
    /// // override only pixel width, auto-detect height
    /// let wininfo = Wininfo::new(Some("1920xauto"), None, None, None, &env).unwrap();
    ///
    /// // override columns, scale everything down by half
    /// let wininfo = Wininfo::new(None, Some("100xauto"), Some(0.5), Some(0.5), &env).unwrap();
    /// ```
    pub fn new(
        spx: Option<&str>,
        sc: Option<&str>,
        scalex: Option<f32>,
        scaley: Option<f32>,
        env: &EnvIdentifiers,
    ) -> Result<Self, RasterError> {
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

        if let Some(spx) = spx {
            let (w, h) = parse_dimension(spx)?;
            if let Some(w) = w {
                spx_width = w;
            }
            if let Some(h) = h {
                spx_height = h;
            }
        }
        if let Some(sc) = sc {
            let (w, h) = parse_dimension(sc)?;
            if let Some(w) = w {
                sc_width = w;
            }
            if let Some(h) = h {
                sc_height = h;
            }
        }

        let scalex = scalex.unwrap_or(1.0);
        let scaley = scaley.unwrap_or(1.0);

        Ok(Wininfo {
            sc_height: (sc_height as f32 * scaley) as u16,
            sc_width: (sc_width as f32 * scalex) as u16,
            spx_height: (spx_height as f32 * scaley) as u16,
            spx_width: (spx_width as f32 * scalex) as u16,
            is_tmux: env.is_tmux(),
            needs_inline: false,
        })
    }
}

pub enum SizeDirection {
    Width,
    Height,
}

impl Wininfo {
    /// Calculates the horizontal cell offset needed to center an image in the terminal.
    ///
    /// # Examples
    /// ```
    /// use rasteroid::term_misc::{EnvIdentifiers, Wininfo};
    ///
    /// let env = EnvIdentifiers::new();
    /// let wininfo = Wininfo::new(None, None, None, None, &env).unwrap();
    ///
    /// let offset = wininfo.center_offset(800, false); // pixel-based image
    /// let offset = wininfo.center_offset(40, true);   // ascii image, already in cells
    /// ```
    pub fn center_offset(&self, image_width: u16, is_cells: bool) -> u16 {
        let offset = if is_cells {
            (self.sc_width as f32 - image_width as f32) / 2.0
        } else {
            let offset_x = (self.spx_width as f32 - image_width as f32) / 2.0;
            offset_x / (self.spx_width as f32 / self.sc_width as f32)
        };
        offset.max(0.0).round() as u16
    }

    /// Converts a dimension string into pixels based on the terminal's current size.
    ///
    /// # Accepted Formats
    /// * `"1920"` or `"1920px"` - explicit pixel value
    /// * `"40c"` - terminal cells
    /// * `"80%"` - percentage of the terminal size
    ///
    /// # Examples
    /// ```
    /// use rasteroid::term_misc::{EnvIdentifiers, Wininfo, SizeDirection};
    ///
    /// let env = EnvIdentifiers::new();
    /// let wininfo = Wininfo::new(None, None, None, None, &env).unwrap();
    ///
    /// let px = wininfo.dim_to_px("80%", SizeDirection::Width).unwrap();
    /// let px = wininfo.dim_to_px("40c", SizeDirection::Height).unwrap();
    /// let px = wininfo.dim_to_px("1920px", SizeDirection::Width).unwrap();
    /// let px = wininfo.dim_to_px("1920", SizeDirection::Width).unwrap();
    /// ```
    pub fn dim_to_px(&self, dim: &str, direction: SizeDirection) -> Result<u32, RasterError> {
        if let Ok(num) = dim.parse::<u32>() {
            return Ok(num);
        }

        let not_px = dim.ends_with("c") || dim.ends_with("%");
        let (spx, sc) = if not_px {
            match direction {
                SizeDirection::Width => (self.spx_width, self.sc_width),
                SizeDirection::Height => (self.spx_height, self.sc_height),
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
                return Ok(value);
            }
        } else if dim.ends_with("%")
            && let Ok(num) = dim.trim_end_matches("%").parse::<f32>()
        {
            let normalized_percent = num / 100.0;
            let value = (spx as f32 * normalized_percent).ceil() as u32;
            return Ok(value);
        }

        Err(RasterError::InvalidDimensionFormat)
    }

    /// Converts a dimension string into terminal cells based on the terminal's current size.
    ///
    /// # Accepted Formats
    /// * `"40"` or `"40c"` - explicit cell value
    /// * `"1920px"` - pixels
    /// * `"80%"` - percentage of the terminal size
    ///
    /// # Examples
    /// ```
    /// use rasteroid::term_misc::{EnvIdentifiers, Wininfo, SizeDirection};
    ///
    /// let env = EnvIdentifiers::new();
    /// let wininfo = Wininfo::new(None, None, None, None, &env).unwrap();
    ///
    /// let cells = wininfo.dim_to_cells("80%", SizeDirection::Width).unwrap();
    /// let cells = wininfo.dim_to_cells("40c", SizeDirection::Height).unwrap();
    /// let cells = wininfo.dim_to_cells("1920px", SizeDirection::Width).unwrap();
    /// let cells = wininfo.dim_to_cells("40", SizeDirection::Width).unwrap();
    /// ```
    pub fn dim_to_cells(&self, dim: &str, direction: SizeDirection) -> Result<u32, RasterError> {
        if let Ok(num) = dim.parse::<u32>() {
            return Ok(num);
        }

        let needs_calc = dim.ends_with("px") || dim.ends_with("%");
        let (spx, sc) = if needs_calc {
            match direction {
                SizeDirection::Width => (self.spx_width, self.sc_width),
                SizeDirection::Height => (self.spx_height, self.sc_height),
            }
        } else {
            (1, 1)
        };

        if dim.ends_with("c") {
            if let Ok(num) = dim.trim_end_matches("c").parse::<u32>() {
                return Ok(num);
            }
        } else if dim.ends_with("px") {
            if let Ok(px) = dim.trim_end_matches("px").parse::<u32>() {
                if sc == 0 || spx == 0 {
                    return Err(RasterError::InvalidDimensionFormat);
                }
                let value = (px as f32 / (spx as f32 / sc as f32)).ceil() as u32;
                return Ok(value);
            }
        } else if dim.ends_with("%")
            && let Ok(percent) = dim.trim_end_matches("%").parse::<f32>()
        {
            let normalized = percent / 100.0;
            let value = (sc as f32 * normalized).ceil() as u32;
            return Ok(value);
        }

        Err(RasterError::InvalidDimensionFormat)
    }
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

#[derive(Clone)]
pub struct EnvIdentifiers {
    pub data: HashMap<String, String>,
}

impl Default for EnvIdentifiers {
    fn default() -> Self {
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
}

impl EnvIdentifiers {
    pub fn new() -> Self {
        Self::default()
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

    pub fn contains(&self, key: &str, substr: &str) -> bool {
        if self.has_key(key) {
            return self.data.get(key).is_some_and(|f| f.contains(substr));
        }
        false
    }

    pub fn term_contains(&self, term: &str) -> bool {
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

    pub fn is_tmux(&self) -> bool {
        self.term_contains("tmux") || self.has_key("TMUX")
    }
}

/// Ensures there are enough lines below the cursor to insert content of the given height.
/// Achieves this by printing `height` newlines to scroll the terminal if needed,
/// then moving the cursor back up by the same amount.
///
/// # Examples
/// ```
/// use rasteroid::term_misc::ensure_space;
///
/// let mut buf = Vec::new();
/// ensure_space(&mut buf, 3).unwrap();
/// assert_eq!(buf, b"\n\n\n\x1B[3A");
/// ```
pub fn ensure_space(out: &mut impl Write, height: u16) -> Result<(), RasterError> {
    write!(out, "{}", "\n".repeat(height as usize))?;
    write!(out, "\x1B[{height}A")?;
    Ok(())
}

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
