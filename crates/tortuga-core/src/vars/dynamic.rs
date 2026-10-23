//! Dynamic variables (R-VAR-8, D52): `{{$guid}}` and the like, a new value wherever
//! they are filled in. The names are the ones Postman and the `.http` clients share:
//!
//! - `$guid`, `$uuid`, `$random.uuid`: a random UUID (version 4);
//! - `$timestamp`: the time in seconds since 1970 (UTC);
//! - `$isoTimestamp`: the time in ISO 8601, UTC, with milliseconds;
//! - `$randomInt`: a whole number from 0 to 1000; with a range, as REST Client writes
//!   it (`$randomInt 1 10`) or the JetBrains client does (`$random.integer(1, 10)`),
//!   from the first number up to but not including the second.
//!
//! The values are for test data, not secrets.

use chrono::{DateTime, SecondsFormat};
use web_time::{SystemTime, UNIX_EPOCH};

/// What a dynamic variable makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dynamic {
    Uuid,
    Timestamp,
    IsoTimestamp,
    /// From `min` up to but not including `max`.
    RandomInt {
        min: i64,
        max: i64,
    },
}

/// The dynamic variables to suggest after `{{` (R-VAR-9).
pub const SUGGESTED: [&str; 6] = [
    "$guid",
    "$uuid",
    "$timestamp",
    "$isoTimestamp",
    "$randomInt",
    "$random.uuid",
];

/// What the dynamic variable `name` makes, for the UI to describe (R-VAR-9).
pub fn kind(name: &str) -> Option<Dynamic> {
    parse(name)
}

/// Whether `name` (as written between `{{` and `}}`, trimmed) is a dynamic variable.
pub fn is_dynamic(name: &str) -> bool {
    parse(name).is_some()
}

/// A new value for the dynamic variable `name`; `None` when it isn't one.
pub(crate) fn value(name: &str) -> Option<String> {
    Some(match parse(name)? {
        Dynamic::Uuid => uuid(rand::random()),
        Dynamic::Timestamp => (now_ms() / 1000).to_string(),
        Dynamic::IsoTimestamp => iso(now_ms()),
        Dynamic::RandomInt { min, max } => rand::random_range(min..max).to_string(),
    })
}

fn parse(name: &str) -> Option<Dynamic> {
    match name {
        "$guid" | "$uuid" | "$random.uuid" => return Some(Dynamic::Uuid),
        "$timestamp" => return Some(Dynamic::Timestamp),
        "$isoTimestamp" => return Some(Dynamic::IsoTimestamp),
        "$randomInt" => return Some(Dynamic::RandomInt { min: 0, max: 1001 }),
        _ => {}
    }
    let (min, max) = if let Some(args) = name.strip_prefix("$randomInt ") {
        let mut words = args.split_whitespace();
        let range = (words.next()?, words.next()?);
        if words.next().is_some() {
            return None;
        }
        range
    } else {
        let args = name.strip_prefix("$random.integer(")?.strip_suffix(')')?;
        let (min, max) = args.split_once(',')?;
        (min.trim(), max.trim())
    };
    let (min, max) = (min.parse().ok()?, max.parse().ok()?);
    (min < max).then_some(Dynamic::RandomInt { min, max })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `2026-10-21T09:15:02.123Z`.
fn iso(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// A version 4 UUID from 128 random bits, in lower case with hyphens.
fn uuid(bits: u128) -> String {
    // Version 4 in bits 76-79, variant `10` in bits 62-63 (RFC 9562).
    let bits = (bits & !(0xF << 76)) | (0x4 << 76);
    let bits = (bits & !(0x3 << 62)) | (0x2 << 62);
    let hex = format!("{bits:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_names_the_tools_share() {
        for name in [
            "$guid",
            "$uuid",
            "$random.uuid",
            "$timestamp",
            "$isoTimestamp",
            "$randomInt",
            "$randomInt 1 10",
            "$randomInt -5   5",
            "$random.integer(1, 10)",
            "$random.integer(-3,3)",
        ] {
            assert!(is_dynamic(name), "{name}");
        }
        for name in [
            "guid",
            "$GUID",
            "$randomFirstName",
            "$randomInt 10 1",
            "$randomInt 1 1",
            "$randomInt 1",
            "$randomInt 1 2 3",
            "$randomInt a b",
            "$random.integer(1, 10",
            "$random.integer(1)",
            "$randomInt ü 😀",
            "",
        ] {
            assert!(!is_dynamic(name), "{name}");
            assert_eq!(value(name), None);
        }
    }

    #[test]
    fn uuids_are_version_4() {
        assert_eq!(uuid(0), "00000000-0000-4000-8000-000000000000");
        assert_eq!(uuid(u128::MAX), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let a = value("$guid").expect("uuid");
        let b = value("$uuid").expect("uuid");
        assert_ne!(a, b, "a new value each time");
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().nth(14), Some('4'));
        assert!(matches!(a.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
    }

    #[test]
    fn times_and_numbers() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_792_573_502_123), "2026-10-21T09:05:02.123Z");
        let seconds: i64 = value("$timestamp").expect("ts").parse().expect("number");
        assert!(seconds > 1_700_000_000);
        let iso_now = value("$isoTimestamp").expect("iso");
        assert!(iso_now.ends_with('Z') && iso_now.len() == 24, "{iso_now}");
        for _ in 0..200 {
            let n: i64 = value("$randomInt").expect("int").parse().expect("number");
            assert!((0..=1000).contains(&n));
            let n: i64 = value("$randomInt 3 5")
                .expect("int")
                .parse()
                .expect("number");
            assert!((3..5).contains(&n));
            let n: i64 = value("$random.integer(-2, -1)")
                .expect("int")
                .parse()
                .expect("n");
            assert_eq!(n, -2);
        }
    }
}
