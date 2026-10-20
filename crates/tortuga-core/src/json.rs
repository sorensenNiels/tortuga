//! JSON helpers: checking and pretty-printing bodies (R-HTTP-4), and the values on the
//! lines of a pretty response, with tests and captures for them (R-TEST-13).

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

/// One step from a JSON value into a member: an object key or an array index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// A value in a JSON response and the way to it (IDEA-51, R-TEST-13): what a test or a
/// capture for it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub path: Vec<Step>,
    /// The value; an object or array is kept empty, as only its kind matters, so big
    /// bodies are not copied once per line.
    pub value: serde_json::Value,
}

/// For each line of `text`, pretty-printed as [`pretty`] does, the value that starts on
/// it; closing brackets have none. `None` if `text` is not JSON laid out that way.
pub fn line_points(text: &str) -> Option<Vec<Option<Point>>> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let mut out = Vec::new();
    walk(&value, &mut Vec::new(), &mut out);
    (out.len() == text.lines().count()).then_some(out)
}

/// The lines `serde_json::to_string_pretty` writes for `value`: one for a scalar or an
/// empty container, else an opening line, the members, and a closing line.
fn walk(value: &serde_json::Value, path: &mut Vec<Step>, out: &mut Vec<Option<Point>>) {
    use serde_json::Value;
    let kept = match value {
        Value::Object(_) => Value::Object(serde_json::Map::new()),
        Value::Array(_) => Value::Array(Vec::new()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
    };
    out.push(Some(Point {
        path: path.clone(),
        value: kept,
    }));
    let children: Vec<(Step, &Value)> = match value {
        Value::Object(map) => map.iter().map(|(k, v)| (Step::Key(k.clone()), v)).collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (Step::Index(i), v))
            .collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => return,
    };
    if children.is_empty() {
        return;
    }
    for (step, child) in children {
        path.push(step);
        walk(child, path, out);
        path.pop();
    }
    out.push(None);
}

fn is_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `text` as a JavaScript/JSON string literal.
fn quoted(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

impl Point {
    /// The path after `root`: `.name` for keys that are names, `other(key)` for other
    /// keys, `[i]` for indexes.
    fn written(&self, root: &str, other: impl Fn(&str) -> String) -> String {
        let mut out = root.to_owned();
        for step in &self.path {
            match step {
                Step::Key(k) if is_identifier(k) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(k);
                }
                Step::Key(k) => out.push_str(&other(k)),
                Step::Index(i) => {
                    out.push('[');
                    out.push_str(&i.to_string());
                    out.push(']');
                }
            }
        }
        out
    }

    /// `data.items[0].id`, for names and messages.
    pub fn display(&self) -> String {
        let out = self.written("", |k| format!("[{}]", quoted(k)));
        if out.is_empty() {
            "the body".into()
        } else {
            out
        }
    }

    /// `res.json().data.items[0].id`, for scripts.
    pub fn script_access(&self) -> String {
        self.written("res.json()", |k| format!("[{}]", quoted(k)))
    }

    /// `$.data.items[0].id`, for captures (R-TEST-4).
    pub fn json_path(&self) -> String {
        self.written("$", |k| {
            let escaped = k.replace('\\', "\\\\").replace('\'', "\\'");
            format!("['{escaped}']")
        })
    }

    /// A variable name for a capture of this value: its key, or `value`.
    pub fn variable_name(&self) -> String {
        match self.path.iter().rev().find_map(|s| match s {
            Step::Key(k) => Some(k),
            Step::Index(_) => None,
        }) {
            Some(k) if is_identifier(k) => k.clone(),
            Some(k) => crate::import::variable_name(k),
            None => "value".into(),
        }
    }

    /// Tests to add for this value, as (menu label, code): that it equals what it is now,
    /// that it has this type, and that it is there.
    pub fn tests(&self) -> Vec<(String, String)> {
        use serde_json::Value;
        let at = self.script_access();
        let name = self.display();
        let test = |what: &str, check: &str| {
            let title = quoted(&format!("{name} {what}"));
            format!("test({title}, () => {{\n  expect({at}){check};\n}});")
        };
        let mut out = Vec::new();
        let literal = match &self.value {
            Value::Null | Value::Array(_) | Value::Object(_) => None,
            Value::Bool(_) | Value::Number(_) | Value::String(_) => Some(self.value.to_string()),
        };
        if let Some(literal) = literal {
            let shown = short(&literal);
            out.push((
                format!("Equals {shown}"),
                test(&format!("is {shown}"), &format!(".to.equal({literal})")),
            ));
        }
        let kind = match &self.value {
            Value::Null => ("null", ".to.be.null".to_owned()),
            Value::Bool(_) => ("a boolean", r#".to.be.a("boolean")"#.to_owned()),
            Value::Number(_) => ("a number", r#".to.be.a("number")"#.to_owned()),
            Value::String(_) => ("a string", r#".to.be.a("string")"#.to_owned()),
            Value::Array(_) => ("an array", r#".to.be.an("array")"#.to_owned()),
            Value::Object(_) => ("an object", r#".to.be.an("object")"#.to_owned()),
        };
        out.push((
            format!("Is {}", kind.0),
            test(&format!("is {}", kind.0), &kind.1),
        ));
        if !self.path.is_empty() {
            let steps: Vec<String> = self
                .path
                .iter()
                .map(|s| match s {
                    Step::Key(k) => quoted(k),
                    Step::Index(i) => quoted(&i.to_string()),
                })
                .collect();
            let title = quoted(&format!("{name} is there"));
            out.push((
                "Is there".to_owned(),
                format!(
                    "test({title}, () => {{\n  expect(res.json()).toHaveProperty([{}]);\n}});",
                    steps.join(", ")
                ),
            ));
        }
        out
    }
}

/// `text` cut to a menu-sized length.
fn short(text: &str) -> String {
    const MAX: usize = 40;
    if text.chars().count() <= MAX {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX).collect();
    format!("{cut}…")
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

    const BODY: &str = r#"{"data": {"accessToken": "ü😀", "items": [{"id": 7}, {"id": 8.5}], "empty": {}, "none": null, "on": true, "my key": "it's"}}"#;

    /// R-TEST-13: every line of pretty JSON knows the value that starts on it.
    #[test]
    fn lines_of_pretty_json_map_to_their_values() {
        let text = pretty(BODY).expect("json");
        let points = line_points(&text).expect("pretty");
        assert_eq!(points.len(), text.lines().count());
        let shown: Vec<(String, Option<String>)> = text
            .lines()
            .zip(&points)
            .map(|(line, p)| (line.trim().to_owned(), p.as_ref().map(Point::display)))
            .collect();
        let at = |line: &str| {
            shown
                .iter()
                .find(|(l, _)| l == line)
                .and_then(|(_, p)| p.clone())
        };
        assert_eq!(at("{").as_deref(), Some("the body"));
        assert_eq!(
            at(r#""accessToken": "ü😀","#).as_deref(),
            Some("data.accessToken")
        );
        assert_eq!(at(r#""id": 8.5"#).as_deref(), Some("data.items[1].id"));
        assert_eq!(at(r#""empty": {},"#).as_deref(), Some("data.empty"));
        assert_eq!(
            at(r#""my key": "it's""#).as_deref(),
            Some(r#"data["my key"]"#)
        );
        assert_eq!(at("}"), None, "closing lines have no value");
        assert_eq!(line_points("{\"a\":1}"), None, "not laid out like `pretty`");
        assert_eq!(line_points("not json"), None);
    }

    /// R-TEST-13: the tests written for a value pass on the response it came from, and
    /// the capture finds the same value.
    #[test]
    fn written_tests_pass_and_captures_find_the_value() {
        use crate::model::{HttpMethod, KeyValue, Request, Response};
        let response = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("content-type", "application/json")],
            body: BODY.as_bytes().to_vec(),
            elapsed: std::time::Duration::ZERO,
        };
        let text = pretty(BODY).expect("json");
        let points: Vec<Point> = line_points(&text)
            .expect("pretty")
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(points.len(), 12, "every value, the body itself included");
        let mut script = String::new();
        for p in &points {
            for (_, code) in p.tests() {
                script.push_str(&code);
                script.push('\n');
            }
            if !p.path.is_empty()
                && !p.value.is_object()
                && !p.value.is_array()
                && !p.value.is_null()
            {
                let captured = crate::runner::capture(&p.json_path(), &response).expect("captured");
                let expected = match &p.value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                assert_eq!(captured, expected, "{}", p.json_path());
            }
        }
        let req = Request::new(HttpMethod::Get, "https://a.test/");
        let run = crate::script::run(
            &script,
            crate::script::Phase::Test,
            &std::collections::BTreeMap::default(),
            &req,
            Some(&response),
        );
        assert_eq!(run.error, None, "{script}");
        assert!(run.tests.len() > 20);
        for t in &run.tests {
            assert!(t.passed, "{}: {:?}", t.name, t.message);
        }
    }

    #[test]
    fn names_and_paths_for_keys_that_are_not_identifiers() {
        let point = Point {
            path: vec![
                Step::Key("data".into()),
                Step::Key("x-id's".into()),
                Step::Index(0),
            ],
            value: serde_json::json!(1),
        };
        assert_eq!(point.script_access(), r#"res.json().data["x-id's"][0]"#);
        assert_eq!(point.json_path(), r"$.data['x-id\'s'][0]");
        assert_eq!(point.variable_name(), "x_id_s");
        let token = Point {
            path: vec![Step::Key("accessToken".into())],
            value: serde_json::json!("t"),
        };
        assert_eq!(token.variable_name(), "accessToken");
        assert_eq!(
            Point {
                path: vec![],
                value: serde_json::json!(1)
            }
            .variable_name(),
            "value"
        );
        let long = Point {
            path: vec![Step::Key("s".into())],
            value: serde_json::json!("x".repeat(100)),
        };
        assert!(long.tests()[0].0.ends_with('…'), "the menu label is short");
        assert!(
            long.tests()[0].1.contains(&"x".repeat(100)),
            "the test checks it all"
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
