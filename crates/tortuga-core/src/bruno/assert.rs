//! Bruno's declarative checks in Tortuga's terms (R-COL-31): `assert` lines as
//! `test(…)` calls with `expect`, and `vars:post-response` values read from the
//! response as captures (R-TEST-4).
//!
//! Bruno evaluates the left side with its own `res`, where `res.body` is the parsed
//! JSON; in Tortuga `res.body` is the text and `res.json()` parses it. A path into the
//! body becomes `res.json()…`, the body on its own stays the text.

use std::fmt::Write as _;

use super::lang::Pair;

/// The operators that take no value.
const UNARY: [&str; 11] = [
    "isEmpty",
    "isNotEmpty",
    "isNull",
    "isUndefined",
    "isDefined",
    "isTruthy",
    "isFalsy",
    "isJson",
    "isNumber",
    "isString",
    "isBoolean",
];

/// The tests for the enabled `assert` lines, one `test(…)` each, and the lines that
/// could not be translated, as Bruno wrote them.
pub(super) fn tests(asserts: &[Pair]) -> (String, Vec<String>) {
    let mut code = String::new();
    let mut left = Vec::new();
    for pair in asserts.iter().filter(|p| p.enabled) {
        let line = format!("{}: {}", pair.key, pair.value);
        match check(&pair.key, &pair.value) {
            Some(check) => {
                // Writing to a String can't fail.
                let _ = writeln!(code, "test({}, () => {check});", js_string(&line));
            }
            None => left.push(line),
        }
    }
    (code, left)
}

/// `expect(…)…` for one assertion.
fn check(lhs: &str, rhs: &str) -> Option<String> {
    let actual = subject(lhs)?;
    let rhs = rhs.trim();
    let (op, value) = match rhs.split_once(char::is_whitespace) {
        Some((op, value)) => (op, value.trim()),
        None if UNARY.contains(&rhs) || rhs == "isArray" => (rhs, ""),
        // Without an operator Bruno compares for equality.
        None => ("eq", rhs),
    };
    let expect = |matcher: String| Some(format!("expect({actual}).{matcher}"));
    match op {
        "eq" => expect(format!("toEqual({})", operand(value))),
        "neq" => expect(format!("not.toEqual({})", operand(value))),
        "gt" => expect(format!("toBeGreaterThan({})", operand(value))),
        "gte" => expect(format!("toBeGreaterThanOrEqual({})", operand(value))),
        "lt" => expect(format!("toBeLessThan({})", operand(value))),
        "lte" => expect(format!("toBeLessThanOrEqual({})", operand(value))),
        "contains" => expect(format!("toContain({})", operand(value))),
        "notContains" => expect(format!("not.toContain({})", operand(value))),
        "length" => expect(format!("toHaveLength({})", operand(value))),
        "matches" => expect(format!("toMatch(new RegExp({}))", pattern(value))),
        "notMatches" => expect(format!("not.toMatch(new RegExp({}))", pattern(value))),
        "startsWith" => Some(format!(
            "expect(String({actual}).startsWith({})).toBe(true)",
            operand(value)
        )),
        "endsWith" => Some(format!(
            "expect(String({actual}).endsWith({})).toBe(true)",
            operand(value)
        )),
        "in" | "notIn" => {
            let list: Vec<String> = value.split(',').map(operand).collect();
            let not = if op == "notIn" { "not." } else { "" };
            Some(format!(
                "expect([{}]).{not}toContain({actual})",
                list.join(", ")
            ))
        }
        "between" => {
            let (low, high) = value.split_once(',')?;
            expect(format!("to.be.within({}, {})", operand(low), operand(high)))
        }
        "isEmpty" => expect("to.be.empty".into()),
        "isNotEmpty" => expect("not.to.be.empty".into()),
        "isNull" => expect("toBeNull()".into()),
        "isUndefined" => expect("toBeUndefined()".into()),
        "isDefined" => expect("toBeDefined()".into()),
        "isTruthy" => expect("toBeTruthy()".into()),
        "isFalsy" => expect("toBeFalsy()".into()),
        "isNumber" | "isString" | "isBoolean" => {
            let kind = op.trim_start_matches("is").to_ascii_lowercase();
            Some(format!("expect(typeof {actual}).toBe(\"{kind}\")"))
        }
        "isArray" => Some(format!("expect(Array.isArray({actual})).toBe(true)")),
        "isJson" => Some(format!(
            "expect({actual} !== null && typeof {actual} === \"object\").toBe(true)"
        )),
        _ => None,
    }
}

/// The left side in Tortuga's `res`, if it reads the response in a way Tortuga has.
fn subject(lhs: &str) -> Option<String> {
    let lhs = lhs.trim();
    match lhs {
        "res.status" | "res.getStatus()" => return Some("res.status".into()),
        "res.statusText" | "res.getStatusText()" => return Some("res.statusText".into()),
        "res.responseTime" | "res.getResponseTime()" => return Some("res.time".into()),
        "res.body" | "res.getBody()" => return Some("res.body".into()),
        _ => {}
    }
    if let Some(name) = header_name(lhs) {
        return Some(format!("res.header({})", js_string(&name)));
    }
    let path = body_path(lhs)?;
    Some(format!("res.json(){path}"))
}

/// `.a[0]["b"]` of `res.body.a[0]["b"]`: a path into the body, made of names, quoted
/// names and indexes only.
fn body_path(expr: &str) -> Option<&str> {
    let rest = expr
        .strip_prefix("res.body")
        .or_else(|| expr.strip_prefix("res.getBody()"))?;
    let simple = !rest.is_empty()
        && rest.starts_with(['.', '[', '?'])
        && rest
            .chars()
            .all(|c| c.is_alphanumeric() || "_$.[]?'\"- ".contains(c));
    simple.then_some(rest)
}

/// The header that `res.headers["name"]`, `res.headers.name` or `res.getHeader("name")`
/// reads.
fn header_name(expr: &str) -> Option<String> {
    let quoted = |s: &str| {
        let s = s.trim();
        let inner = s
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))?;
        (!inner.is_empty()).then(|| inner.to_owned())
    };
    if let Some(rest) = expr.strip_prefix("res.headers[") {
        return quoted(rest.strip_suffix(']')?);
    }
    if let Some(rest) = expr.strip_prefix("res.getHeader(") {
        return quoted(rest.strip_suffix(')')?);
    }
    let name = expr.strip_prefix("res.headers.")?;
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then(|| name.to_owned())
}

/// What a `vars:post-response` value reads, as a capture (R-TEST-4): `status`,
/// `header:<name>` or a `JSONPath`.
pub(super) fn capture(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if matches!(expr, "res.status" | "res.getStatus()") {
        return Some("status".into());
    }
    if let Some(name) = header_name(expr) {
        return Some(format!("header:{name}"));
    }
    if matches!(expr, "res.body" | "res.getBody()") {
        return Some("$".into());
    }
    let path = body_path(expr)?.replace("?.", ".").replace("?[", "[");
    (!path.contains(['?', ' '])).then(|| format!("${path}"))
}

/// A right-hand value as JavaScript: numbers, `true`, `false`, `null` and quoted text
/// as written, `{{variables}}` filled in, anything else as text.
fn operand(value: &str) -> String {
    let value = value.trim();
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    let number = !value.is_empty()
        && value.parse::<f64>().is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    if quoted || number || matches!(value, "true" | "false" | "null" | "undefined") {
        value.to_owned()
    } else if value.contains("{{") {
        format!("vars.replaceIn({})", js_string(value))
    } else {
        js_string(value)
    }
}

/// A regular expression's text as a string for `new RegExp`, so backslashes stay.
fn pattern(value: &str) -> String {
    let value = value.trim();
    let inner = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value);
    js_string(inner)
}

/// `text` as a JavaScript string literal.
fn js_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyValue, Request, Response};
    use crate::script::{Phase, run as run_script};

    fn pair(key: &str, value: &str) -> Pair {
        Pair {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    /// R-COL-31: every operator becomes a test that passes on a response that meets it
    /// and fails on one that doesn't.
    #[test]
    fn assertions_run_as_tests() {
        let asserts = [
            pair("res.status", "eq 200"),
            pair("res.status", "neq 404"),
            pair("res.status", "in 200, 201"),
            pair("res.status", "notIn 500,502"),
            pair("res.status", "between 200, 299"),
            pair("res.status", "200"),
            pair("res.body.id", "gt 6"),
            pair("res.body.id", "gte 7"),
            pair("res.body.id", "lt 8"),
            pair("res.body.id", "lte 7"),
            pair("res.body.id", "isNumber"),
            pair("res.body.name", "eq \"Ann ü😀\""),
            pair("res.body.name", "isString"),
            pair("res.body.name", "startsWith Ann"),
            pair("res.body.name", "endsWith 😀"),
            pair("res.body.name", "matches ^A\\w+"),
            pair("res.body.name", "notMatches ^\\d"),
            pair("res.body.tags", "contains a"),
            pair("res.body.tags", "notContains z"),
            pair("res.body.tags", "length 2"),
            pair("res.body.tags", "isArray"),
            pair("res.body.tags", "isNotEmpty"),
            pair("res.body.none", "isEmpty"),
            pair("res.body.missing", "isUndefined"),
            pair("res.body.nothing", "isNull"),
            pair("res.body.ok", "isTruthy"),
            pair("res.body.ok", "isBoolean"),
            pair("res.body['ok']", "eq true"),
            pair("res.body?.nested.deep", "isFalsy"),
            pair("res.getBody()", "isDefined"),
            pair("res.body", "contains Ann"),
            pair("res.body.nested", "isJson"),
            pair("res.headers['content-type']", "contains json"),
            pair("res.getHeader(\"X-Id\")", "eq \"7\""),
            pair("res.responseTime", "lt 100000"),
            pair("res.body.id", "eq {{id}}"),
        ];
        let (code, left) = tests(&asserts);
        assert_eq!(left, Vec::<String>::new());
        assert_eq!(code.lines().count(), asserts.len());

        let request = Request::new(crate::model::HttpMethod::Get, "https://shop.test/");
        let response = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![
                KeyValue::new("Content-Type", "application/json"),
                KeyValue::new("X-Id", "7"),
            ],
            body: r#"{"id": 7, "name": "Ann ü😀", "tags": ["a", "b"], "none": [], "nothing": null, "ok": true, "nested": {"deep": 0}}"#
                .as_bytes()
                .to_vec(),
            elapsed: std::time::Duration::from_millis(5),
        };
        let vars = std::collections::BTreeMap::from([("id".to_owned(), "7".to_owned())]);
        let out = run_script(&code, Phase::Test, &vars, &request, Some(&response));
        assert_eq!(out.error, None);
        let failed: Vec<_> = out.tests.iter().filter(|t| !t.passed).collect();
        // `{{id}}` is filled in as text, which doesn't equal the number 7.
        assert_eq!(failed.len(), 1, "{failed:#?}");
        assert_eq!(failed[0].name, "res.body.id: eq {{id}}");

        let wrong = Response {
            status: 500,
            body: b"{}".to_vec(),
            ..response
        };
        let out = run_script(&code, Phase::Test, &vars, &request, Some(&wrong));
        assert!(out.tests.iter().filter(|t| t.passed).count() < 10);
    }

    #[test]
    fn what_cannot_be_translated_is_returned() {
        let asserts = [
            pair("res('items.id')", "eq 1"),
            pair("res.body.id", "frobnicates 1"),
            pair("res.status", "between 1"),
            Pair {
                enabled: false,
                ..pair("res.status", "eq 1")
            },
        ];
        let (code, left) = tests(&asserts);
        assert_eq!(code, "");
        assert_eq!(
            left,
            [
                "res('items.id'): eq 1",
                "res.body.id: frobnicates 1",
                "res.status: between 1"
            ]
        );
    }

    #[test]
    fn captures_from_response_reads() {
        assert_eq!(capture("res.status").as_deref(), Some("status"));
        assert_eq!(capture(" res.getStatus() ").as_deref(), Some("status"));
        assert_eq!(capture("res.body").as_deref(), Some("$"));
        assert_eq!(capture("res.body.token").as_deref(), Some("$.token"));
        assert_eq!(
            capture("res.body.items[0]['id']").as_deref(),
            Some("$.items[0]['id']")
        );
        assert_eq!(capture("res.body?.data?.id").as_deref(), Some("$.data.id"));
        assert_eq!(
            capture("res.headers['x-token']").as_deref(),
            Some("header:x-token")
        );
        assert_eq!(capture("res.headers.etag").as_deref(), Some("header:etag"));
        assert_eq!(
            capture("res.getHeader('ETag')").as_deref(),
            Some("header:ETag")
        );
        assert_eq!(capture("res.body.token.trim()"), None);
        assert_eq!(capture("res('token')"), None);
        assert_eq!(capture("bru.getVar('x')"), None);
        assert_eq!(capture(""), None);
    }
}
