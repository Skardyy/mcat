use std::{collections::HashMap, env};

use crate::media_encoder::ResizeMode;

pub fn dim_to_px(dim: &str) -> Result<u32, String> {
    if let Ok(num) = dim.parse::<u32>() {
        return Ok(num);
    }

    if dim.ends_with("px") {
        if let Ok(num) = dim.trim_end_matches("px").parse::<u32>() {
            return Ok(num);
        }
    } else if dim.ends_with("c") {
        if let Ok(num) = dim.trim_end_matches("c").parse::<u32>() {
            return Ok(num);
        }
    } else if dim.ends_with("%") {
        if let Ok(num) = dim.trim_end_matches("%").parse::<u32>() {
            return Ok(num);
        }
    }

    Err(format!("Invalid dimension format: {}", dim))
}

pub fn parse_resize_mode(resize_mode: &str) -> ResizeMode {
    match resize_mode {
        "fit" => ResizeMode::Fit,
        "crop" => ResizeMode::Crop,
        "strech" => ResizeMode::Strech,
        _ => ResizeMode::Fit,
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
