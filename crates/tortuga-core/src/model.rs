//! Serializable data model shared by every frontend.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::query;

/// HTTP methods supported by Tortuga (R-HTTP-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub const ALL: [Self; 7] = [
        Self::Get,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
        Self::Head,
        Self::Options,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("unknown HTTP method `{s}`"))
    }
}

/// A name/value pair that can be switched off without deleting it.
/// Used for query params, headers and form fields (R-HTTP-2, R-HTTP-3).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
    #[serde(default = "enabled_by_default", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "signature required by serde"
)]
fn is_true(b: &bool) -> bool {
    *b
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            enabled: true,
        }
    }

    /// Enabled and with a non-blank name, i.e. it takes part in the request.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.name.trim().is_empty()
    }
}

/// Request body (R-HTTP-4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "content", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Text(String),
    Json(String),
    Form(Vec<KeyValue>),
}

impl RequestBody {
    /// The Content-Type sent when the user has not set one (R-HTTP-4).
    pub fn default_content_type(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Text(_) => Some("text/plain; charset=utf-8"),
            Self::Json(_) => Some("application/json"),
            Self::Form(_) => Some("application/x-www-form-urlencoded"),
        }
    }
}

/// A request as the user edits and saves it.
///
/// `url` holds everything before the `?`; the query string lives in `params`
/// so that individual params can be disabled (R-HTTP-2). Use
/// [`Request::display_url`] / [`Request::set_display_url`] for the URL bar.
///
/// With a `server` (R-SRV-3), `url` is a path relative to that server's base URL
/// in the active environment; [`crate::vars::prepare`] makes it absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
    pub method: HttpMethod,
    /// Id of a collection server (R-SRV-3); `None` means `url` is absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub url: String,
    #[serde(default)]
    pub params: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: RequestBody,
    /// Overrides the global default timeout (R-HTTP-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl Default for Request {
    fn default() -> Self {
        Self::new(HttpMethod::Get, "")
    }
}

impl Request {
    /// Creates a request from a full URL; any query string is split into `params`.
    pub fn new(method: HttpMethod, url: impl AsRef<str>) -> Self {
        let (url, params) = query::split_url(url.as_ref());
        Self {
            name: String::from("Untitled request"),
            method,
            server: None,
            url,
            params,
            headers: Vec::new(),
            body: RequestBody::None,
            timeout_ms: None,
        }
    }

    /// The URL as shown in the URL bar: base URL plus enabled params.
    pub fn display_url(&self) -> String {
        query::join_url(&self.url, &self.params)
    }

    /// Updates from text typed in the URL bar. Enabled params are replaced by
    /// what was typed; disabled params are kept (R-HTTP-2).
    pub fn set_display_url(&mut self, text: &str) {
        let (url, typed) = query::split_url(text);
        self.url = url;
        let disabled = self.params.drain(..).filter(|p| !p.enabled);
        self.params = typed.into_iter().chain(disabled).collect();
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }

    /// True if an enabled header with this name exists (case-insensitive).
    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|h| h.is_active() && h.name.trim().eq_ignore_ascii_case(name))
    }
}

/// The result of executing a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    pub body: Vec<u8>,
    pub elapsed: Duration,
}

/// How a response body should be presented (R-HTTP-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyView {
    Empty,
    /// Pretty-printed JSON.
    Json(String),
    Text(String),
    /// Not valid UTF-8; offer saving to a file instead of rendering.
    Binary,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Decides how to show the body: JSON is pretty-printed when the
    /// content type says JSON (or the text parses as JSON), other UTF-8 is
    /// text, anything else is binary.
    pub fn body_view(&self) -> BodyView {
        if self.body.is_empty() {
            return BodyView::Empty;
        }
        let Ok(text) = std::str::from_utf8(&self.body) else {
            return BodyView::Binary;
        };
        let looks_json = self.content_type().is_some_and(|ct| {
            let ct = ct.to_ascii_lowercase();
            ct.contains("/json") || ct.contains("+json")
        }) || matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'['));
        if looks_json
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            && let Ok(pretty) = serde_json::to_string_pretty(&value)
        {
            return BodyView::Json(pretty);
        }
        BodyView::Text(text.to_owned())
    }

    /// File name suggestion when saving the body, based on the content type.
    pub fn suggested_file_name(&self) -> String {
        let ext = match self
            .content_type()
            .map(|ct| ct.split(';').next().unwrap_or("").trim())
        {
            Some("application/json") => "json",
            Some("text/html") => "html",
            Some("text/plain") => "txt",
            Some("application/xml" | "text/xml") => "xml",
            Some("application/pdf") => "pdf",
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            Some("image/gif") => "gif",
            Some("application/zip") => "zip",
            _ => "bin",
        };
        format!("response.{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(content_type: Option<&str>, body: &[u8]) -> Response {
        Response {
            status: 200,
            status_text: "OK".into(),
            headers: content_type
                .map(|ct| KeyValue::new("Content-Type", ct))
                .into_iter()
                .collect(),
            body: body.to_vec(),
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn method_parses_case_insensitively() {
        assert_eq!("patch".parse::<HttpMethod>(), Ok(HttpMethod::Patch));
        assert!("FETCH".parse::<HttpMethod>().is_err());
    }

    #[test]
    fn request_round_trips_through_json() {
        let mut req = Request::new(HttpMethod::Post, "https://example.com/x?a=1");
        req.headers
            .push(KeyValue::new("Accept", "application/json"));
        req.body = RequestBody::Form(vec![KeyValue::new("k", "v")]);
        req.timeout_ms = Some(500);

        let json = serde_json::to_string(&req).expect("serialize");
        let back: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, back);
    }

    #[test]
    fn new_request_splits_query_into_params() {
        let req = Request::new(HttpMethod::Get, "https://api.test/items?page=2&q=a%20b");
        assert_eq!(req.url, "https://api.test/items");
        assert_eq!(
            req.params,
            vec![KeyValue::new("page", "2"), KeyValue::new("q", "a b")]
        );
        assert_eq!(req.display_url(), "https://api.test/items?page=2&q=a%20b");
    }

    #[test]
    fn editing_url_keeps_disabled_params() {
        let mut req = Request::new(HttpMethod::Get, "https://api.test/?a=1&b=2");
        req.params[1].enabled = false;
        assert_eq!(req.display_url(), "https://api.test/?a=1");

        req.set_display_url("https://api.test/?a=1&c=3");
        let names: Vec<_> = req
            .params
            .iter()
            .map(|p| (p.name.as_str(), p.enabled))
            .collect();
        assert_eq!(names, [("a", true), ("c", true), ("b", false)]);
    }

    #[test]
    fn has_header_ignores_disabled_and_case() {
        let mut req = Request::default();
        req.headers.push(KeyValue::new("content-TYPE", "x"));
        assert!(req.has_header("Content-Type"));
        req.headers[0].enabled = false;
        assert!(!req.has_header("Content-Type"));
    }

    #[test]
    fn json_body_is_pretty_printed_preserving_key_order() {
        let res = response(
            Some("application/json; charset=utf-8"),
            br#"{"z":1,"a":[true]}"#,
        );
        assert_eq!(
            res.body_view(),
            BodyView::Json("{\n  \"z\": 1,\n  \"a\": [\n    true\n  ]\n}".into())
        );
    }

    #[test]
    fn json_without_content_type_is_detected() {
        let res = response(None, b"[1,2]");
        assert!(matches!(res.body_view(), BodyView::Json(_)));
    }

    #[test]
    fn invalid_json_falls_back_to_text() {
        let res = response(Some("application/json"), b"{not json");
        assert_eq!(res.body_view(), BodyView::Text("{not json".into()));
    }

    #[test]
    fn non_utf8_is_binary_and_empty_is_empty() {
        assert_eq!(
            response(Some("image/png"), &[0x89, 0xff, 0x00]).body_view(),
            BodyView::Binary
        );
        assert_eq!(response(None, b"").body_view(), BodyView::Empty);
        assert_eq!(
            response(Some("image/png"), &[0]).suggested_file_name(),
            "response.png"
        );
    }
}
