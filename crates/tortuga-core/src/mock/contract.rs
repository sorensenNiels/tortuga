//! The contract check (R-MOCK-8, D47): does a real response look like the example it
//! should match? Status, media type and, for JSON, the shape: keys and value types,
//! never values, so a difference never shows response data (P6).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{Example, MockRequest, guess_content_type};
use crate::json::path_with_key;
use crate::model::{Request, RequestBody, Response, is_json_content_type};
use crate::script::TestResult;
use crate::vars;

/// How many differences a failed check lists before "and N more".
const SHOWN: usize = 20;

/// The check of `response` to `sent` against `examples`, as a test result; `None` for a
/// request without examples. `variables` fill the example's `{{variables}}`.
pub fn check(
    examples: &[Example],
    sent: &Request,
    response: &Response,
    variables: &BTreeMap<String, String>,
) -> Option<TestResult> {
    if examples.is_empty() {
        return None;
    }
    let Some(example) = choose(examples, sent, response.status) else {
        let mut statuses = Vec::new();
        for e in examples {
            if !statuses.contains(&e.status) {
                statuses.push(e.status);
            }
        }
        let statuses: Vec<String> = statuses.iter().map(u16::to_string).collect();
        return Some(TestResult {
            name: "Matches an example".into(),
            passed: false,
            message: Some(format!(
                "Status {} is not one of the examples' ({})",
                response.status,
                statuses.join(", ")
            )),
        });
    };
    let body = vars::substitute(&example.body, variables, &mut BTreeSet::new());
    let mut found = Differences::default();
    let expected_type = example
        .headers
        .iter()
        .find(|h| h.is_active() && h.is_named("content-type"))
        .map(|h| vars::substitute(&h.value, variables, &mut BTreeSet::new()))
        .or_else(|| expected_media_type(&body).map(str::to_owned));
    if let Some(expected) = &expected_type {
        compare_media_types(expected, response.content_type(), &mut found);
        if is_json_content_type(expected)
            && let Ok(shape) = serde_json::from_str::<Value>(&body)
        {
            match serde_json::from_slice::<Value>(&response.body) {
                Ok(actual) => compare("$", &shape, &actual, &mut found),
                Err(_) => found.add("The body is not JSON".into()),
            }
        }
    }
    Some(found.result(&example.name))
}

/// The media type an example body without a Content-Type stands for: as the mock
/// guesses it (R-MOCK-2), but JSON for one that is only not JSON because of a
/// `{{variable}}` without a value.
fn expected_media_type(body: &str) -> Option<&'static str> {
    if body.is_empty() {
        None
    } else if matches!(body.trim_start().as_bytes().first(), Some(b'{' | b'[')) {
        Some("application/json")
    } else {
        Some(guess_content_type(body))
    }
}

/// Of the examples with `status`, the first whose conditions hold for `sent`, else the
/// first.
fn choose<'a>(examples: &'a [Example], sent: &Request, status: u16) -> Option<&'a Example> {
    let mut same_status = examples.iter().filter(|e| e.status == status).peekable();
    let first = *same_status.peek()?;
    let request = as_received(sent);
    Some(
        same_status
            .find(|e| e.conditions.hold_for(&request))
            .unwrap_or(first),
    )
}

/// `sent` as the mock would have received it, for its examples' conditions.
fn as_received(sent: &Request) -> MockRequest {
    let body = match &sent.body {
        RequestBody::Text(text) | RequestBody::Json(text) | RequestBody::Xml(text) => {
            text.clone().into_bytes()
        }
        RequestBody::None | RequestBody::Form(_) | RequestBody::Multipart(_) => Vec::new(),
    };
    let mut headers: Vec<_> = sent
        .headers
        .iter()
        .filter(|h| h.is_active())
        .cloned()
        .collect();
    if !sent.has_header("content-type")
        && let Some(content_type) = sent.body.default_content_type()
    {
        headers.push(crate::model::KeyValue::new("Content-Type", content_type));
    }
    MockRequest {
        method: sent.method.as_str().to_owned(),
        path: String::new(),
        query: sent
            .params
            .iter()
            .filter(|p| p.is_active())
            .cloned()
            .collect(),
        headers,
        body,
    }
}

fn compare_media_types(expected: &str, actual: Option<&str>, found: &mut Differences) {
    let expected = essence(expected);
    match actual.map(essence) {
        None => found.add(format!(
            "There is no Content-Type, the example has {expected}"
        )),
        Some(actual) if actual != expected => {
            found.add(format!(
                "The Content-Type is {actual}, the example has {expected}"
            ));
        }
        Some(_) => {}
    }
}

/// `application/json` of `Application/JSON; charset=utf-8`.
fn essence(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Compares `actual` at `path` with the example's `shape` (R-MOCK-8).
fn compare(path: &str, shape: &Value, actual: &Value, found: &mut Differences) {
    match (shape, actual) {
        (Value::Null, _) => {}
        (Value::Object(keys), Value::Object(values)) => {
            for (key, shape) in keys {
                let at = path_with_key(path, key);
                match values.get(key) {
                    Some(value) => compare(&at, shape, value, found),
                    None => found.add(format!("{at} is missing")),
                }
            }
            for key in values.keys().filter(|k| !keys.contains_key(*k)) {
                found.add(format!(
                    "{} is not in the example",
                    path_with_key(path, key)
                ));
            }
        }
        (Value::Array(items), Value::Array(values)) => {
            if let Some(shape) = items.first() {
                for (i, value) in values.iter().enumerate() {
                    compare(&format!("{path}[{i}]"), shape, value, found);
                }
            }
        }
        _ if kind(shape) == kind(actual) => {}
        _ => found.add(format!(
            "{} is {}, the example has {}",
            if path == "$" { "The body" } else { path },
            kind(actual),
            kind(shape)
        )),
    }
}

/// A JSON value's type, for messages.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// The differences found: the first [`SHOWN`], and how many more.
#[derive(Debug, Default)]
struct Differences {
    shown: Vec<String>,
    more: usize,
}

impl Differences {
    fn add(&mut self, difference: String) {
        if self.shown.len() < SHOWN {
            self.shown.push(difference);
        } else {
            self.more += 1;
        }
    }

    fn result(self, example: &str) -> TestResult {
        let mut message = self.shown.join("; ");
        if self.more > 0 {
            message = format!("{message}; and {} more", self.more);
        }
        TestResult {
            name: format!("Matches example “{example}”"),
            passed: self.shown.is_empty(),
            message: (!self.shown.is_empty()).then_some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HttpMethod, KeyValue};
    use std::time::Duration;

    fn example(name: &str, status: u16, body: &str) -> Example {
        Example {
            status,
            body: body.into(),
            ..Example::new(name)
        }
    }

    fn response(status: u16, content_type: &str, body: &str) -> Response {
        Response {
            status,
            status_text: String::new(),
            headers: vec![KeyValue::new("Content-Type", content_type)],
            body: body.as_bytes().to_vec(),
            elapsed: Duration::ZERO,
        }
    }

    fn sent() -> Request {
        Request::new(HttpMethod::Get, "http://api.test/orders/7")
    }

    fn run(examples: &[Example], response: &Response) -> TestResult {
        check(examples, &sent(), response, &BTreeMap::new()).expect("the request has examples")
    }

    fn differences(examples: &[Example], response: &Response) -> Vec<String> {
        run(examples, response)
            .message
            .map(|m| m.split("; ").map(str::to_owned).collect())
            .unwrap_or_default()
    }

    #[test]
    fn requests_without_examples_are_not_checked() {
        let r = response(200, "application/json", "{}");
        assert_eq!(check(&[], &sent(), &r, &BTreeMap::new()), None);
    }

    #[test]
    fn a_response_shaped_like_its_example_passes_whatever_its_values() {
        // R-MOCK-8: values and key order are not compared.
        let e = example(
            "200 OK",
            200,
            r#"{"id": 1, "name": "x", "tags": ["a"], "note": null}"#,
        );
        let r = response(
            200,
            "application/json; charset=utf-8",
            r#"{"tags": ["b", "c"], "name": "ü😀", "id": 99, "note": {"any": true}}"#,
        );
        let result = run(&[e], &r);
        assert_eq!(result.name, "Matches example “200 OK”");
        assert!(result.passed, "{result:?}");
        assert_eq!(result.message, None);
    }

    #[test]
    fn missing_extra_and_mistyped_keys_are_named_by_their_path() {
        let e = example(
            "200 OK",
            200,
            r#"{"id": 1, "total": 2, "items": [{"sku": "a"}], "a b": true}"#,
        );
        let r = response(
            200,
            "application/json",
            r#"{"id": "1", "items": [{"sku": "a"}, {"sku": 3}], "a b": "yes", "debug": 1}"#,
        );
        assert_eq!(
            differences(&[e], &r),
            [
                "$.id is a string, the example has a number",
                "$.total is missing",
                "$.items[1].sku is a number, the example has a string",
                "$['a b'] is a string, the example has a boolean",
                "$.debug is not in the example",
            ]
        );
    }

    #[test]
    fn a_status_no_example_has_fails_and_lists_the_statuses() {
        let examples = [
            example("200 OK", 200, "{}"),
            example("404", 404, "{}"),
            example("Other 200", 200, "{}"),
        ];
        let result = run(&examples, &response(500, "application/json", "{}"));
        assert!(!result.passed);
        assert_eq!(result.name, "Matches an example");
        assert_eq!(
            result.message.as_deref(),
            Some("Status 500 is not one of the examples' (200, 404)")
        );
    }

    #[test]
    fn the_example_for_the_responses_status_is_compared() {
        let examples = [
            example("200 OK", 200, r#"{"id": 1}"#),
            example("Not found", 404, r#"{"error": "x"}"#),
        ];
        let result = run(
            &examples,
            &response(404, "application/json", r#"{"error": "gone"}"#),
        );
        assert_eq!(result.name, "Matches example “Not found”");
        assert!(result.passed);
    }

    #[test]
    fn conditions_choose_between_examples_with_the_same_status() {
        let mut admin = example("Admin", 200, r#"{"role": "x", "rights": []}"#);
        admin.conditions.query.insert("as".into(), "admin".into());
        let user = example("User", 200, r#"{"role": "x"}"#);
        let mut request = sent();
        request.params.push(KeyValue::new("as", "admin"));
        let r = response(
            200,
            "application/json",
            r#"{"role": "a", "rights": ["all"]}"#,
        );
        let result = check(&[admin, user], &request, &r, &BTreeMap::new()).expect("examples");
        assert_eq!(result.name, "Matches example “Admin”");
        assert!(result.passed, "{result:?}");
        // Without the parameter, the first example with the status is compared.
        let result = run(&[example("User", 200, r#"{"role": "x"}"#)], &r);
        assert!(!result.passed);
    }

    #[test]
    fn the_content_type_must_be_the_examples() {
        let mut e = example("200 OK", 200, "<order/>");
        e.headers
            .push(KeyValue::new("Content-Type", "Application/XML"));
        assert!(
            run(
                std::slice::from_ref(&e),
                &response(200, "application/xml; charset=utf-8", "<o/>")
            )
            .passed
        );
        assert_eq!(
            differences(&[e], &response(200, "text/html", "<o/>")),
            ["The Content-Type is text/html, the example has application/xml"]
        );
        // Guessed from an example body without a Content-Type.
        let e = example("200 OK", 200, r#"{"id": 1}"#);
        let mut r = response(200, "", r#"{"id": 1}"#);
        r.headers.clear();
        assert_eq!(
            differences(&[e], &r),
            ["There is no Content-Type, the example has application/json"]
        );
        // An example without a body or Content-Type checks only the status.
        assert!(run(&[example("204", 204, "")], &response(204, "text/plain", "")).passed);
    }

    #[test]
    fn a_body_that_is_not_json_or_of_another_type_is_reported() {
        let e = example("200 OK", 200, r#"{"id": 1}"#);
        assert_eq!(
            differences(
                std::slice::from_ref(&e),
                &response(200, "application/json", "oops")
            ),
            ["The body is not JSON"]
        );
        assert_eq!(
            differences(&[e], &response(200, "application/json", "[1]")),
            ["The body is an array, the example has an object"]
        );
    }

    #[test]
    fn example_variables_are_filled_before_the_body_is_read() {
        let e = example("200 OK", 200, r#"{"id": {{id}}, "name": "{{name}}"}"#);
        let r = response(200, "application/json", r#"{"id": 5, "name": "x"}"#);
        let vars = BTreeMap::from([("id".to_owned(), "7".to_owned())]);
        let result = check(&[e], &sent(), &r, &vars).expect("examples");
        assert!(result.passed, "{result:?}");
        // Still not JSON: only status and Content-Type are checked.
        let e = example("200 OK", 200, r#"{"id": {{unknown}}}"#);
        assert!(run(&[e], &response(200, "application/json", "[]")).passed);
    }

    #[test]
    fn many_differences_are_cut_after_twenty() {
        let e = example("200 OK", 200, r#"[{"id": 1}]"#);
        let body = format!("[{}]", vec![r#"{"id": "x"}"#; 25].join(","));
        let result = run(&[e], &response(200, "application/json", &body));
        let message = result.message.expect("differences");
        assert_eq!(message.matches("is a string").count(), 20);
        assert!(message.ends_with("; and 5 more"), "{message}");
    }
}
