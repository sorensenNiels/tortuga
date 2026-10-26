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
    /// GET, HEAD and OPTIONS: methods that don't change anything on a well-behaved
    /// server, so a protected environment lets them through (R-SRV-9).
    pub fn is_safe(self) -> bool {
        match self {
            Self::Get | Self::Head | Self::Options => true,
            Self::Post | Self::Put | Self::Patch | Self::Delete => false,
        }
    }

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

    /// Whether this is called `name`, the way header names compare: ignoring case and
    /// surrounding spaces.
    pub fn is_named(&self, name: &str) -> bool {
        self.name.trim().eq_ignore_ascii_case(name.trim())
    }
}

/// Whether a `Content-Type` value says JSON: `application/json`, `text/json` or a
/// `+json` type such as `application/problem+json`. Case and parameters are ignored.
pub fn is_json_content_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or_default().trim();
    essence.rsplit_once('/').is_some_and(|(_, subtype)| {
        let subtype = subtype.to_ascii_lowercase();
        subtype == "json" || subtype == "x-json" || subtype.ends_with("+json")
    })
}

/// Whether a `Content-Type` value says XML: `application/xml`, `text/xml` or a `+xml`
/// type such as `application/soap+xml` (R-HTTP-19). Case and parameters are ignored.
pub fn is_xml_content_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or_default().trim();
    essence.rsplit_once('/').is_some_and(|(_, subtype)| {
        let subtype = subtype.to_ascii_lowercase();
        subtype == "xml" || subtype.ends_with("+xml")
    })
}

/// Request body (R-HTTP-4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "content", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Text(String),
    Json(String),
    /// XML text (R-HTTP-19).
    Xml(String),
    Form(Vec<KeyValue>),
    /// `multipart/form-data` (R-HTTP-15).
    Multipart(Vec<FormPart>),
}

impl RequestBody {
    /// The Content-Type sent when the user has not set one (R-HTTP-4). A multipart
    /// body always gets its own, with the boundary.
    pub fn default_content_type(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Text(_) => Some("text/plain; charset=utf-8"),
            Self::Json(_) => Some("application/json"),
            Self::Xml(_) => Some("application/xml"),
            Self::Form(_) => Some("application/x-www-form-urlencoded"),
            Self::Multipart(_) => Some("multipart/form-data"),
        }
    }
}

/// The body type query params move into (R-HTTP-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsBody {
    Json,
    Xml,
}

/// One part of a `multipart/form-data` body (R-HTTP-15): a text value, or a file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FormPart {
    pub name: String,
    /// The text, or the path of the file (relative to the project folder, D21).
    #[serde(default)]
    pub value: String,
    /// `value` is a file to upload.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub file: bool,
    /// Overrides the part's content type (for files, otherwise guessed by the server).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default = "enabled_by_default", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

impl FormPart {
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            enabled: true,
            ..Self::default()
        }
    }

    pub fn file(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            file: true,
            ..Self::text(name, path)
        }
    }

    /// Enabled and named.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.name.trim().is_empty()
    }
}

/// Authentication sent with a request (R-HTTP-5). Fields may contain `{{variables}}`;
/// use secret variables (R-VAR-3) so credentials are not committed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Auth {
    #[default]
    None,
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    Bearer {
        #[serde(default)]
        token: String,
    },
    /// Any header and value, e.g. `X-API-Key: {{api_key}}`.
    Header {
        #[serde(default)]
        name: String,
        #[serde(default)]
        value: String,
    },
}

impl Auth {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }

    /// Every text field, for variable substitution and checks.
    pub fn fields_mut(&mut self) -> Vec<&mut String> {
        match self {
            Self::None => vec![],
            Self::Basic { username, password } => vec![username, password],
            Self::Bearer { token } => vec![token],
            Self::Header { name, value } => vec![name, value],
        }
    }

    pub fn fields(&self) -> Vec<&str> {
        match self {
            Self::None => vec![],
            Self::Basic { username, password } => vec![username, password],
            Self::Bearer { token } => vec![token],
            Self::Header { name, value } => vec![name, value],
        }
    }

    /// The header this auth adds as (name, value), or `None` for no auth or a
    /// header without a name.
    pub fn header(&self) -> Option<(String, String)> {
        use base64::Engine as _;
        match self {
            Self::None => None,
            Self::Basic { username, password } => Some((
                "Authorization".to_owned(),
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD
                        .encode(format!("{username}:{password}"))
                ),
            )),
            Self::Bearer { token } => Some((
                "Authorization".to_owned(),
                format!("Bearer {}", token.trim()),
            )),
            Self::Header { name, value } => {
                (!name.trim().is_empty()).then(|| (name.trim().to_owned(), value.trim().to_owned()))
            }
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
    /// What the request is for, in Markdown (R-COL-22).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub method: HttpMethod,
    /// Id of a collection server (R-SRV-3); `None` means `url` is absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub url: String,
    #[serde(default)]
    pub params: Vec<KeyValue>,
    /// Values of the `:name` path variables in `url`, in URL order (R-HTTP-10).
    /// Kept in step with `url` by [`Request::sync_path_variables`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_vars: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: RequestBody,
    /// Overrides the global default timeout (R-HTTP-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// `None` = use the collection's default auth (R-HTTP-5, R-COL-4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// Use the collection's cookie jar: send its cookies and store new ones (R-CK-2).
    #[serde(default = "enabled_by_default", skip_serializing_if = "is_true")]
    pub cookies: bool,
    /// Follow redirects, as worked out from the project's, workspace's and app's
    /// settings when the request is prepared (R-SET-1); `None`: the client's setting.
    /// Not saved: requests don't set it themselves.
    #[serde(skip)]
    pub follow_redirects: Option<bool>,
    /// Pre-request and test scripts (R-TEST-1, R-TEST-2).
    #[serde(default, skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
    /// Values put into variables from the response: variable name → `$.json.path`,
    /// `header:<name>` or `status` (R-TEST-4).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub capture: std::collections::BTreeMap<String, String>,
    /// Responses this request is designed to give, served by the mock server (R-MOCK-1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<crate::mock::Example>,
}

/// The scripts of a request (R-TEST-1, R-TEST-2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Scripts {
    /// Runs before variables are filled in and the request is sent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_request: String,
    /// Runs after the response arrived.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tests: String,
}

impl Scripts {
    pub fn is_empty(&self) -> bool {
        self.pre_request.trim().is_empty() && self.tests.trim().is_empty()
    }
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
        let mut request = Self {
            name: String::from("Untitled request"),
            description: String::new(),
            method,
            server: None,
            url,
            params,
            path_vars: Vec::new(),
            headers: Vec::new(),
            body: RequestBody::None,
            timeout_ms: None,
            auth: None,
            cookies: true,
            follow_redirects: None,
            scripts: Scripts::default(),
            capture: std::collections::BTreeMap::new(),
            examples: Vec::new(),
        };
        request.sync_path_variables();
        request
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
        self.sync_path_variables();
    }

    /// Whether the query params could move into the body (R-HTTP-21): the method sends
    /// a body, there is none yet, and a param takes part in the request.
    pub fn can_move_params_to_body(&self) -> bool {
        !self.method.is_safe()
            && self.body == RequestBody::None
            && self.params.iter().any(KeyValue::is_active)
    }

    /// Moves the active query params into a new body of type `to` (R-HTTP-21).
    /// Switched-off params stay in the query table.
    pub fn move_params_to_body(&mut self, to: ParamsBody) {
        self.body = match to {
            ParamsBody::Json => RequestBody::Json(query::params_as_json(&self.params)),
            ParamsBody::Xml => RequestBody::Xml(query::params_as_xml(&self.params)),
        };
        self.params.retain(|p| !p.is_active());
    }

    /// Makes `path_vars` match the `:name` segments of `url`: values of names still
    /// in the URL are kept, new names start empty, names no longer in it are dropped.
    pub fn sync_path_variables(&mut self) {
        let old = std::mem::take(&mut self.path_vars);
        self.path_vars = query::path_variable_names(&self.url)
            .into_iter()
            .map(|name| {
                let value = old
                    .iter()
                    .find(|v| v.name == name)
                    .map(|v| v.value.clone())
                    .unwrap_or_default();
                KeyValue::new(name, value)
            })
            .collect();
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }

    /// True if an enabled header with this name exists (case-insensitive).
    pub fn has_header(&self, name: &str) -> bool {
        self.header(name).is_some()
    }

    /// The value of the first enabled header called `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.is_active() && h.is_named(name))
            .map(|h| h.value.as_str())
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
    /// Where `elapsed` went (R-HTTP-25). `None` for a response that wasn't just sent
    /// (one from history, the mock's own answers, tests).
    pub timing: Option<Timing>,
}

/// Where a response's time went (R-HTTP-25). The phases add up to the response's
/// `elapsed`; with redirects followed, each phase is the sum over every hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Looking up the host name. `None` when it wasn't measured (the web build).
    pub dns: Option<Duration>,
    /// Opening the connection, with the TLS handshake and a proxy's tunnel. `None` when
    /// it wasn't measured (the web build).
    pub connect: Option<Duration>,
    /// From sending the request until the response's headers arrived: sending the body
    /// and the server's work.
    pub waiting: Duration,
    /// From the headers until the last byte of the body.
    pub download: Duration,
    pub connection: Connection,
    /// The HTTP version the response came with, e.g. `HTTP/2`; `None` in the web build,
    /// where the browser doesn't say.
    pub version: Option<&'static str>,
}

/// Whether a send opened a new connection (R-HTTP-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    New,
    /// An open connection from an earlier send was used: no lookup, no connecting.
    Reused,
    /// The platform doesn't say (the web build).
    Unknown,
}

impl Timing {
    /// The phases to show, in order, with their names; phases that weren't measured are
    /// left out, and so are lookup and connecting on a reused connection.
    pub fn phases(&self) -> Vec<(Phase, Duration)> {
        let mut phases = Vec::new();
        if self.connection != Connection::Reused {
            phases.extend(self.dns.map(|d| (Phase::Dns, d)));
            phases.extend(self.connect.map(|d| (Phase::Connect, d)));
        }
        phases.push((Phase::Waiting, self.waiting));
        phases.push((Phase::Download, self.download));
        phases
    }
}

/// A phase of a send (R-HTTP-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Dns,
    Connect,
    Waiting,
    Download,
}

/// How a response body should be presented (R-HTTP-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyView {
    Empty,
    /// Pretty-printed JSON.
    Json(String),
    /// Pretty-printed XML (R-HTTP-19).
    Xml(String),
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
            .find(|h| h.is_named(name))
            .map(|h| h.value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Decides how to show the body: JSON is pretty-printed when the
    /// content type says JSON (or the text parses as JSON), XML when the content type
    /// says XML (or there is none and the text parses as XML, R-HTTP-19), other UTF-8
    /// is text, anything else is binary.
    pub fn body_view(&self) -> BodyView {
        if self.body.is_empty() {
            return BodyView::Empty;
        }
        let Ok(text) = std::str::from_utf8(&self.body) else {
            return BodyView::Binary;
        };
        let looks_json = self.content_type().is_some_and(is_json_content_type)
            || matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'['));
        if looks_json
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            && let Ok(pretty) = serde_json::to_string_pretty(&value)
        {
            return BodyView::Json(pretty);
        }
        if self.looks_xml(text)
            && let Some(pretty) = crate::xml::pretty(text)
        {
            return BodyView::Xml(pretty);
        }
        BodyView::Text(text.to_owned())
    }

    /// The content type says XML, or there is none and `text` starts like markup.
    pub fn looks_xml(&self, text: &str) -> bool {
        match self.content_type() {
            Some(content_type) => is_xml_content_type(content_type),
            None => text.trim_start().starts_with('<'),
        }
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
            Some("image/svg+xml") => "svg",
            Some(ct) if is_xml_content_type(ct) => "xml",
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

    #[test]
    fn params_move_into_the_body_only_when_it_is_empty_r_http_21() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/users?q=ada&n=2");
        request.params.push(KeyValue {
            enabled: false,
            ..KeyValue::new("debug", "1")
        });
        assert!(!request.can_move_params_to_body(), "GET sends no body");
        request.method = HttpMethod::Post;
        assert!(request.can_move_params_to_body());

        request.move_params_to_body(ParamsBody::Json);
        assert_eq!(
            request.body,
            RequestBody::Json("{\n  \"q\": \"ada\",\n  \"n\": 2\n}".into())
        );
        assert_eq!(request.params.len(), 1, "switched-off params stay");
        assert_eq!(request.display_url(), "https://a.test/users");
        assert!(!request.can_move_params_to_body(), "the body is taken now");

        let mut request = Request::new(HttpMethod::Put, "https://a.test/?x=1");
        request.move_params_to_body(ParamsBody::Xml);
        assert!(matches!(&request.body, RequestBody::Xml(x) if x.contains("<x>1</x>")));
        assert_eq!(request.params, []);
    }

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
            timing: None,
        }
    }

    #[test]
    fn json_content_types() {
        for json in [
            "application/json",
            "Application/JSON; charset=utf-8",
            "text/json",
            "application/x-json",
            "application/problem+json",
            "application/vnd.api+json ; ext=bulk",
        ] {
            assert!(is_json_content_type(json), "{json}");
        }
        for other in [
            "application/jsonl",
            "text/plain",
            "application/json-seq",
            "json",
            "",
        ] {
            assert!(!is_json_content_type(other), "{other}");
        }
    }

    #[test]
    fn header_names_ignore_case_and_spaces() {
        let request = Request {
            headers: vec![
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("Accept", "off")
                },
                KeyValue::new(" content-TYPE ", "application/json"),
            ],
            ..Request::default()
        };
        assert_eq!(request.header("Content-Type"), Some("application/json"));
        assert_eq!(
            request.header("accept"),
            None,
            "disabled headers don't count"
        );
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
        req.auth = Some(Auth::Bearer {
            token: "{{token}}".into(),
        });
        req.cookies = false;
        req.description = "# Ü😀\n\n- one".into();

        let json = serde_json::to_string(&req).expect("serialize");
        let back: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, back);
    }

    /// R-COL-22: drafts saved before descriptions existed still load.
    #[test]
    fn request_without_a_description_loads() {
        let json = r#"{"name": "Old", "method": "GET", "url": "https://a.test/"}"#;
        let back: Request = serde_json::from_str(json).expect("deserialize");
        assert_eq!(back.description, "");
        assert!(
            !serde_json::to_string(&back)
                .expect("serialize")
                .contains("description")
        );
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
    fn auth_header_values() {
        let basic = Auth::Basic {
            username: "Aladdin".into(),
            password: "open sesame".into(),
        };
        // Example from RFC 7617.
        let authorization = |value: &str| Some(("Authorization".to_owned(), value.to_owned()));
        assert_eq!(
            basic.header(),
            authorization("Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==")
        );
        let bearer = Auth::Bearer {
            token: " abc ".into(),
        };
        assert_eq!(bearer.header(), authorization("Bearer abc"));
        assert_eq!(Auth::None.header(), None);
    }

    /// R-HTTP-22: any header name and value.
    #[test]
    fn header_auth_sends_its_own_header() {
        let key = Auth::Header {
            name: " X-API-Key ".into(),
            value: " ü😀 ".into(),
        };
        assert_eq!(
            key.header(),
            Some(("X-API-Key".to_owned(), "ü😀".to_owned()))
        );
        let unnamed = Auth::Header {
            name: "  ".into(),
            value: "k".into(),
        };
        assert_eq!(unnamed.header(), None, "no header without a name");
    }

    #[test]
    fn path_variables_follow_the_url_and_keep_values() {
        let mut req = Request::new(HttpMethod::Get, "https://a.test/users/:id");
        assert_eq!(req.path_vars, [KeyValue::new("id", "")]);
        req.path_vars[0].value = "42".into();
        req.set_display_url("https://a.test/users/:id/orders/:order?x=1");
        assert_eq!(
            req.path_vars,
            [KeyValue::new("id", "42"), KeyValue::new("order", "")]
        );
        req.set_display_url("https://a.test/users");
        assert_eq!(req.path_vars, Vec::<KeyValue>::new());
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

    /// R-HTTP-19.
    #[test]
    fn xml_content_types() {
        for xml in [
            "application/xml",
            "Text/XML; charset=utf-8",
            "application/soap+xml",
            "application/atom+xml ; type=feed",
        ] {
            assert!(is_xml_content_type(xml), "{xml}");
        }
        for other in [
            "text/html",
            "application/xml-dtd",
            "xml",
            "application/json",
            "",
        ] {
            assert!(!is_xml_content_type(other), "{other}");
        }
    }

    /// R-HTTP-19.
    #[test]
    fn xml_body_is_pretty_printed() {
        let res = response(Some("text/xml"), "<a><b>ü😀</b></a>".as_bytes());
        assert_eq!(
            res.body_view(),
            BodyView::Xml("<a>\n  <b>ü😀</b>\n</a>".into())
        );
        assert_eq!(res.suggested_file_name(), "response.xml");
        let detected = response(None, b"<?xml version=\"1.0\"?><a/>");
        assert!(matches!(detected.body_view(), BodyView::Xml(_)));
    }

    /// R-HTTP-19: HTML is not reformatted, and broken XML is shown as it came.
    #[test]
    fn html_and_invalid_xml_stay_text() {
        let html = response(Some("text/html"), b"<p>hi</p>");
        assert_eq!(html.body_view(), BodyView::Text("<p>hi</p>".into()));
        let broken = response(Some("application/xml"), b"<a><b></a>");
        assert_eq!(broken.body_view(), BodyView::Text("<a><b></a>".into()));
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
