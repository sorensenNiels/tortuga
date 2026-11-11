//! What changed in a project's API between two revisions (R-CLI-7, D77): requests
//! added, removed, moved and changed, compared by what a client sees (method, path,
//! params, headers, auth type, body shape, examples), never by values (P6). A pure
//! function over two loaded projects; reading a git revision is the caller's job.

mod markdown;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub use markdown::markdown;

use crate::changes::{Change, json_shape_changes};
use crate::collection::{Collection, Item};
use crate::mock::Example;
use crate::model::{Auth, FormPart, KeyValue, Request, RequestBody};
use crate::workflow::path_text;

/// The requests that changed, removed and moved ones first, then changed, then added,
/// each by file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApiDiff {
    pub requests: Vec<RequestDiff>,
}

/// One request that changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestDiff {
    /// Its file in the project, as it is after (before, when removed).
    pub file: String,
    pub method: String,
    /// The URL's path, without its server, base URL variable or query.
    pub path: String,
    pub name: String,
    pub status: Status,
    pub changes: Vec<Detail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Removed,
    /// Its file moved; `from` is the old one.
    Moved {
        from: String,
    },
    Changed,
    Added,
}

/// One change of a request, as Markdown text, e.g. param `page` removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detail {
    pub text: String,
    /// May break clients: something removed, renamed or changed (D77).
    pub breaking: bool,
}

impl ApiDiff {
    /// How many requests have a change that may break clients.
    pub fn breaking(&self) -> usize {
        self.requests.iter().filter(|r| r.breaking()).count()
    }
}

impl RequestDiff {
    pub fn breaking(&self) -> bool {
        self.status == Status::Removed || self.changes.iter().any(|c| c.breaking)
    }
}

/// How the project's API changed from `before` to `after`; `None` where the project
/// does not exist at that revision.
pub fn diff(before: Option<&Collection>, after: Option<&Collection>) -> ApiDiff {
    let old = requests(before);
    let new = requests(after);
    let mut added: Vec<&Path> = new
        .keys()
        .filter(|p| !old.contains_key(*p))
        .map(PathBuf::as_path)
        .collect();
    let mut out = Vec::new();
    for (path, &a) in &old {
        if let Some(&b) = new.get(path) {
            let changes = compare(a, b);
            if !changes.is_empty() {
                out.push(entry(path, b, Status::Changed, changes));
            }
            continue;
        }
        let moved = added.iter().position(|p| {
            new.get(*p)
                .is_some_and(|b| a.method == b.method && api_path(&a.url) == api_path(&b.url))
        });
        match moved.map(|i| added.remove(i)) {
            Some(to) => {
                let b = new[to];
                let from = path_text(path);
                out.push(entry(to, b, Status::Moved { from }, compare(a, b)));
            }
            None => out.push(entry(path, a, Status::Removed, Vec::new())),
        }
    }
    for path in added {
        out.push(entry(path, new[path], Status::Added, Vec::new()));
    }
    out.sort_by_key(|r| (rank(&r.status), r.file.clone()));
    ApiDiff { requests: out }
}

fn rank(status: &Status) -> u8 {
    match status {
        Status::Removed => 0,
        Status::Moved { .. } => 1,
        Status::Changed => 2,
        Status::Added => 3,
    }
}

fn entry(path: &Path, r: &Request, status: Status, changes: Vec<Detail>) -> RequestDiff {
    RequestDiff {
        file: path_text(path),
        method: r.method.as_str().to_owned(),
        path: api_path(&r.url).to_owned(),
        name: r.name.clone(),
        status,
        changes,
    }
}

/// Every request of the project by file.
fn requests(c: Option<&Collection>) -> BTreeMap<PathBuf, &Request> {
    fn walk<'a>(items: &'a [Item], out: &mut BTreeMap<PathBuf, &'a Request>) {
        for item in items {
            match item {
                Item::Folder(f) => walk(&f.items, out),
                Item::Request(r) => {
                    out.insert(r.path.clone(), &r.request);
                }
                Item::Broken(_) => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    if let Some(c) = c {
        walk(&c.items, &mut out);
    }
    out
}

/// The path of `url`: without a leading `{{base_url}}` or `scheme://host`, and without
/// its query. `/` when there is none.
pub fn api_path(url: &str) -> &str {
    let url = url.split(['?', '#']).next().unwrap_or_default().trim();
    let rest = if let Some(after) = url.strip_prefix("{{") {
        after
            .find("}}")
            .and_then(|i| after.get(i + 2..))
            .unwrap_or(url)
    } else if let Some(i) = url.find("://") {
        let after = url.get(i + 3..).unwrap_or_default();
        after
            .find('/')
            .and_then(|j| after.get(j..))
            .unwrap_or_default()
    } else {
        url
    };
    if rest.is_empty() { "/" } else { rest }
}

/// Collects the details of one request.
#[derive(Default)]
struct Details(Vec<Detail>);

impl Details {
    fn add(&mut self, text: String, breaking: bool) {
        self.0.push(Detail { text, breaking });
    }
}

/// `` `text` ``, with backticks in it made quotes so the code span holds.
fn code(text: &str) -> String {
    format!("`{}`", text.replace('`', "'"))
}

/// How `b` differs from `a` for a client (D77).
fn compare(a: &Request, b: &Request) -> Vec<Detail> {
    let mut out = Details::default();
    if a.method != b.method {
        let (x, y) = (a.method.as_str(), b.method.as_str());
        out.add(format!("method {} → {}", code(x), code(y)), true);
    }
    let (pa, pb) = (api_path(&a.url), api_path(&b.url));
    if pa != pb {
        out.add(format!("path {} → {}", code(pa), code(pb)), true);
    } else if base(&a.url) != base(&b.url) {
        out.add("base URL changed".into(), false);
    }
    if a.server != b.server {
        let id = |s: &Option<String>| s.as_deref().map_or_else(|| "none".into(), code);
        out.add(
            format!("server {} → {}", id(&a.server), id(&b.server)),
            false,
        );
    }
    let params = (active_names(&a.params), active_names(&b.params));
    named(&mut out, "param", &params.0, &params.1, Case::Sensitive);
    headers(&mut out, a, b);
    let (x, y) = (auth(a.auth.as_ref()), auth(b.auth.as_ref()));
    if x != y {
        out.add(format!("auth {x} → {y}"), true);
    }
    body(&mut out, &a.body, &b.body);
    examples(&mut out, &a.examples, &b.examples);
    out.0
}

/// The URL before its path: the server or base URL variable.
fn base(url: &str) -> &str {
    let path = api_path(url);
    url.split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim()
        .strip_suffix(path)
        .unwrap_or_default()
}

/// How names are compared.
#[derive(Clone, Copy)]
enum Case {
    Sensitive,
    /// Header names.
    Ignored,
}

/// Names added and removed; one of each is a rename.
fn named(out: &mut Details, what: &str, a: &[&str], b: &[&str], case: Case) {
    let same = |x: &str, y: &str| match case {
        Case::Sensitive => x == y,
        Case::Ignored => x.eq_ignore_ascii_case(y),
    };
    let missing = |n: &&str, other: &[&str]| !other.iter().any(|o| same(n, o));
    let removed: Vec<&str> = a.iter().copied().filter(|n| missing(n, b)).collect();
    let added: Vec<&str> = b.iter().copied().filter(|n| missing(n, a)).collect();
    if let ([old], [new]) = (removed.as_slice(), added.as_slice()) {
        out.add(
            format!("{what} {} renamed to {}", code(old), code(new)),
            true,
        );
        return;
    }
    for n in removed {
        out.add(format!("{what} {} removed", code(n)), true);
    }
    for n in added {
        out.add(format!("{what} {} added", code(n)), false);
    }
}

/// The trimmed names of the active rows.
fn active_names(rows: &[KeyValue]) -> Vec<&str> {
    rows.iter()
        .filter(|k| k.is_active())
        .map(|k| k.name.trim())
        .collect()
}

fn part_names(parts: &[FormPart]) -> Vec<&str> {
    parts
        .iter()
        .filter(|p| p.is_active())
        .map(|p| p.name.trim())
        .collect()
}

/// Headers by name in either case; a changed value is named, never shown (P6).
fn headers(out: &mut Details, a: &Request, b: &Request) {
    let (x, y) = (active_names(&a.headers), active_names(&b.headers));
    named(out, "header", &x, &y, Case::Ignored);
    for name in x {
        let value = |r: &Request| r.header(name).map(str::to_owned);
        if y.iter().any(|n| n.eq_ignore_ascii_case(name)) && value(a) != value(b) {
            out.add(format!("header {} changed", code(name)), false);
        }
    }
}

/// The auth as the summary names it; values are never shown (P6).
fn auth(auth: Option<&Auth>) -> String {
    match auth {
        None => "inherited".into(),
        Some(Auth::None) => "none".into(),
        Some(Auth::Basic { .. }) => "Basic".into(),
        Some(Auth::Bearer { .. }) => "Bearer".into(),
        Some(Auth::Header { name, .. }) => format!("header {}", code(name.trim())),
    }
}

fn body_kind(body: &RequestBody) -> &'static str {
    match body {
        RequestBody::None => "none",
        RequestBody::Text(_) => "text",
        RequestBody::Json(_) => "JSON",
        RequestBody::Xml(_) => "XML",
        RequestBody::Form(_) => "form",
        RequestBody::Multipart(_) => "multipart",
    }
}

fn body(out: &mut Details, a: &RequestBody, b: &RequestBody) {
    match (a, b) {
        (RequestBody::None, RequestBody::None) => {}
        (RequestBody::None, _) => out.add(format!("{} body added", body_kind(b)), false),
        (_, RequestBody::None) => out.add(format!("{} body removed", body_kind(a)), true),
        (RequestBody::Json(x), RequestBody::Json(y)) => text_body(out, "body", x, y),
        (RequestBody::Text(x), RequestBody::Text(y))
        | (RequestBody::Xml(x), RequestBody::Xml(y)) => {
            if x != y {
                out.add("body changed".into(), false);
            }
        }
        (RequestBody::Form(x), RequestBody::Form(y)) => {
            named(
                out,
                "form field",
                &active_names(x),
                &active_names(y),
                Case::Sensitive,
            );
        }
        (RequestBody::Multipart(x), RequestBody::Multipart(y)) => {
            named(out, "part", &part_names(x), &part_names(y), Case::Sensitive);
        }
        _ => out.add(format!("body {} → {}", body_kind(a), body_kind(b)), true),
    }
}

/// A body that may be JSON: its shape, else whether its text changed. `what` names it
/// (body, example `ok`).
fn text_body(out: &mut Details, what: &str, a: &str, b: &str) {
    if a == b {
        return;
    }
    match (json_value(a), json_value(b)) {
        (Some(x), Some(y)) => {
            for c in json_shape_changes(&x, &y) {
                shape_detail(out, what, &c);
            }
        }
        _ => out.add(format!("{what} changed"), false),
    }
}

fn shape_detail(out: &mut Details, what: &str, c: &Change) {
    let at = code(&c.at);
    match (&c.before, &c.after) {
        (Some(_), None) => out.add(format!("{what}: {at} removed"), true),
        (None, Some(t)) => out.add(format!("{what}: {at} added ({t})"), false),
        (Some(x), Some(y)) => out.add(format!("{what}: {at} {x} → {y}"), true),
        (None, None) => {}
    }
}

/// `text` as JSON, also when a `{{variable}}` stands for a value (`{"id": {{id}}}`).
fn json_value(text: &str) -> Option<serde_json::Value> {
    if text.trim().is_empty() {
        return None;
    }
    serde_json::from_str(text)
        .ok()
        .or_else(|| serde_json::from_str(&bare_variables_as_null(text)).ok())
}

/// `text` with each `{{name}}` outside a JSON string replaced by `null`.
fn bare_variables_as_null(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut in_string, mut escaped) = (false, false);
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if !in_string
            && rest.starts_with("{{")
            && let Some(after) = rest.find("}}").and_then(|end| rest.get(end + 2..))
        {
            out.push_str("null");
            rest = after;
            continue;
        }
        out.push(c);
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                (false, _) => {}
            }
        } else if c == '"' {
            in_string = true;
        }
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    out
}

/// Examples by name; one removed and one added is a rename.
fn examples(out: &mut Details, a: &[Example], b: &[Example]) {
    let missing = |e: &&Example, other: &[Example]| !other.iter().any(|o| o.name == e.name);
    let removed: Vec<&Example> = a.iter().filter(|e| missing(e, b)).collect();
    let added: Vec<&Example> = b.iter().filter(|e| missing(e, a)).collect();
    let mut pairs: Vec<(&Example, &Example)> = a
        .iter()
        .filter_map(|x| b.iter().find(|y| y.name == x.name).map(|y| (x, y)))
        .collect();
    if let ([old], [new]) = (removed.as_slice(), added.as_slice()) {
        out.add(
            format!("example {} renamed to {}", code(&old.name), code(&new.name)),
            false,
        );
        pairs.push((old, new));
    } else {
        for e in removed {
            out.add(format!("example {} removed", code(&e.name)), true);
        }
        for e in added {
            let name = code(&e.name);
            out.add(format!("example {name} added ({})", e.status), false);
        }
    }
    for (x, y) in pairs {
        let what = format!("example {}", code(&y.name));
        if x.status != y.status {
            out.add(format!("{what}: status {} → {}", x.status, y.status), true);
        }
        let (cx, cy) = (content_type(x), content_type(y));
        if cx != cy {
            let name = |c: Option<String>| c.map_or_else(|| "none".into(), |c| code(&c));
            out.add(
                format!("{what}: Content-Type {} → {}", name(cx), name(cy)),
                true,
            );
        }
        text_body(out, &what, &x.body, &y.body);
    }
}

/// An example's media type, lower case, without parameters.
fn content_type(e: &Example) -> Option<String> {
    e.headers
        .iter()
        .find(|h| h.is_active() && h.is_named("content-type"))
        .and_then(|h| h.value.split(';').next())
        .map(|t| t.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests;
