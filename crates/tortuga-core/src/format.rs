//! On-disk TOML format of collections (R-GIT-1..8). Pure text in, text out.
//!
//! Writing is deterministic (R-GIT-3): fixed key order, defaults left out,
//! maps sorted by key. Unknown keys are ignored when reading.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{HttpMethod, KeyValue, Request, RequestBody};

/// Format version written by this build (R-GIT-7).
pub const FORMAT_VERSION: u32 = 1;

/// `tortuga.toml` at the collection root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    /// Collection-level variable defaults (R-VAR-2).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

impl Manifest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: FORMAT_VERSION,
            name: name.into(),
            servers: Vec::new(),
            variables: BTreeMap::new(),
        }
    }
}

/// A named server (R-SRV-1). `id` is what requests refer to and never changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
}

/// One file in `environments/` (R-VAR-2, R-SRV-2). `id` is the file stem.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Environment {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    /// Server id → base URL.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

/// `_folder.toml`: display name and position of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct RequestFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    #[serde(default)]
    method: HttpMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyFile>,
}

#[derive(Serialize, Deserialize)]
struct BodyFile {
    kind: BodyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<KeyValue>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BodyKind {
    Text,
    Json,
    Form,
}

/// Reads a request file. `stem` names the request when the file has no `name`.
/// Returns the request and its position.
pub fn parse_request(text: &str, stem: &str) -> Result<(Request, Option<i64>), String> {
    let file: RequestFile = from_toml(text)?;
    let mut request = Request::new(file.method, &file.url);
    // A query string typed into `url` by hand ends up in `params` first.
    request.params.extend(file.params);
    request.name = file.name.unwrap_or_else(|| stem.to_owned());
    request.server = file.server.filter(|s| !s.is_empty());
    request.headers = file.headers;
    request.timeout_ms = file.timeout_ms;
    request.body = match file.body {
        None => RequestBody::None,
        Some(BodyFile {
            kind: BodyKind::Text,
            content,
            ..
        }) => RequestBody::Text(content.unwrap_or_default()),
        Some(BodyFile {
            kind: BodyKind::Json,
            content,
            ..
        }) => RequestBody::Json(content.unwrap_or_default()),
        Some(BodyFile {
            kind: BodyKind::Form,
            fields,
            ..
        }) => RequestBody::Form(fields),
    };
    Ok((request, file.seq))
}

pub fn write_request(request: &Request, seq: Option<i64>) -> String {
    let body = match &request.body {
        RequestBody::None => None,
        RequestBody::Text(t) => Some(BodyFile {
            kind: BodyKind::Text,
            content: Some(t.clone()),
            fields: vec![],
        }),
        RequestBody::Json(t) => Some(BodyFile {
            kind: BodyKind::Json,
            content: Some(t.clone()),
            fields: vec![],
        }),
        RequestBody::Form(f) => Some(BodyFile {
            kind: BodyKind::Form,
            content: None,
            fields: f.clone(),
        }),
    };
    to_toml(&RequestFile {
        name: Some(request.name.clone()),
        seq,
        method: request.method,
        server: request.server.clone(),
        url: request.url.clone(),
        timeout_ms: request.timeout_ms,
        params: request.params.clone(),
        headers: request.headers.clone(),
        body,
    })
}

pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    from_toml(text)
}

pub fn write_manifest(manifest: &Manifest) -> String {
    to_toml(manifest)
}

pub fn parse_folder(text: &str) -> Result<FolderMeta, String> {
    from_toml(text)
}

pub fn write_folder(meta: &FolderMeta) -> String {
    to_toml(meta)
}

pub fn parse_environment(text: &str, id: &str) -> Result<Environment, String> {
    let mut env: Environment = from_toml(text)?;
    id.clone_into(&mut env.id);
    if env.name.trim().is_empty() {
        id.clone_into(&mut env.name);
    }
    Ok(env)
}

pub fn write_environment(env: &Environment) -> String {
    to_toml(env)
}

fn from_toml<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|e| {
        let line = e
            .span()
            .map(|span| text[..span.start.min(text.len())].lines().count().max(1));
        match line {
            Some(line) => format!("line {line}: {}", e.message()),
            None => e.message().to_owned(),
        }
    })
}

fn to_toml<T: Serialize>(value: &T) -> String {
    toml::to_string(value).expect("collection types always serialize to TOML")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Request {
        let mut r = Request::new(HttpMethod::Post, "/users?page=2");
        r.name = "Create user".into();
        r.server = Some("users".into());
        r.headers.push(KeyValue::new("Accept", "application/json"));
        r.headers.push(KeyValue {
            enabled: false,
            ..KeyValue::new("X-Debug", "1")
        });
        r.body = RequestBody::Json("{\n  \"name\": \"it's {{user}}\"\n}\n".into());
        r.timeout_ms = Some(5000);
        r
    }

    #[test]
    fn request_file_is_readable_and_stable() {
        let text = write_request(&sample(), Some(2));
        assert_eq!(
            text,
            r#"name = "Create user"
seq = 2
method = "POST"
server = "users"
url = "/users"
timeout_ms = 5000

[[params]]
name = "page"
value = "2"

[[headers]]
name = "Accept"
value = "application/json"

[[headers]]
name = "X-Debug"
value = "1"
enabled = false

[body]
kind = "json"
content = """
{
  "name": "it's {{user}}"
}
"""
"#
        );
    }

    #[test]
    fn request_round_trips() {
        let original = sample();
        let (back, seq) = parse_request(&write_request(&original, Some(2)), "x").expect("parse");
        assert_eq!(seq, Some(2));
        assert_eq!(back, original);
    }

    #[test]
    fn form_body_round_trips() {
        let mut r = Request::new(HttpMethod::Post, "http://a.test");
        r.body = RequestBody::Form(vec![KeyValue::new("a", "1")]);
        let (back, _) = parse_request(&write_request(&r, None), "x").expect("parse");
        assert_eq!(back.body, r.body);
    }

    #[test]
    fn minimal_hand_written_file() {
        let (r, seq) =
            parse_request("url = \"https://a.test/x?q=1\"\n", "my-request").expect("parse");
        assert_eq!(
            (r.name.as_str(), r.method, seq),
            ("my-request", HttpMethod::Get, None)
        );
        assert_eq!(r.url, "https://a.test/x");
        assert_eq!(r.params, [KeyValue::new("q", "1")]);
    }

    #[test]
    fn defaults_are_not_written() {
        let r = Request::new(HttpMethod::Get, "https://a.test");
        assert_eq!(
            write_request(&r, None),
            "name = \"Untitled request\"\nmethod = \"GET\"\nurl = \"https://a.test\"\n"
        );
    }

    #[test]
    fn errors_name_the_line() {
        let err = parse_request("name = \"x\"\nmethod = \"FETCH\"\n", "x").expect_err("bad method");
        assert!(err.starts_with("line 2:"), "{err}");
        let err = parse_request("<<<<<<< HEAD\nname = \"x\"\n", "x").expect_err("conflict marker");
        assert!(err.starts_with("line 1:"), "{err}");
    }

    #[test]
    fn manifest_and_environment_round_trip() {
        let mut m = Manifest::new("Acme API");
        m.servers.push(Server {
            id: "users".into(),
            name: "Users API".into(),
        });
        m.variables.insert("v".into(), "2".into());
        let text = write_manifest(&m);
        assert_eq!(
            text,
            "format = 1\nname = \"Acme API\"\n\n[[servers]]\nid = \"users\"\nname = \"Users API\"\n\n[variables]\nv = \"2\"\n"
        );
        assert_eq!(parse_manifest(&text), Ok(m));

        let env = Environment {
            id: "dev".into(),
            name: "Development".into(),
            seq: Some(1),
            servers: BTreeMap::from([("users".into(), "http://localhost:8080".into())]),
            variables: BTreeMap::from([("b".into(), "2".into()), ("a".into(), "1".into())]),
        };
        let text = write_environment(&env);
        assert_eq!(
            text,
            "name = \"Development\"\nseq = 1\n\n[servers]\nusers = \"http://localhost:8080\"\n\n[variables]\na = \"1\"\nb = \"2\"\n"
        );
        assert_eq!(parse_environment(&text, "dev"), Ok(env));
    }
}
