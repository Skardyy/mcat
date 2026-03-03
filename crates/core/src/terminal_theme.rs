use std::time::Duration;

/// Maximum time spent probing the terminal background before falling back.
const DETECTION_TIMEOUT: Duration = Duration::from_millis(75);

pub fn detect_terminal_background() -> Option<termbg::Theme> {
    termbg::theme(DETECTION_TIMEOUT).ok()
}

#[cfg(test)]
mod tests {
    use super::detect_terminal_background;

    #[test]
    fn detector_is_callable() {
        let _ = detect_terminal_background();
    }
}
