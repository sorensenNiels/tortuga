//! Mock APIs from the example responses saved with requests (R-MOCK-1 to R-MOCK-3,
//! D43, D44, D46).
//!
//! [`MockApi`] decides how a request to the mock is answered: which saved request it
//! matches and which of its examples answers. It is pure, so it compiles for wasm; the
//! listener that feeds it is native only ([`server`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::collection::{Collection, Item};
use crate::format::Environment;
use crate::model::{HttpMethod, KeyValue, Request, Response};
use crate::vars::{self, Context};

pub mod contract;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

/// The port the mock listens on unless the user picks another (R-MOCK-2).
pub const DEFAULT_PORT: u16 = 4010;

/// The request header that picks an example by name (R-MOCK-3).
pub const EXAMPLE_HEADER: &str = "X-Tortuga-Example";

/// The id of the environment that sends requests to the mock (R-MOCK-4, D45). Never
/// saved; a project's own environment ids come from names and can't start with `:`.
pub const ENVIRONMENT_ID: &str = ":mock";

/// A response a request is designed to give (R-MOCK-1), saved in its request file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    pub name: String,
    #[serde(default = "ok")]
    pub status: u16,
    /// Holds the answer back, to try slow responses (R-MOCK-3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    /// May hold `{{variables}}`, filled from the mock's environment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// When this example answers (R-MOCK-3); empty: always.
    #[serde(
        default,
        rename = "match",
        skip_serializing_if = "Conditions::is_empty"
    )]
    pub conditions: Conditions,
}

fn ok() -> u16 {
    200
}

impl Example {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ok(),
            delay_ms: None,
            body: String::new(),
            headers: Vec::new(),
            conditions: Conditions::default(),
        }
    }

    /// An example of `response`, without the headers that carry credentials (P6) or
    /// describe the transfer rather than the response.
    pub fn from_response(name: impl Into<String>, response: &Response) -> Self {
        const LEFT_OUT: [&str; 7] = [
            "authorization",
            "cookie",
            "set-cookie",
            "content-length",
            "transfer-encoding",
            "connection",
            "date",
        ];
        Self {
            status: response.status,
            body: String::from_utf8_lossy(&response.body).into_owned(),
            headers: response
                .headers
                .iter()
                .filter(|h| !LEFT_OUT.iter().any(|name| h.is_named(name)))
                .cloned()
                .collect(),
            ..Self::new(name)
        }
    }
}

/// What a request to the mock must have for an example to answer it (R-MOCK-3).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Conditions {
    /// Query parameter name → value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    /// Header name (any case) → value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// A `JSONPath` or `xpath:` expression on the body, as captures take (R-TEST-4) →
    /// the value it must find.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub body: BTreeMap<String, String>,
}

impl Conditions {
    pub fn is_empty(&self) -> bool {
        self.query.is_empty() && self.headers.is_empty() && self.body.is_empty()
    }

    fn hold_for(&self, request: &MockRequest) -> bool {
        let query = self.query.iter().all(|(name, value)| {
            request
                .query
                .iter()
                .any(|p| p.name == *name && p.value == *value)
        });
        let headers = self
            .headers
            .iter()
            .all(|(name, value)| request.header(name) == Some(value.trim()));
        query && headers && (self.body.is_empty() || self.body_holds(request))
    }

    fn body_holds(&self, request: &MockRequest) -> bool {
        // Captures read a response; the request's body and headers serve as one.
        let as_response = Response {
            status: 0,
            status_text: String::new(),
            headers: request.headers.clone(),
            body: request.body.clone(),
            elapsed: Duration::ZERO,
        };
        self.body.iter().all(|(expr, expected)| {
            crate::runner::capture(expr, &as_response).is_ok_and(|found| found == *expected)
        })
    }
}

/// A request the mock received.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MockRequest {
    pub method: String,
    /// Without the query string, still percent-encoded.
    pub path: String,
    /// Decoded query parameters.
    pub query: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body: Vec<u8>,
}

impl MockRequest {
    /// A request for `target` (a path with an optional query string).
    pub fn new(method: &str, target: &str) -> Self {
        let (path, query) = crate::query::split_url(target);
        Self {
            method: method.to_owned(),
            path,
            query,
            ..Self::default()
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.is_named(name))
            .map(|h| h.value.trim())
    }
}

/// How the mock answers a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<KeyValue>,
    pub body: Vec<u8>,
    pub delay: Option<Duration>,
    pub outcome: Outcome,
}

/// What answered a request, for the mock's log (R-MOCK-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Example {
        /// The request file, relative to the project.
        request: PathBuf,
        request_name: String,
        example: String,
    },
    /// Nothing defined answers it; why, as a sentence.
    NoMatch(String),
}

/// What the mock received and how it answered, for its log (R-MOCK-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub at: std::time::SystemTime,
    pub method: String,
    /// The path with its query string.
    pub target: String,
    pub status: u16,
    pub outcome: Outcome,
}

/// A saved request as the mock sees it.
#[derive(Debug, Clone)]
struct Route {
    request: PathBuf,
    name: String,
    method: HttpMethod,
    server: Option<String>,
    segments: Vec<Segment>,
    /// The path as written, for messages.
    path: String,
    examples: Vec<Example>,
    variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    /// `:name` or `{{name}}`: any one segment.
    Any,
}

impl Route {
    fn new(
        collection: &Collection,
        environment: Option<&str>,
        path: PathBuf,
        request: &Request,
    ) -> Self {
        let written = route_path(request);
        let segments = segments(written)
            .map(|s| {
                if s.starts_with(':') || s.contains("{{") {
                    Segment::Any
                } else {
                    Segment::Literal(decode(s))
                }
            })
            .collect();
        // Secret values are never added, so they never reach a mock answer (P6).
        let variables = Context::for_request(collection, environment, Some(&path)).variables;
        Self {
            name: request.name.clone(),
            method: request.method,
            server: request.server.clone(),
            path: format!("/{}", segments_of(written).join("/")),
            segments,
            examples: request.examples.clone(),
            variables,
            request: path,
        }
    }

    /// How well `path` matches: the number of literal segments, or `None`.
    fn score(&self, path: &[String]) -> Option<usize> {
        if path.len() != self.segments.len() {
            return None;
        }
        let mut literal = 0;
        for (segment, given) in self.segments.iter().zip(path) {
            match segment {
                Segment::Literal(text) if text == given => literal += 1,
                Segment::Literal(_) => return None,
                Segment::Any => {}
            }
        }
        Some(literal)
    }

    /// How many leading segments match, to name the closest requests.
    fn closeness(&self, path: &[String]) -> usize {
        self.segments
            .iter()
            .zip(path)
            .take_while(|(segment, given)| match segment {
                Segment::Literal(text) => text == *given,
                Segment::Any => true,
            })
            .count()
    }

    fn describe(&self) -> String {
        format!("{} {} ({})", self.method, self.path, self.name)
    }

    fn answer(&self, request: &MockRequest) -> Answer {
        let chosen = match request.header(EXAMPLE_HEADER) {
            Some(name) => self
                .examples
                .iter()
                .find(|e| e.name.trim().eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    format!(
                        "“{}” has no example named “{name}” ({EXAMPLE_HEADER})",
                        self.name
                    )
                }),
            None => self
                .examples
                .iter()
                .find(|e| e.conditions.hold_for(request))
                .ok_or_else(|| {
                    if self.examples.is_empty() {
                        format!("“{}” has no examples yet", self.name)
                    } else {
                        format!("no example of “{}” matches this request", self.name)
                    }
                }),
        };
        match chosen {
            Ok(example) => self.example_answer(example),
            Err(reason) => no_match(&reason, &[self.describe()]),
        }
    }

    fn example_answer(&self, example: &Example) -> Answer {
        let mut unknown = BTreeSet::new();
        let fill = |text: &str, unknown: &mut BTreeSet<String>| {
            vars::substitute(text, &self.variables, unknown)
        };
        let mut headers: Vec<KeyValue> = example
            .headers
            .iter()
            .filter(|h| h.is_active())
            .map(|h| KeyValue::new(h.name.trim(), fill(&h.value, &mut unknown)))
            .collect();
        let body = fill(&example.body, &mut unknown);
        if !body.is_empty() && !headers.iter().any(|h| h.is_named("content-type")) {
            headers.push(KeyValue::new("Content-Type", guess_content_type(&body)));
        }
        Answer {
            status: example.status,
            headers,
            body: body.into_bytes(),
            delay: example.delay_ms.map(Duration::from_millis),
            outcome: Outcome::Example {
                request: self.request.clone(),
                request_name: self.name.clone(),
                example: example.name.clone(),
            },
        }
    }
}

/// Answers requests from a project's examples (R-MOCK-2, R-MOCK-3).
#[derive(Debug, Clone, Default)]
pub struct MockApi {
    project: String,
    routes: Vec<Route>,
}

impl MockApi {
    /// The mock of `collection`, with `{{variables}}` in examples taken from
    /// `environment` (an id) and the project's defaults.
    pub fn new(collection: &Collection, environment: Option<&str>) -> Self {
        let mut routes = Vec::new();
        collect(collection, environment, &collection.items, &mut routes);
        Self {
            project: collection.name().to_owned(),
            routes,
        }
    }

    /// The number of requests with at least one example.
    pub fn example_count(&self) -> usize {
        self.routes
            .iter()
            .filter(|r| !r.examples.is_empty())
            .count()
    }

    pub fn answer(&self, request: &MockRequest) -> Answer {
        let path: Vec<String> = segments(&request.path).map(decode).collect();
        let method = request.method.parse::<HttpMethod>().ok();
        // With several servers, the mock's environment puts each under `/<server id>`
        // (R-MOCK-4); a path without such a prefix is matched against every request.
        let by_server = path.split_first().and_then(|(first, rest)| {
            self.best(method, rest, |r| {
                r.server.as_deref() == Some(first.as_str())
            })
        });
        match by_server.or_else(|| self.best(method, &path, |_| true)) {
            Some(route) => route.answer(request),
            None => self.nothing_matches(request, method, &path),
        }
    }

    fn best(
        &self,
        method: Option<HttpMethod>,
        path: &[String],
        server: impl Fn(&Route) -> bool,
    ) -> Option<&Route> {
        let mut best: Option<(&Route, usize)> = None;
        for route in self
            .routes
            .iter()
            .filter(|r| Some(r.method) == method && server(r))
        {
            if let Some(score) = route.score(path)
                && best.is_none_or(|(_, s)| score > s)
            {
                best = Some((route, score));
            }
        }
        best.map(|(route, _)| route)
    }

    fn nothing_matches(
        &self,
        request: &MockRequest,
        method: Option<HttpMethod>,
        path: &[String],
    ) -> Answer {
        let other_methods: Vec<String> = self
            .routes
            .iter()
            .filter(|r| r.score(path).is_some() && Some(r.method) != method)
            .map(Route::describe)
            .collect();
        if !other_methods.is_empty() {
            return no_match(
                &format!(
                    "no request of “{}” is {} {}; the path is defined for another method",
                    self.project, request.method, request.path
                ),
                &other_methods,
            );
        }
        let mut closest: Vec<(&Route, usize)> = self
            .routes
            .iter()
            .map(|r| (r, r.closeness(path)))
            .filter(|(_, n)| *n > 0)
            .collect();
        closest.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let closest: Vec<String> = closest.iter().take(3).map(|(r, _)| r.describe()).collect();
        no_match(
            &format!(
                "no request of “{}” is {} {}",
                self.project, request.method, request.path
            ),
            &closest,
        )
    }
}

fn collect(
    collection: &Collection,
    environment: Option<&str>,
    items: &[Item],
    out: &mut Vec<Route>,
) {
    for item in items {
        match item {
            Item::Request(r) => out.push(Route::new(
                collection,
                environment,
                r.path.clone(),
                &r.request,
            )),
            Item::Folder(f) => collect(collection, environment, &f.items, out),
            Item::Broken(_) => {}
        }
    }
}

/// The environment that sends `collection`'s requests to a mock at `base`
/// (`http://127.0.0.1:4010`), R-MOCK-4: every server points at it, under `/<server id>`
/// when there are several, and variables keep their default values.
pub fn environment(collection: &Collection, base: &str) -> Environment {
    let servers = &collection.manifest.servers;
    let base = base.trim_end_matches('/');
    Environment {
        id: ENVIRONMENT_ID.to_owned(),
        name: "Mock".to_owned(),
        seq: None,
        color: None,
        // Only answers from examples: nothing to protect (R-SRV-9).
        protected: false,
        servers: servers
            .iter()
            .map(|s| {
                let url = if servers.len() > 1 {
                    format!("{base}/{}", s.id)
                } else {
                    base.to_owned()
                };
                (s.id.clone(), url)
            })
            .collect(),
        variables: BTreeMap::new(),
    }
}

/// The `404` for a request nothing answers: a JSON body with the reason and the
/// closest requests, so a developer sees what is not defined yet (R-MOCK-2).
fn no_match(reason: &str, closest: &[String]) -> Answer {
    let reason = crate::error::sentence(reason);
    let body = serde_json::json!({ "error": reason, "closest": closest });
    Answer {
        status: 404,
        headers: vec![KeyValue::new("Content-Type", "application/json")],
        body: serde_json::to_vec_pretty(&body).unwrap_or_default(),
        delay: None,
        outcome: Outcome::NoMatch(reason),
    }
}

/// The path part of a request's URL: as written with a server, otherwise without
/// the scheme and host (or the `{{variable}}` that stands for them).
fn route_path(request: &Request) -> &str {
    let url = request.url.trim();
    if request.server.is_some() {
        return url;
    }
    if let Some(rest) = url.strip_prefix("{{") {
        return rest.find("}}").map_or("", |i| &rest[i + 2..]);
    }
    match url.find("://") {
        Some(i) => {
            let rest = &url[i + 3..];
            rest.find('/').map_or("", |slash| &rest[slash..])
        }
        None => url,
    }
}

fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|s| !s.is_empty())
}

fn segments_of(path: &str) -> Vec<&str> {
    segments(path).collect()
}

fn decode(segment: &str) -> String {
    percent_encoding::percent_decode_str(segment)
        .decode_utf8_lossy()
        .into_owned()
}

/// The Content-Type of an example body that doesn't say: JSON, XML or text.
fn guess_content_type(body: &str) -> &'static str {
    if serde_json::from_str::<serde_json::Value>(body).is_ok() {
        "application/json"
    } else if body.trim_start().starts_with('<') {
        "application/xml"
    } else {
        "text/plain; charset=utf-8"
    }
}

#[cfg(test)]
mod tests;
