//! What changed between two responses (R-HIST-5), and how their shapes differ between
//! environments (R-SRV-10): the status, the headers, and the
//! body: JSON by structure and value, with a `JSONPath` for each change (as the filter
//! writes them, R-HTTP-13), other text line by line.

use crate::json::path_with_key;
use crate::model::{Response, is_json_content_type};

/// Changes listed at most; the rest are counted.
pub const MAX_CHANGES: usize = 500;

/// Headers that differ on nearly every send, so they are left out.
const VOLATILE_HEADERS: [&str; 2] = ["date", "age"];

/// How the second response differs from the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// (before, after), when the status changed.
    pub status: Option<(u16, u16)>,
    pub elapsed_ms: (u64, u64),
    pub size: (usize, usize),
    pub headers: Vec<Change>,
    pub body: BodyChanges,
}

/// What changed in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyChanges {
    Same,
    /// Both are JSON: each value that was added, removed or changed; `more` counts the
    /// changes past [`MAX_CHANGES`].
    Json {
        changes: Vec<Change>,
        more: usize,
    },
    /// Text that isn't JSON on both sides: how many lines differ, and the first (from 1).
    Lines {
        differ: usize,
        first: usize,
    },
}

/// One difference, at a header name or a `JSONPath`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub at: String,
    /// The value before, as JSON for a body value; `None` when it was added.
    pub before: Option<String>,
    /// The value after; `None` when it was removed.
    pub after: Option<String>,
}

impl Comparison {
    /// How many changes there are, for "3 changes" (R-HIST-5).
    pub fn count(&self) -> usize {
        let body = match &self.body {
            BodyChanges::Same => 0,
            BodyChanges::Json { changes, more } => changes.len() + more,
            BodyChanges::Lines { differ, .. } => *differ,
        };
        usize::from(self.status.is_some()) + self.headers.len() + body
    }
}

/// How `after` differs from `before`.
pub fn compare(before: &Response, after: &Response) -> Comparison {
    Comparison {
        status: (before.status != after.status).then_some((before.status, after.status)),
        elapsed_ms: (millis(before), millis(after)),
        size: (before.body.len(), after.body.len()),
        headers: header_changes(before, after),
        body: body_changes(before, after),
    }
}

/// How the shape of `after`'s JSON body differs from `before`'s (R-SRV-10): keys one
/// has and the other hasn't, and values of another type ("number" → "string"), with
/// array items compared by their first one. Values themselves are not compared: ids
/// and dates differ between environments anyway. Empty when either isn't JSON.
pub fn shape_changes(before: &Response, after: &Response) -> Vec<Change> {
    let (Some(a), Some(b)) = (json_body(before), json_body(after)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    shape("$", &a, &b, &mut out);
    out.truncate(MAX_CHANGES);
    out
}

fn shape(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<Change>) {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (key, va) in x {
                let at = path_with_key(path, key);
                match y.get(key) {
                    Some(vb) => shape(&at, va, vb, out),
                    None => out.push(Change {
                        at,
                        before: Some(type_name(va).to_owned()),
                        after: None,
                    }),
                }
            }
            for (key, vb) in y.iter().filter(|(k, _)| !x.contains_key(*k)) {
                out.push(Change {
                    at: path_with_key(path, key),
                    before: None,
                    after: Some(type_name(vb).to_owned()),
                });
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if let (Some(va), Some(vb)) = (x.first(), y.first()) {
                shape(&format!("{path}[*]"), va, vb, out);
            }
        }
        _ if type_name(a) == type_name(b) => {}
        _ => out.push(Change {
            at: path.to_owned(),
            before: Some(type_name(a).to_owned()),
            after: Some(type_name(b).to_owned()),
        }),
    }
}

/// The JSON type of `value`, as the shape changes name it.
fn type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// The body as JSON, when the response says it is and it parses.
fn json_body(r: &Response) -> Option<serde_json::Value> {
    r.content_type()
        .is_some_and(is_json_content_type)
        .then(|| serde_json::from_slice(&r.body).ok())
        .flatten()
}

fn millis(r: &Response) -> u64 {
    u64::try_from(r.elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// Headers added, removed or changed, by name in either case; [`VOLATILE_HEADERS`] and
/// repeated headers (compared as one value, joined) included as such.
fn header_changes(before: &Response, after: &Response) -> Vec<Change> {
    let values = |r: &Response| {
        let mut map: std::collections::BTreeMap<String, Vec<&str>> =
            std::collections::BTreeMap::new();
        for h in &r.headers {
            let name = h.name.to_ascii_lowercase();
            if !VOLATILE_HEADERS.contains(&name.as_str()) {
                map.entry(name).or_default().push(h.value.as_str());
            }
        }
        map.into_iter()
            .map(|(k, v)| (k, v.join(", ")))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let (old, new) = (values(before), values(after));
    let names: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    names
        .into_iter()
        .filter(|n| old.get(*n) != new.get(*n))
        .map(|n| Change {
            at: n.clone(),
            before: old.get(n).cloned(),
            after: new.get(n).cloned(),
        })
        .collect()
}

fn body_changes(before: &Response, after: &Response) -> BodyChanges {
    if before.body == after.body {
        return BodyChanges::Same;
    }
    if let (Some(a), Some(b)) = (json_body(before), json_body(after)) {
        let mut changes = Vec::new();
        let mut more = 0;
        json_changes("$", &a, &b, &mut changes, &mut more);
        return if changes.is_empty() {
            // Same values, written differently (spacing, key order).
            BodyChanges::Same
        } else {
            BodyChanges::Json { changes, more }
        };
    }
    let (a, b) = (
        String::from_utf8_lossy(&before.body),
        String::from_utf8_lossy(&after.body),
    );
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let longest = a.len().max(b.len());
    let differ = (0..longest).filter(|i| a.get(*i) != b.get(*i)).count();
    let first = (0..longest)
        .find(|i| a.get(*i) != b.get(*i))
        .map_or(1, |i| i + 1);
    BodyChanges::Lines { differ, first }
}

/// Adds how `b` differs from `a` at `path`: keys by name, array items by place.
fn json_changes(
    path: &str,
    a: &serde_json::Value,
    b: &serde_json::Value,
    out: &mut Vec<Change>,
    more: &mut usize,
) {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (key, va) in x {
                let at = path_with_key(path, key);
                match y.get(key) {
                    Some(vb) => json_changes(&at, va, vb, out, more),
                    None => push(out, more, at, Some(va), None),
                }
            }
            for (key, vb) in y.iter().filter(|(k, _)| !x.contains_key(*k)) {
                push(out, more, path_with_key(path, key), None, Some(vb));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().max(y.len()) {
                let at = format!("{path}[{i}]");
                match (x.get(i), y.get(i)) {
                    (Some(va), Some(vb)) => json_changes(&at, va, vb, out, more),
                    (va, vb) => push(out, more, at, va, vb),
                }
            }
        }
        _ if a == b => {}
        _ => push(out, more, path.to_owned(), Some(a), Some(b)),
    }
}

fn push(
    out: &mut Vec<Change>,
    more: &mut usize,
    at: String,
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
) {
    if out.len() < MAX_CHANGES {
        out.push(Change {
            at,
            before: before.map(ToString::to_string),
            after: after.map(ToString::to_string),
        });
    } else {
        *more += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::KeyValue;

    fn response(status: u16, headers: &[(&str, &str)], body: &str) -> Response {
        Response {
            status,
            status_text: String::new(),
            headers: headers.iter().map(|(k, v)| KeyValue::new(*k, *v)).collect(),
            body: body.as_bytes().to_vec(),
            elapsed: Duration::from_millis(5),
            timing: None,
        }
    }

    const JSON: (&str, &str) = ("Content-Type", "application/json");

    /// R-HIST-5: JSON values added, removed and changed by path, keys that aren't names
    /// quoted, arrays by place; the same values written differently are the same.
    #[test]
    fn json_bodies_are_compared_by_structure() {
        let a = response(
            200,
            &[JSON],
            r#"{"id": 7, "total": 9.5, "lines": [{"qty": 1}], "x-y": "ü", "gone": true}"#,
        );
        let b = response(
            201,
            &[JSON],
            r#"{"id": 7, "total": 12, "lines": [{"qty": 2}, {"qty": 1}], "x-y": "😀", "new": null}"#,
        );
        let c = compare(&a, &b);
        assert_eq!(c.status, Some((200, 201)));
        let BodyChanges::Json { changes, more } = &c.body else {
            panic!("{:?}", c.body)
        };
        let at: Vec<(&str, Option<&str>, Option<&str>)> = changes
            .iter()
            .map(|c| (c.at.as_str(), c.before.as_deref(), c.after.as_deref()))
            .collect();
        assert_eq!(
            at,
            [
                ("$.total", Some("9.5"), Some("12")),
                ("$.lines[0].qty", Some("1"), Some("2")),
                ("$.lines[1]", None, Some(r#"{"qty":1}"#)),
                ("$['x-y']", Some("\"ü\""), Some("\"😀\"")),
                ("$.gone", Some("true"), None),
                ("$.new", None, Some("null")),
            ]
        );
        assert_eq!((*more, c.count()), (0, 7));

        let respaced = response(
            200,
            &[JSON],
            "{\"gone\":true,\"id\":7,\"x-y\":\"ü\",\"lines\":[{\"qty\":1}],\"total\":9.5}",
        );
        assert_eq!(compare(&a, &respaced).body, BodyChanges::Same);
    }

    /// R-HIST-5: headers by name in either case, Date and Age left out.
    #[test]
    fn headers_are_compared_by_name() {
        let a = response(200, &[("ETag", "1"), ("Date", "Mon"), ("X-Old", "a")], "");
        let b = response(200, &[("etag", "2"), ("Date", "Tue"), ("X-New", "b")], "");
        let c = compare(&a, &b);
        let names: Vec<&str> = c.headers.iter().map(|h| h.at.as_str()).collect();
        assert_eq!(names, ["etag", "x-new", "x-old"]);
        assert_eq!(c.body, BodyChanges::Same);
        assert_eq!(c.count(), 3);
    }

    /// R-HIST-5: other text line by line; very many JSON changes are counted, not listed.
    #[test]
    fn text_bodies_and_long_lists() {
        let c = compare(
            &response(200, &[], "a\nb\nc"),
            &response(200, &[], "a\nB\nc\nd"),
        );
        assert_eq!(
            c.body,
            BodyChanges::Lines {
                differ: 2,
                first: 2
            }
        );

        let many = |v: u32| {
            let items: Vec<String> = (0..600).map(|i| (i + v).to_string()).collect();
            response(200, &[JSON], &format!("[{}]", items.join(",")))
        };
        let BodyChanges::Json { changes, more } = compare(&many(0), &many(1)).body else {
            panic!("json")
        };
        assert_eq!((changes.len(), more), (MAX_CHANGES, 100));
    }

    /// R-SRV-10: shapes differ by keys and types, not values; arrays by their first item.
    #[test]
    fn shapes_are_compared_without_values() {
        let a = response(
            200,
            &[JSON],
            r#"{"id": 1, "price": 9.5, "items": [{"sku": "A"}], "legacy": true}"#,
        );
        let b = response(
            200,
            &[JSON],
            r#"{"id": 2, "price": "9.50", "items": [{"sku": "B", "qty": 1}], "discount": 0.1}"#,
        );
        let at: Vec<(String, Option<String>, Option<String>)> = shape_changes(&a, &b)
            .into_iter()
            .map(|c| (c.at, c.before, c.after))
            .collect();
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(
            at,
            [
                ("$.price".to_owned(), s("number"), s("string")),
                ("$.items[*].qty".to_owned(), None, s("number")),
                ("$.legacy".to_owned(), s("boolean"), None),
                ("$.discount".to_owned(), None, s("number")),
            ]
        );
        assert_eq!(shape_changes(&a, &response(200, &[], "not json")), []);
    }
}
