use std::{collections::HashMap, env};

pub fn get_env_identifiers() -> HashMap<String, String> {
    let keys = vec![
        "TERM",
        "TERM_PROGRAM",
        "LC_TERMINAL",
        "VIM_TERMINAL",
        "KITTY_WINDOW_ID",
    ];
    let mut result = HashMap::new();

    for &key in &keys {
        if let Ok(value) = env::var(key) {
            result.insert(key.to_string(), value.to_lowercase());
        }
    }

    result.insert("OS".to_string(), env::consts::OS.to_string());

    result
}
