//! A first draft of a request's description (R-COL-35): Markdown built from what the
//! request sends and the examples it answers with, with blanks for what only its author
//! knows. Credentials are never copied: auth is named by its kind, and the values of
//! `Authorization` and `Cookie` headers are left out.
//!
//! Writing to a `String` cannot fail, so the `fmt::Result`s of `write!` are dropped.

use std::fmt::Write as _;

use crate::model::{Auth, KeyValue, Request, RequestBody};

/// Placeholder the author replaces with what the request is for.
const PURPOSE: &str = "_What this request does, when to use it and what it changes._";

/// The draft description of `request`, in Markdown.
pub fn draft(request: &Request) -> String {
    let mut out = format!("# {}\n\n{PURPOSE}\n\n", request.name.trim());
    let path = if request.url.trim().is_empty() {
        "/"
    } else {
        request.url.trim()
    };
    out.push_str(&code_span(&format!("{} {path}", request.method)));
    out.push('\n');
    if let Some(server) = &request.server {
        let _ = write!(out, "\nServer: {}\n", code_span(server));
    }

    let path_vars: Vec<Row<'_>> = request.path_vars.iter().map(Row::from).collect();
    table_section(&mut out, "Path variables", &path_vars);
    let params: Vec<Row<'_>> = request
        .params
        .iter()
        .filter(|p| !p.name.trim().is_empty())
        .map(Row::from)
        .collect();
    table_section(&mut out, "Query parameters", &params);
    let headers: Vec<Row<'_>> = request
        .headers
        .iter()
        .filter(|h| h.is_active())
        .map(|h| {
            if h.is_named("Authorization") || h.is_named("Cookie") {
                Row {
                    example: "…".to_owned(),
                    ..Row::from(h)
                }
            } else {
                Row::from(h)
            }
        })
        .collect();
    table_section(&mut out, "Headers", &headers);
    auth_section(&mut out, request.auth.as_ref());
    body_section(&mut out, &request.body);
    responses_section(&mut out, request);
    out
}

/// A row of a Name | Example | Description table.
struct Row<'a> {
    name: &'a str,
    /// The example value as a table cell.
    example: String,
    optional: bool,
}

impl<'a> From<&'a KeyValue> for Row<'a> {
    fn from(kv: &'a KeyValue) -> Self {
        Row {
            name: kv.name.trim(),
            example: cell(&kv.value),
            // A switched-off parameter is usually one the request can do without.
            optional: !kv.enabled,
        }
    }
}

fn table_section(out: &mut String, heading: &str, rows: &[Row<'_>]) {
    if !rows.is_empty() {
        let _ = write!(out, "\n## {heading}\n\n{}", table(rows));
    }
}

fn table(rows: &[Row<'_>]) -> String {
    let mut out = String::from("| Name | Example | Description |\n|---|---|---|\n");
    for row in rows {
        let note = if row.optional { "Optional." } else { "" };
        let _ = writeln!(out, "| {} | {} | {note} |", cell(row.name), row.example);
    }
    out
}

fn auth_section(out: &mut String, auth: Option<&Auth>) {
    let text = match auth {
        None => "Inherited from the folder or project.".to_owned(),
        Some(Auth::None) => "None.".to_owned(),
        Some(Auth::Basic { .. }) => "Basic: a user name and password.".to_owned(),
        Some(Auth::Bearer { .. }) => "Bearer token.".to_owned(),
        Some(Auth::Header { name, .. }) => format!("The header `{}`.", name.trim()),
    };
    let _ = write!(out, "\n## Auth\n\n{text}\n");
}

fn body_section(out: &mut String, body: &RequestBody) {
    let (content_type, block) = match body {
        RequestBody::None => return,
        RequestBody::Text(text) => (body.default_content_type(), fenced("", text)),
        RequestBody::Json(text) => (
            body.default_content_type(),
            fenced(
                "json",
                &crate::json::pretty(text).unwrap_or_else(|| text.clone()),
            ),
        ),
        RequestBody::Xml(text) => (
            body.default_content_type(),
            fenced(
                "xml",
                &crate::xml::pretty(text).unwrap_or_else(|| text.clone()),
            ),
        ),
        RequestBody::Form(fields) => {
            let rows: Vec<Row<'_>> = fields.iter().map(Row::from).collect();
            (body.default_content_type(), table(&rows))
        }
        RequestBody::Multipart(parts) => {
            let rows: Vec<Row<'_>> = parts
                .iter()
                .map(|p| Row {
                    name: p.name.trim(),
                    example: if p.file {
                        format!("file {}", cell(&p.value))
                    } else {
                        cell(&p.value)
                    },
                    optional: !p.enabled,
                })
                .collect();
            (body.default_content_type(), table(&rows))
        }
    };
    out.push_str("\n## Body\n\n");
    if let Some(content_type) = content_type {
        let _ = write!(out, "{}\n\n", code_span(content_type));
    }
    out.push_str(&block);
}

fn responses_section(out: &mut String, request: &Request) {
    out.push_str("\n## Responses\n");
    if request.examples.is_empty() {
        out.push_str("\n| Status | Meaning |\n|---|---|\n| 200 | |\n");
        return;
    }
    for example in &request.examples {
        let reason = reqwest::StatusCode::from_u16(example.status)
            .ok()
            .and_then(|s| s.canonical_reason())
            .map(|r| format!(" {r}"))
            .unwrap_or_default();
        let _ = write!(
            out,
            "\n### {}{reason}: {}\n",
            example.status,
            example.name.trim()
        );
        if example.body.trim().is_empty() {
            continue;
        }
        let content_type = example
            .headers
            .iter()
            .find(|h| h.is_named("Content-Type"))
            .map_or("", |h| h.value.as_str());
        let block = if crate::model::is_json_content_type(content_type) {
            let body = crate::json::pretty(&example.body).unwrap_or_else(|| example.body.clone());
            fenced("json", &body)
        } else if content_type.contains("xml") {
            fenced("xml", &example.body)
        } else {
            fenced("", &example.body)
        };
        out.push('\n');
        out.push_str(&block);
    }
}

/// `text` as a table cell: inline code on one line, with `|` escaped.
fn cell(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    code_span(&text.replace(['\r', '\n'], " ")).replace('|', "\\|")
}

/// `text` as inline code, with a backtick fence longer than any run of backticks in it.
fn code_span(text: &str) -> String {
    let ticks = "`".repeat(longest_run(text, '`') + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// `text` as a fenced code block in `lang`.
fn fenced(lang: &str, text: &str) -> String {
    let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
    format!("{fence}{lang}\n{}\n{fence}\n", text.trim_end())
}

fn longest_run(text: &str, c: char) -> usize {
    text.split(|other| other != c)
        .map(str::len)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FormPart, HttpMethod};

    /// R-COL-35: the draft lists what the request sends and its examples.
    #[test]
    fn a_draft_lists_what_the_request_sends() {
        let mut request = Request::new(HttpMethod::Post, "{{base}}/users/:id?verbose=1");
        request.name = "Create user ü😀".into();
        request.path_vars[0].value = "42".into();
        request.params.push(KeyValue {
            enabled: false,
            ..KeyValue::new("dry_run", "true")
        });
        request.headers = vec![
            KeyValue::new("Accept", "application/json"),
            KeyValue::new("Authorization", "Bearer s3cret"),
            KeyValue {
                enabled: false,
                ..KeyValue::new("X-Off", "1")
            },
        ];
        request.auth = Some(Auth::Bearer {
            token: "t0ken".into(),
        });
        request.body = RequestBody::Json("{\"name\":\"Ada\"}".into());
        let mut created = crate::mock::Example::new("Created");
        created.status = 201;
        created.body = "{\"id\":42}".into();
        created.headers = vec![KeyValue::new("Content-Type", "application/json")];
        request.examples = vec![created, crate::mock::Example::new("Plain")];

        assert_eq!(
            draft(&request),
            "# Create user ü😀\n\n\
             _What this request does, when to use it and what it changes._\n\n\
             `POST {{base}}/users/:id`\n\n\
             ## Path variables\n\n| Name | Example | Description |\n|---|---|---|\n\
             | `id` | `42` |  |\n\n\
             ## Query parameters\n\n| Name | Example | Description |\n|---|---|---|\n\
             | `verbose` | `1` |  |\n| `dry_run` | `true` | Optional. |\n\n\
             ## Headers\n\n| Name | Example | Description |\n|---|---|---|\n\
             | `Accept` | `application/json` |  |\n| `Authorization` | … |  |\n\n\
             ## Auth\n\nBearer token.\n\n\
             ## Body\n\n`application/json`\n\n```json\n{\n  \"name\": \"Ada\"\n}\n```\n\n\
             ## Responses\n\n### 201 Created: Created\n\n```json\n{\n  \"id\": 42\n}\n```\n\n\
             ### 200 OK: Plain\n"
        );
    }

    /// R-COL-35: secrets never reach the draft.
    #[test]
    fn credentials_are_left_out() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/");
        request.headers = vec![KeyValue::new("cookie", "session=s3cret")];
        request.auth = Some(Auth::Basic {
            username: "ada".into(),
            password: "pa55".into(),
        });
        let text = draft(&request);
        for secret in ["s3cret", "ada", "pa55"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.contains("Basic: a user name and password."));
    }

    /// R-COL-35: a bare request still gets a usable outline.
    #[test]
    fn an_empty_request_gets_an_outline() {
        let mut request = Request::new(HttpMethod::Get, "");
        request.name = String::new();
        assert_eq!(
            draft(&request),
            "# \n\n_What this request does, when to use it and what it changes._\n\n`GET /`\n\n\
             ## Auth\n\nInherited from the folder or project.\n\n\
             ## Responses\n\n| Status | Meaning |\n|---|---|\n| 200 | |\n"
        );
    }

    /// R-COL-35: values that would break the Markdown are fenced and escaped.
    #[test]
    fn markdown_in_values_stays_text() {
        let mut request = Request::new(HttpMethod::Post, "https://a.test/");
        request.params = vec![KeyValue::new("q", "a|b `c`\nd")];
        request.body = RequestBody::Multipart(vec![
            FormPart::text("note", "hi"),
            FormPart {
                file: true,
                ..FormPart::text("photo", "img/a b.png")
            },
        ]);
        let text = draft(&request);
        assert!(text.contains("| `q` | ``a\\|b `c` d`` |  |"), "{text}");
        assert_eq!(code_span("`x`"), "`` `x` ``");
        assert!(
            text.contains("| `photo` | file `img/a b.png` |  |"),
            "{text}"
        );

        request.body = RequestBody::Text("```\ncode\n```".into());
        assert!(draft(&request).contains("````\n```\ncode\n```\n````\n"));
    }
}
