//! Display formatting for the UI.

use std::time::Duration;

use tortuga_core::settings::Level;
use tortuga_core::vars::Origin;

use crate::i18n;

/// Largest body shown in the viewer; bigger bodies are truncated (R-NF-2).
pub const MAX_DISPLAY_BYTES: usize = 2 * 1024 * 1024;

#[expect(clippy::cast_precision_loss, reason = "display only")]
pub fn size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{} KB", i18n::decimal(b / KB, 1))
    } else {
        format!("{} MB", i18n::decimal(b / (KB * KB), 1))
    }
}

pub fn elapsed(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{} s", i18n::decimal(d.as_secs_f64(), 2))
    }
}

/// A phase of a send (R-HTTP-25): under 10 ms with a decimal, so a fast lookup on a
/// local network doesn't read "0 ms".
pub fn phase(d: Duration) -> String {
    if d < Duration::from_millis(10) {
        format!("{} ms", i18n::decimal(d.as_secs_f64() * 1000.0, 1))
    } else {
        elapsed(d)
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

/// Where a default header or auth comes from: "the project", "folder “Users”".
pub fn origin(origin: &Origin) -> String {
    match origin {
        Origin::Project => t!("level-project"),
        Origin::Workspace => t!("level-workspace"),
        Origin::Folder(name) => t!("level-folder", name = name),
    }
}

/// Where a setting comes from (D39): "the project", "the workspace", "the app's settings".
pub fn level(level: Level) -> String {
    match level {
        Level::Project => t!("level-project"),
        Level::Workspace => t!("level-workspace"),
        Level::App => t!("level-app"),
    }
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

/// The first line of a Markdown description, without heading or quote marks, cut to a
/// length that fits a tooltip (R-COL-22); `None` when there is no text.
pub fn summary_line(description: &str) -> Option<String> {
    let line = description
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '>']).trim())
        .find(|l| !l.is_empty())?;
    let (cut, was_cut) = truncate(line, 120);
    Some(if was_cut {
        format!("{cut}…")
    } else {
        cut.to_owned()
    })
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
/// year), in local time (R-HIST-2) and the UI's language (R-I18N-3).
pub fn day_label(ms: u64, now_ms: u64, offset_minutes: i32) -> String {
    let (day, today) = (
        local_days(ms, offset_minutes),
        local_days(now_ms, offset_minutes),
    );
    match today - day {
        0 => return t!("date-today"),
        1 => return t!("date-yesterday"),
        _ => {}
    }
    let (year, month, date) = civil(day);
    let month = month_name(month);
    if year == civil(today).0 {
        // 1970-01-01 was a Thursday.
        let weekday = weekday_name((day + 3).rem_euclid(7));
        t!(
            "date-this-year",
            weekday = weekday,
            day = date,
            month = month
        )
    } else {
        t!("date-other-year", day = date, month = month, year = year)
    }
}

/// The short name of a weekday, 0 being Monday.
fn weekday_name(weekday: i64) -> String {
    match weekday {
        0 => t!("weekday-mon"),
        1 => t!("weekday-tue"),
        2 => t!("weekday-wed"),
        3 => t!("weekday-thu"),
        4 => t!("weekday-fri"),
        5 => t!("weekday-sat"),
        _ => t!("weekday-sun"),
    }
}

/// The short name of a month, 1 being January.
fn month_name(month: u32) -> String {
    match month {
        1 => t!("month-jan"),
        2 => t!("month-feb"),
        3 => t!("month-mar"),
        4 => t!("month-apr"),
        5 => t!("month-may"),
        6 => t!("month-jun"),
        7 => t!("month-jul"),
        8 => t!("month-aug"),
        9 => t!("month-sep"),
        10 => t!("month-oct"),
        11 => t!("month-nov"),
        _ => t!("month-dec"),
    }
}

/// Local time of day, like "14:05".
pub fn clock(ms: u64, offset_minutes: i32) -> String {
    let local_ms = i64::try_from(ms).unwrap_or(i64::MAX) + i64::from(offset_minutes) * 60_000;
    let minutes = local_ms.div_euclid(60_000).rem_euclid(24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Local time of day with seconds, like "14:05:09".
pub fn clock_seconds(ms: u64, offset_minutes: i32) -> String {
    let local_ms = i64::try_from(ms).unwrap_or(i64::MAX) + i64::from(offset_minutes) * 60_000;
    let seconds = local_ms.div_euclid(1000).rem_euclid(24 * 3600);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
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
        // R-I18N-3
        crate::i18n::set(tortuga_core::settings::Language::Danish);
        assert_eq!(size(1536), "1,5 KB");
        crate::i18n::set(tortuga_core::settings::Language::English);
    }

    #[test]
    fn durations() {
        assert_eq!(elapsed(Duration::from_millis(42)), "42 ms");
        assert_eq!(elapsed(Duration::from_millis(1234)), "1.23 s");
        // R-HTTP-25
        assert_eq!(phase(Duration::from_micros(400)), "0.4 ms");
        assert_eq!(phase(Duration::ZERO), "0.0 ms");
        assert_eq!(phase(Duration::from_millis(48)), "48 ms");
        // R-I18N-3
        crate::i18n::set(tortuga_core::settings::Language::Danish);
        assert_eq!(elapsed(Duration::from_millis(1234)), "1,23 s");
        assert_eq!(phase(Duration::from_micros(2500)), "2,5 ms");
        crate::i18n::set(tortuga_core::settings::Language::English);
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
        crate::i18n::set(tortuga_core::settings::Language::Danish);
        assert_eq!(day_label(now, now, 120), "I dag");
        assert_eq!(day_label(now - 86_400_000, now, 120), "I går");
        assert_eq!(day_label(now - 3 * 86_400_000, now, 120), "ons. 14. okt.");
        assert_eq!(day_label(now - 300 * 86_400_000, now, 0), "21. dec. 2025");
        crate::i18n::set(tortuga_core::settings::Language::English);
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn clock_with_seconds_in_local_time() {
        assert_eq!(clock_seconds(3_723_000, 0), "01:02:03");
        assert_eq!(clock_seconds(3_723_000, -120), "23:02:03", "the day before");
        assert_eq!(clock_seconds(u64::MAX, 0).len(), 8);
    }

    /// R-COL-22: a description's first line, for a tooltip.
    #[test]
    fn summary_line_of_a_description() {
        assert_eq!(summary_line(""), None);
        assert_eq!(summary_line("  \n\n#\n"), None);
        assert_eq!(
            summary_line("\n## Orders ü😀\n\nMore.").as_deref(),
            Some("Orders ü😀")
        );
        assert_eq!(summary_line("> quoted").as_deref(), Some("quoted"));
        let long = "é".repeat(100);
        let line = summary_line(&long).expect("a line");
        assert!(line.ends_with('…'));
        assert!(line.len() <= 120 + '…'.len_utf8());
    }
}
