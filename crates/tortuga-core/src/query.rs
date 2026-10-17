//! Keeps the URL bar and the query-param table in sync (R-HTTP-2), and finds and
//! fills `:name` path variables (R-HTTP-10).
//!
//! The table holds decoded values; the URL holds percent-encoded ones.
//! Only characters that would break the query structure are encoded, so
//! `{{variables}}` and most readable text stay as typed.

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

use crate::model::KeyValue;

/// Characters encoded in query keys/values when building the URL.
const QUERY_COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b'=')
    .add(b'<')
    .add(b'>');

/// Characters encoded in a path variable value, so it stays one path segment.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'?')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// Byte index where the path of `url` starts: after `scheme://host:port`, or 0 for a
/// path like `/users`. `{{base}}/users` starts at the first `/` after the variable.
fn path_start(url: &str) -> usize {
    if url.starts_with('/') {
        return 0;
    }
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    url[after_scheme..]
        .find('/')
        .map_or(url.len(), |i| after_scheme + i)
}

/// Splits a path segment like `:id.json` into the variable name (`id`) and the rest.
fn segment_variable(segment: &str) -> Option<(&str, &str)> {
    let rest = segment.strip_prefix(':')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| rest.split_at(end))
}

/// Names of the `:name` path variables in `url` (before any `?`), in order, without duplicates.
pub fn path_variable_names(url: &str) -> Vec<String> {
    let url = url.split('?').next().unwrap_or(url);
    let mut names: Vec<String> = Vec::new();
    for segment in url[path_start(url)..].split('/') {
        if let Some((name, _)) = segment_variable(segment)
            && !names.iter().any(|n| n == name)
        {
            names.push(name.to_owned());
        }
    }
    names
}

/// Replaces each `:name` path segment with its URL-encoded value. Fails with the
/// names that have no (or an empty) value.
pub fn fill_path_variables(url: &str, values: &[KeyValue]) -> Result<String, Vec<String>> {
    let start = path_start(url);
    let (head, path) = url.split_at(start);
    let mut missing = Vec::new();
    let segments: Vec<String> = path
        .split('/')
        .map(|segment| match segment_variable(segment) {
            None => segment.to_owned(),
            Some((name, rest)) => match values.iter().find(|v| v.name == name) {
                Some(v) if !v.value.is_empty() => {
                    format!("{}{rest}", utf8_percent_encode(&v.value, PATH_SEGMENT))
                }
                _ => {
                    if !missing.iter().any(|m| m == name) {
                        missing.push(name.to_owned());
                    }
                    segment.to_owned()
                }
            },
        })
        .collect();
    if missing.is_empty() {
        Ok(format!("{head}{}", segments.join("/")))
    } else {
        Err(missing)
    }
}

/// Splits `text` at the first `?` into the base URL and decoded params.
pub fn split_url(text: &str) -> (String, Vec<KeyValue>) {
    let Some((base, query)) = text.split_once('?') else {
        return (text.to_owned(), Vec::new());
    };
    let params = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            KeyValue::new(decode(name), decode(value))
        })
        .collect();
    (base.to_owned(), params)
}

/// Builds the URL from the base and the enabled params.
pub fn join_url(base: &str, params: &[KeyValue]) -> String {
    let query: Vec<String> = params
        .iter()
        .filter(|p| p.is_active())
        .map(|p| {
            if p.value.is_empty() {
                encode(&p.name)
            } else {
                format!("{}={}", encode(&p.name), encode(&p.value))
            }
        })
        .collect();
    if query.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", query.join("&"))
    }
}

/// Encodes form fields as `application/x-www-form-urlencoded`.
pub fn encode_form(fields: &[KeyValue]) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for f in fields.iter().filter(|f| f.is_active()) {
        ser.append_pair(&f.name, &f.value);
    }
    ser.finish()
}

fn encode(s: &str) -> String {
    utf8_percent_encode(s, QUERY_COMPONENT).to_string()
}

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_variables_are_found_only_in_the_path() {
        assert_eq!(
            path_variable_names("http://localhost:8080/users/:id/orders/:orderId?x=:no"),
            ["id", "orderId"]
        );
        assert_eq!(path_variable_names("/users/:id.json"), ["id"]);
        assert_eq!(path_variable_names("{{base}}/users/:id/:id"), ["id"]);
        assert_eq!(
            path_variable_names("https://a.test/a:b/:/x"),
            Vec::<String>::new()
        );
        assert_eq!(path_variable_names("localhost:8080"), Vec::<String>::new());
    }

    #[test]
    fn path_variables_are_filled_and_encoded() {
        let values = [KeyValue::new("id", "a b/c"), KeyValue::new("fmt", "json")];
        assert_eq!(
            fill_path_variables("http://h:1/users/:id/x.:fmt", &values),
            Ok("http://h:1/users/a%20b%2Fc/x.:fmt".to_owned()),
            "only whole-segment variables are filled"
        );
        assert_eq!(
            fill_path_variables("/users/:id.json", &values),
            Ok("/users/a%20b%2Fc.json".to_owned())
        );
        assert_eq!(
            fill_path_variables("/a/:x/:y", &[KeyValue::new("x", "")]),
            Err(vec!["x".to_owned(), "y".to_owned()])
        );
    }

    #[test]
    fn url_without_query_has_no_params() {
        assert_eq!(
            split_url("https://a.test/x"),
            ("https://a.test/x".into(), vec![])
        );
    }

    #[test]
    fn flags_and_empty_values_round_trip() {
        let (base, params) = split_url("http://a.test/?flag&empty=&x=1");
        assert_eq!(params[0], KeyValue::new("flag", ""));
        assert_eq!(params[1], KeyValue::new("empty", ""));
        // A param with an empty value is written without `=`.
        assert_eq!(join_url(&base, &params), "http://a.test/?flag&empty&x=1");
    }

    #[test]
    fn special_characters_are_encoded_in_url_and_decoded_in_table() {
        let params = vec![
            KeyValue::new("q", "a&b=c d+e#f"),
            KeyValue::new("v", "{{token}}"),
        ];
        let url = join_url("http://a.test", &params);
        assert_eq!(url, "http://a.test?q=a%26b%3Dc%20d%2Be%23f&v={{token}}");
        assert_eq!(split_url(&url).1, params);
    }

    #[test]
    fn blank_and_disabled_params_are_left_out() {
        let mut off = KeyValue::new("off", "1");
        off.enabled = false;
        let params = vec![KeyValue::new("  ", "x"), off, KeyValue::new("on", "1")];
        assert_eq!(join_url("http://a.test", &params), "http://a.test?on=1");
    }

    #[test]
    fn form_encoding_uses_plus_for_spaces() {
        let fields = vec![
            KeyValue::new("name", "Ada Lovelace"),
            KeyValue::new("x", "a&b"),
        ];
        assert_eq!(encode_form(&fields), "name=Ada+Lovelace&x=a%26b");
    }
}
