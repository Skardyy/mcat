use std::{collections::HashMap, env};

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
