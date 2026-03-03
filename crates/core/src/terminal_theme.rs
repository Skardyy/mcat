use std::time::Duration;

/// Maximum time spent probing the terminal background before falling back.
const DETECTION_TIMEOUT: Duration = Duration::from_millis(75);

pub fn detect_terminal_background() -> Option<termbg::Theme> {
    termbg::theme(DETECTION_TIMEOUT).ok()
}

pub fn detect_terminal_background_if_tty(stdout_is_tty: bool) -> Option<termbg::Theme> {
    detect_terminal_background_with(stdout_is_tty, detect_terminal_background)
}

fn detect_terminal_background_with<F>(stdout_is_tty: bool, detector: F) -> Option<termbg::Theme>
where
    F: FnOnce() -> Option<termbg::Theme>,
{
    if stdout_is_tty { detector() } else { None }
}

#[cfg(test)]
mod tests {
    use super::{detect_terminal_background, detect_terminal_background_with};

    #[test]
    fn detector_is_callable() {
        let _ = detect_terminal_background();
    }

    #[test]
    fn detector_is_skipped_when_stdout_is_not_a_tty() {
        let detected = detect_terminal_background_with(false, || panic!("detector should not run"));

        assert_eq!(detected, None);
    }

    #[test]
    fn detector_runs_when_stdout_is_a_tty() {
        let detected = detect_terminal_background_with(true, || Some(termbg::Theme::Light));

        assert_eq!(detected, Some(termbg::Theme::Light));
    }
}
