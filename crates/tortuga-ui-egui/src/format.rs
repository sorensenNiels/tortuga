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

/// Days since 1970-01-01 in local time, for a UTC time in milliseconds.
fn local_days(ms: u64, offset_minutes: i32) -> i64 {
    let local_ms = i64::try_from(ms).unwrap_or(i64::MAX) + i64::from(offset_minutes) * 60_000;
    local_ms.div_euclid(86_400_000)
}

/// Year, month (1-12) and day of a day number (days since 1970-01-01).
/// Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// "Today", "Yesterday", or a date like "Mon 13 Oct" (with the year when it isn't this
/// year), in local time (R-HIST-2).
pub fn day_label(ms: u64, now_ms: u64, offset_minutes: i32) -> String {
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (day, today) = (
        local_days(ms, offset_minutes),
        local_days(now_ms, offset_minutes),
    );
    match today - day {
        0 => return "Today".into(),
        1 => return "Yesterday".into(),
        _ => {}
    }
    let (year, month, date) = civil(day);
    // 1970-01-01 was a Thursday.
    let weekday = WEEKDAYS[usize::try_from((day + 3).rem_euclid(7)).unwrap_or(0)];
    let month = MONTHS[(month as usize).saturating_sub(1).min(11)];
    if year == civil(today).0 {
        format!("{weekday} {date} {month}")
    } else {
        format!("{date} {month} {year}")
    }
}

/// "1 step", "2 steps".
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Local time of day, like "14:05".
pub fn clock(ms: u64, offset_minutes: i32) -> String {
    let local_ms = i64::try_from(ms).unwrap_or(i64::MAX) + i64::from(offset_minutes) * 60_000;
    let minutes = local_ms.div_euclid(60_000).rem_euclid(24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
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

    #[test]
    fn days_and_clock_times_in_local_time() {
        // 2026-10-17 12:30 UTC (a Saturday).
        let now = 1_792_240_200_000;
        assert_eq!(clock(now, 0), "12:30");
        assert_eq!(clock(now, 120), "14:30");
        assert_eq!(clock(now, -13 * 60), "23:30");
        assert_eq!(day_label(now, now, 120), "Today");
        assert_eq!(day_label(now - 86_400_000, now, 120), "Yesterday");
        assert_eq!(day_label(now - 3 * 86_400_000, now, 120), "Wed 14 Oct");
        assert_eq!(day_label(now - 300 * 86_400_000, now, 0), "21 Dec 2025");
        // 23:30 local the day before is yesterday, though it is the same UTC day.
        assert_eq!(day_label(now, now + 60_000 * 60, -13 * 60), "Yesterday");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
    }
}
