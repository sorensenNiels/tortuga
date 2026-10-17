//! JSON helpers for the body editor (R-HTTP-4).

/// Returns a human-readable parse error, or `None` if `text` is valid JSON.
/// Blank text counts as valid (nothing to send yet).
pub fn check(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(text)
        .err()
        .map(|e| {
            format!(
                "Invalid JSON at line {}, column {}: {}",
                e.line(),
                e.column(),
                describe(&e)
            )
        })
}

/// Pretty-prints `text`, keeping key order. `None` if it is not valid JSON.
pub fn pretty(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

fn describe(e: &serde_json::Error) -> String {
    // serde_json appends " at line X column Y"; we report that separately.
    let msg = e.to_string();
    msg.split(" at line ").next().unwrap_or(&msg).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_and_blank_json_have_no_error() {
        assert_eq!(check(r#"{"a": [1, 2]}"#), None);
        assert_eq!(check("   "), None);
    }

    #[test]
    fn error_reports_position() {
        let err = check("{\n  \"a\": 1,\n}").expect("invalid");
        assert!(
            err.starts_with("Invalid JSON at line 3, column 1:"),
            "{err}"
        );
        assert!(
            !err.contains(" at line 3 column"),
            "position not repeated: {err}"
        );
    }

    #[test]
    fn pretty_keeps_key_order() {
        assert_eq!(
            pretty(r#"{"b":1,"a":2}"#).as_deref(),
            Some("{\n  \"b\": 1,\n  \"a\": 2\n}")
        );
        assert_eq!(pretty("nope"), None);
    }
}
