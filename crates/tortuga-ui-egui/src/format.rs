//! Display formatting for the UI.

use std::time::Duration;

/// Largest body shown in the viewer; bigger bodies are truncated (R-NF-2).
pub const MAX_DISPLAY_BYTES: usize = 2 * 1024 * 1024;

#[expect(clippy::cast_precision_loss, reason = "display only")]
pub fn size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

pub fn elapsed(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

/// Capitalises the first letter (Rust error messages are lower-case by convention).
pub fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Cuts `text` to at most `max` bytes on a char boundary. Returns whether it was cut.
pub fn truncate(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(1023), "1023 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn durations() {
        assert_eq!(elapsed(Duration::from_millis(42)), "42 ms");
        assert_eq!(elapsed(Duration::from_millis(1234)), "1.23 s");
    }

    #[test]
    fn sentences_start_upper_case() {
        assert_eq!(sentence("request timed out"), "Request timed out");
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncate("hello", 10), ("hello", false));
        assert_eq!(truncate("hello", 3), ("hel", true));
        // 'é' is two bytes; cutting in its middle must back off.
        assert_eq!(truncate("aé", 2), ("a", true));
    }
}
