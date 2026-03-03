use std::time::Duration;

pub fn detect_terminal_background() -> Option<termbg::Theme> {
    termbg::theme(Duration::from_millis(250)).ok()
}

#[cfg(test)]
mod tests {
    use super::detect_terminal_background;

    #[test]
    fn detector_is_callable() {
        let _ = detect_terminal_background();
    }
}
