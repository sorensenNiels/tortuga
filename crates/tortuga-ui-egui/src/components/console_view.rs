//! The Console of a send (R-TEST-9): what the scripts logged, their tests, breakpoint hits
//! (R-TEST-10) and errors, in order, each with its line. Objects and arrays fold open.

use egui::{CollapsingHeader, Grid, Label, RichText, Ui};
use serde_json::Value;
use tortuga_core::script::{ConsoleEntry, LogLevel, ScriptKind};

use crate::format;
use crate::theme;

/// Longest text shown for one value before it is cut short.
const MAX_TEXT: usize = 2_000;

/// Whether the console has an error, so its tab shows red.
pub fn console_went_wrong(entries: &[(ScriptKind, ConsoleEntry)]) -> bool {
    entries.iter().any(|(_, e)| match e {
        ConsoleEntry::Error { .. } => true,
        ConsoleEntry::Log { level, .. } => *level == LogLevel::Error,
        ConsoleEntry::Test { .. } | ConsoleEntry::Break { .. } | ConsoleEntry::Note { .. } => false,
    })
}

pub fn console_view(ui: &mut Ui, entries: &[(ScriptKind, ConsoleEntry)]) {
    if entries.is_empty() {
        theme::hint(
            ui,
            "Nothing was logged. Use console.log(…) in a script, or click a line number in the \
             Scripts tab to set a breakpoint.",
        );
        return;
    }
    let mut script = None;
    for (n, (kind, entry)) in entries.iter().enumerate() {
        if script != Some(*kind) {
            script = Some(*kind);
            if n > 0 {
                ui.add_space(6.0);
            }
            theme::section_label(ui, &script_name(*kind));
        }
        ui.push_id(n, |ui| entry_row(ui, entry));
    }
}

fn script_name(kind: ScriptKind) -> String {
    match kind {
        ScriptKind::PreRequest => t!("console-pre-request"),
        ScriptKind::Tests => t!("tests"),
        ScriptKind::StepTests => t!("console-step-tests"),
    }
}

/// "line 3", muted, in a column of its own so the entries line up.
fn line_label(ui: &mut Ui, line: Option<u32>) {
    let p = theme::palette(ui);
    let text = line
        .map(|l| t!("console-line", line = l))
        .unwrap_or_default();
    ui.add_sized(
        [52.0, ui.spacing().interact_size.y],
        Label::new(RichText::new(text).small().color(p.muted)),
    );
}

fn entry_row(ui: &mut Ui, entry: &ConsoleEntry) {
    let p = theme::palette(ui);
    match entry {
        ConsoleEntry::Log { level, args, line } => {
            let color = match level {
                LogLevel::Log => p.text,
                LogLevel::Info => p.blue,
                LogLevel::Warn => p.redirect,
                LogLevel::Error => p.fail,
                LogLevel::Debug => p.muted,
            };
            ui.horizontal_wrapped(|ui| {
                line_label(ui, *line);
                if let Some(text) = entry.log_text() {
                    let (shown, _) = format::truncate(&text, MAX_TEXT);
                    ui.add(Label::new(RichText::new(shown).monospace().color(color)).wrap());
                }
            });
            // Objects and arrays can be opened below the line.
            for (i, arg) in args.iter().enumerate().filter(|(_, a)| is_container(a)) {
                ui.indent(i, |ui| value_tree(ui, "", arg));
            }
        }
        ConsoleEntry::Test {
            name,
            passed,
            message,
            line,
        } => {
            ui.horizontal_wrapped(|ui| {
                line_label(ui, *line);
                if *passed {
                    ui.label(RichText::new("✓").color(p.ok));
                    ui.label(name);
                } else {
                    ui.label(RichText::new("✗").color(p.fail));
                    ui.label(RichText::new(name).color(p.fail));
                    if let Some(m) = message {
                        ui.label(RichText::new(m).color(p.muted));
                    }
                }
            });
        }
        ConsoleEntry::Break { line, values } => {
            let title = t!("console-breakpoint", line = *line, count = values.len());
            CollapsingHeader::new(RichText::new(title).color(p.fail))
                .id_salt("break")
                .show(ui, |ui| {
                    if values.is_empty() {
                        theme::hint(ui, t!("console-no-variables"));
                    }
                    for (i, (name, value)) in values.iter().enumerate() {
                        ui.push_id(i, |ui| value_tree(ui, name, value));
                    }
                });
        }
        ConsoleEntry::Error { message, line } => {
            ui.horizontal_wrapped(|ui| {
                line_label(ui, *line);
                ui.add(Label::new(RichText::new(message).color(p.fail)).wrap());
            });
        }
        ConsoleEntry::Note { message } => theme::hint(ui, message),
    }
}

fn is_container(v: &Value) -> bool {
    match v {
        Value::Array(_) => true,
        Value::Object(_) => special(v).is_none(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

/// What a value JSON can't hold stands for (`{"$type": "undefined"}`), as JavaScript writes it.
fn special(v: &Value) -> Option<String> {
    let obj = v.as_object()?;
    let kind = obj.get("$type")?.as_str()?;
    if obj.len() > 2 {
        return None;
    }
    let value = obj.get("value").and_then(Value::as_str);
    Some(match (kind, value) {
        ("function", _) => {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or_default();
            format!("function {name}()")
        }
        ("circular", _) => "[circular]".to_owned(),
        ("more", _) => "…".to_owned(),
        (_, Some(value)) => value.to_owned(),
        (other, None) => other.to_owned(),
    })
}

/// A value on one line: strings quoted, numbers, `true`, `undefined`, `{…}`.
fn short(v: &Value) -> String {
    if let Some(s) = special(v) {
        return s;
    }
    match v {
        Value::Array(items) => t!("console-array", count = items.len()),
        Value::Object(map) => t!("console-object", count = map.len()),
        other => format::truncate(&other.to_string(), MAX_TEXT).0.to_owned(),
    }
}

/// `name: value`, or just the value without a name; objects and arrays fold open into
/// their members.
fn value_tree(ui: &mut Ui, name: &str, value: &Value) {
    let p = theme::palette(ui);
    let prefix = if name.is_empty() {
        String::new()
    } else {
        format!("{name}: ")
    };
    if !is_container(value) {
        Grid::new("value").num_columns(2).show(ui, |ui| {
            ui.label(RichText::new(prefix.trim_end()).monospace().color(p.muted));
            let color = match value {
                Value::String(_) => p.ok,
                Value::Number(_) => p.blue,
                Value::Bool(_) | Value::Null => p.purple,
                Value::Object(_) | Value::Array(_) => p.muted,
            };
            ui.add(Label::new(RichText::new(short(value)).monospace().color(color)).wrap());
        });
        return;
    }
    CollapsingHeader::new(RichText::new(format!("{prefix}{}", short(value))).monospace())
        .id_salt(name)
        .show(ui, |ui| match value {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    ui.push_id(i, |ui| value_tree(ui, &i.to_string(), item));
                }
            }
            Value::Object(map) => {
                for (i, (key, item)) in map.iter().enumerate() {
                    ui.push_id(i, |ui| value_tree(ui, key, item));
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use serde_json::json;

    use super::*;

    #[test]
    fn values_json_cannot_hold_read_like_javascript() {
        assert_eq!(short(&json!({"$type": "undefined"})), "undefined");
        assert_eq!(
            short(&json!({"$type": "function", "name": "f"})),
            "function f()"
        );
        assert_eq!(short(&json!({"$type": "NaN"})), "NaN");
        assert_eq!(short(&json!({"$type": "date", "value": "Tue"})), "Tue");
        assert_eq!(short(&json!("ü😀")), "\"ü😀\"");
        assert_eq!(short(&json!([1, 2])), "[…] 2 items");
        assert_eq!(short(&json!({"a": 1})), "{…} 1 key");
        // An object of the user's that happens to have a `$type` key stays an object.
        assert!(is_container(&json!({"$type": "x", "a": 1, "b": 2})));
    }

    /// R-TEST-9, R-TEST-10: logs, tests, breakpoint hits and errors are shown with lines.
    #[test]
    fn shows_every_kind_of_entry() {
        let entries = vec![
            (
                ScriptKind::PreRequest,
                ConsoleEntry::Log {
                    level: LogLevel::Warn,
                    args: vec![json!("token"), json!({"id": 7})],
                    line: Some(2),
                },
            ),
            (
                ScriptKind::Tests,
                ConsoleEntry::Test {
                    name: "status".into(),
                    passed: false,
                    message: Some("Expected 500 to be 200".into()),
                    line: Some(4),
                },
            ),
            (
                ScriptKind::Tests,
                ConsoleEntry::Break {
                    line: 6,
                    values: vec![("body".into(), json!({"id": 7}))],
                },
            ),
            (
                ScriptKind::Tests,
                ConsoleEntry::Error {
                    message: "TypeError: x".into(),
                    line: Some(9),
                },
            ),
        ];
        assert!(console_went_wrong(&entries));
        let mut h = crate::test_host::harness(|ui| console_view(ui, &entries));
        h.run();
        h.get_by_label("PRE-REQUEST SCRIPT");
        h.get_by_label("TESTS");
        h.get_by_label(r#"token {"id":7}"#);
        h.get_by_label("line 4");
        h.get_by_label("⏺ Breakpoint, line 6 — 1 variable").click();
        h.run();
        h.get_by_label("body: {…} 1 key");
        h.get_by_label("TypeError: x");
    }
}
