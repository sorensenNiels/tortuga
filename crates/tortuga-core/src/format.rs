//! On-disk TOML format of collections (R-GIT-1..8). Pure text in, text out.
//!
//! Writing is deterministic (R-GIT-3): fixed key order, defaults left out,
//! maps sorted by key. Unknown keys are ignored when reading.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mock::Example;
use crate::model::{Auth, FormPart, HttpMethod, KeyValue, Request, RequestBody, Scripts};
use crate::settings::RequestSettings;

/// Newest format version this build reads (R-GIT-7). Files are written as format 1
/// unless they use something older builds would drop when they save the file: then
/// those builds open the project read-only instead ([`Manifest::required_format`]).
pub const FORMAT_VERSION: u32 = 2;

/// `tortuga.toml` at the collection root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub name: String,
    /// What the project is, in Markdown (R-COL-22).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Names of secret variables (R-VAR-3). Only the names are shared; values stay
    /// on each machine. Kept sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    /// Headers every request inherits (R-COL-4). A request header with the same
    /// name, enabled or not, replaces it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Auth for requests that don't set their own (R-HTTP-5, R-COL-4). Absent = inherit
    /// the workspace's; `Auth::None` = send none, whatever the workspace sets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// Collection-level variable defaults (R-VAR-2).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    /// How the project's requests are sent, over the workspace's and app's (R-SET-1).
    #[serde(default, skip_serializing_if = "RequestSettings::is_empty")]
    pub settings: RequestSettings,
}

impl Manifest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: 1,
            name: name.into(),
            description: String::new(),
            secrets: Vec::new(),
            servers: Vec::new(),
            headers: Vec::new(),
            auth: None,
            variables: BTreeMap::new(),
            settings: RequestSettings::default(),
        }
    }

    /// The format version this manifest is written as: 2 with settings (R-SET-1), so
    /// builds that don't know them open the project read-only instead of dropping
    /// them; otherwise 1.
    pub fn required_format(&self) -> u32 {
        if self.settings.is_empty() { 1 } else { 2 }
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
    /// The colour of its dot in the environment picker (R-SRV-8); `None`: the default.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "known_color"
    )]
    pub color: Option<EnvironmentColor>,
    /// Ask before anything but GET, HEAD or OPTIONS is sent to it, and before a workflow
    /// runs against it (R-SRV-9), e.g. for production. Shared, so the team decides once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protected: bool,
    /// Server id → base URL.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

impl Environment {
    /// Whether this environment is the one called `name` across a workspace (R-WS-9).
    pub fn is_named(&self, name: &str) -> bool {
        names_match(&self.name, name)
    }
}

impl Server {
    /// Whether this is the server a workspace environment calls `name` (R-WS-9).
    pub fn is_named(&self, name: &str) -> bool {
        names_match(&self.name, name)
    }
}

/// The form of an environment or server name that projects and the workspace are matched
/// by (R-WS-9, D62): case and surrounding spaces don't count, so "staging" in one project
/// is "Staging" in another.
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether two environment or server names mean the same one (R-WS-9, D62).
pub fn names_match(a: &str, b: &str) -> bool {
    name_key(a) == name_key(b)
}

/// The colours an environment can be marked with (R-SRV-8), so production can be red.
/// Named, not RGB, so each theme draws them in shades that read on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentColor {
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Grey,
}

impl EnvironmentColor {
    pub const ALL: [Self; 9] = [
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Grey,
    ];

    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Teal => "Teal",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Grey => "Grey",
        }
    }
}

/// A colour this build doesn't know (from a newer one) reads as the default rather than
/// making the environment unreadable.
fn known_color<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<EnvironmentColor>, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(name.and_then(|n| {
        EnvironmentColor::deserialize(
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&n),
        )
        .ok()
    }))
}

/// `_folder.toml`: display name and position of a folder, and the defaults its
/// requests inherit (R-COL-11).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    /// What the folder holds, in Markdown (R-COL-22).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Headers the folder's requests inherit; a header with the same name in a
    /// subfolder or the request, enabled or not, replaces it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Auth for the folder's requests that don't set their own. Absent = inherit;
    /// `Auth::None` = send none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// Variable defaults for the folder's requests.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

/// What a folder's requests inherit from it (R-COL-11), and its description (R-COL-22),
/// which is shown with the folder, not inherited.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderDefaults {
    pub description: String,
    pub headers: Vec<KeyValue>,
    /// `None` = inherit.
    pub auth: Option<Auth>,
    pub variables: BTreeMap<String, String>,
}

impl FolderDefaults {
    /// Nothing for its requests to inherit (the description isn't inherited).
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty() && self.auth.is_none() && self.variables.is_empty()
    }
}

impl FolderMeta {
    /// A folder file with this name, position and defaults.
    pub fn new(name: String, seq: Option<i64>, defaults: &FolderDefaults) -> Self {
        Self {
            name: Some(name),
            seq,
            description: defaults.description.clone(),
            headers: defaults.headers.clone(),
            auth: defaults.auth.clone(),
            variables: defaults.variables.clone(),
        }
    }

    pub fn defaults(&self) -> FolderDefaults {
        FolderDefaults {
            description: self.description.clone(),
            headers: self.headers.clone(),
            auth: self.auth.clone(),
            variables: self.variables.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RequestFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    /// R-COL-22
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default)]
    method: HttpMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    /// Written only when off (R-CK-2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cookies: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    path_vars: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<KeyValue>,
    /// Absent = inherit the collection's auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyFile>,
    /// Variable name → where to take it from in the response (R-TEST-4).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    capture: BTreeMap<String, String>,
    /// Pre-request and test scripts (R-TEST-1, R-TEST-2).
    #[serde(default, skip_serializing_if = "Scripts::is_empty")]
    scripts: Scripts,
    /// Example responses for the mock server (R-MOCK-1, D44).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    examples: Vec<Example>,
}

#[derive(Serialize, Deserialize)]
struct BodyFile {
    kind: BodyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<KeyValue>,
    /// Parts of a multipart body (R-HTTP-15).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parts: Vec<FormPart>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BodyKind {
    Text,
    Json,
    /// R-HTTP-19.
    Xml,
    Form,
    Multipart,
}

/// Reads a request file. `stem` names the request when the file has no `name`.
/// Returns the request and its position.
pub fn parse_request(text: &str, stem: &str) -> Result<(Request, Option<i64>), String> {
    let file: RequestFile = from_toml(text)?;
    let mut request = Request::new(file.method, &file.url);
    // A query string typed into `url` by hand ends up in `params` first.
    request.params.extend(file.params);
    request.name = file.name.unwrap_or_else(|| stem.to_owned());
    request.description = file.description;
    request.server = file.server.filter(|s| !s.is_empty());
    request.headers = file.headers;
    request.timeout_ms = file.timeout_ms;
    request.cookies = file.cookies.unwrap_or(true);
    request.auth = file.auth;
    // Values saved for the URL's path variables; stale ones are dropped (R-HTTP-10).
    request.path_vars = file.path_vars;
    request.sync_path_variables();
    request.capture = file.capture;
    request.scripts = file.scripts;
    request.examples = file.examples;
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
            kind: BodyKind::Xml,
            content,
            ..
        }) => RequestBody::Xml(content.unwrap_or_default()),
        Some(BodyFile {
            kind: BodyKind::Form,
            fields,
            ..
        }) => RequestBody::Form(fields),
        Some(BodyFile {
            kind: BodyKind::Multipart,
            parts,
            ..
        }) => RequestBody::Multipart(parts),
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
            parts: vec![],
        }),
        RequestBody::Json(t) => Some(BodyFile {
            kind: BodyKind::Json,
            content: Some(t.clone()),
            fields: vec![],
            parts: vec![],
        }),
        RequestBody::Xml(t) => Some(BodyFile {
            kind: BodyKind::Xml,
            content: Some(t.clone()),
            fields: vec![],
            parts: vec![],
        }),
        RequestBody::Form(f) => Some(BodyFile {
            kind: BodyKind::Form,
            content: None,
            fields: f.clone(),
            parts: vec![],
        }),
        RequestBody::Multipart(p) => Some(BodyFile {
            kind: BodyKind::Multipart,
            content: None,
            fields: vec![],
            parts: p.clone(),
        }),
    };
    to_toml(&RequestFile {
        name: Some(request.name.clone()),
        seq,
        description: request.description.clone(),
        method: request.method,
        server: request.server.clone(),
        url: request.url.clone(),
        timeout_ms: request.timeout_ms,
        cookies: (!request.cookies).then_some(false),
        params: request.params.clone(),
        // Only variables that have a value are worth writing.
        path_vars: request
            .path_vars
            .iter()
            .filter(|v| !v.value.is_empty())
            .cloned()
            .collect(),
        headers: request.headers.clone(),
        auth: request.auth.clone(),
        body,
        capture: request.capture.clone(),
        scripts: request.scripts.clone(),
        examples: request.examples.clone(),
    })
}

pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    from_toml(text)
}

pub fn write_manifest(manifest: &Manifest) -> String {
    let manifest = Manifest {
        format: manifest.required_format(),
        ..manifest.clone()
    };
    to_toml(&manifest)
}

pub fn parse_folder(text: &str) -> Result<FolderMeta, String> {
    from_toml(text)
}

pub fn write_folder(meta: &FolderMeta) -> String {
    to_toml(meta)
}

/// Reads a workflow file (R-FLOW-1); `id` is its file stem.
pub fn parse_workflow(text: &str, id: &str) -> Result<crate::workflow::Workflow, String> {
    let mut workflow: crate::workflow::Workflow = from_toml(text)?;
    id.clone_into(&mut workflow.id);
    if workflow.name.trim().is_empty() {
        id.clone_into(&mut workflow.name);
    }
    Ok(workflow)
}

pub fn write_workflow(workflow: &crate::workflow::Workflow) -> String {
    to_toml(workflow)
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

    /// R-COL-11, R-GIT-3: folder files with defaults are readable, stable and round-trip.
    #[test]
    fn folder_file_with_defaults() {
        let meta = FolderMeta::new(
            "Admin".into(),
            Some(3),
            &FolderDefaults {
                headers: vec![KeyValue::new("X-Area", "admin")],
                auth: Some(Auth::Bearer {
                    token: "{{admin_token}}".into(),
                }),
                variables: [("tenant".to_owned(), "acme".to_owned())].into(),
                ..FolderDefaults::default()
            },
        );
        let text = write_folder(&meta);
        assert_eq!(
            text,
            r#"name = "Admin"
seq = 3

[[headers]]
name = "X-Area"
value = "admin"

[auth]
kind = "bearer"
token = "{{admin_token}}"

[variables]
tenant = "acme"
"#
        );
        assert_eq!(parse_folder(&text).expect("parse"), meta);
        assert_eq!(
            write_folder(&FolderMeta::new(
                "Plain".into(),
                None,
                &FolderDefaults::default()
            )),
            "name = \"Plain\"\n",
            "no defaults: nothing extra written"
        );
    }

    /// R-TEST-1, R-TEST-2, R-TEST-4: scripts and captures in the request file.
    #[test]
    fn scripts_and_captures_in_the_request_file() {
        let mut r = Request::new(HttpMethod::Post, "/login");
        r.name = "Log in".into();
        r.capture.insert("token".into(), "$.access_token".into());
        r.capture.insert("trace".into(), "header:X-Trace".into());
        r.scripts.pre_request = "vars.set(\"now\", Date.now());\n".into();
        r.scripts.tests = "test(\"ok\", () => expect(res.status).toBe(200));\n".into();
        let text = write_request(&r, Some(1));
        assert_eq!(
            text,
            r#"name = "Log in"
seq = 1
method = "POST"
url = "/login"

[capture]
token = "$.access_token"
trace = "header:X-Trace"

[scripts]
pre_request = """
vars.set("now", Date.now());
"""
tests = """
test("ok", () => expect(res.status).toBe(200));
"""
"#
        );
        let (back, _) = parse_request(&text, "x").expect("parse");
        assert_eq!(back, r);
    }

    /// R-COL-22: descriptions are Markdown under the name, as readable multi-line
    /// strings in the request, folder and project files, and left out when empty.
    #[test]
    fn descriptions_in_the_files() {
        let description = "# Log in\n\nGets a **token** for `ü😀`.\n\n- one\n- two \"quoted\"\n";
        let mut r = Request::new(HttpMethod::Post, "/login");
        r.name = "Log in".into();
        r.description = description.into();
        let text = write_request(&r, Some(1));
        assert_eq!(
            text,
            r#"name = "Log in"
seq = 1
description = """
# Log in

Gets a **token** for `ü😀`.

- one
- two "quoted"
"""
method = "POST"
url = "/login"
"#
        );
        let (back, _) = parse_request(&text, "x").expect("parse");
        assert_eq!(back, r);
        // One line, and one without a final newline, round-trip too.
        for one in ["Just a line.", "Ends without a newline\nsecond"] {
            r.description = one.into();
            let (back, _) = parse_request(&write_request(&r, None), "x").expect("parse");
            assert_eq!(back.description, one);
        }
        r.description.clear();
        assert!(!write_request(&r, None).contains("description"));

        let mut manifest = Manifest::new("Shop");
        manifest.description = description.into();
        let text = write_manifest(&manifest);
        assert!(text.starts_with("format = 1\nname = \"Shop\"\ndescription = \"\"\"\n# Log in"));
        assert_eq!(parse_manifest(&text).expect("manifest"), manifest);

        let defaults = FolderDefaults {
            description: description.into(),
            ..FolderDefaults::default()
        };
        let meta = FolderMeta::new("Auth".into(), Some(2), &defaults);
        let back = parse_folder(&write_folder(&meta)).expect("folder");
        assert_eq!(back.defaults(), defaults);
        assert!(defaults.is_empty(), "nothing to inherit");
    }

    /// R-WS-8: a capture into the workspace is a quoted key and round-trips.
    #[test]
    fn workspace_capture_in_the_request_file() {
        let mut r = Request::new(HttpMethod::Post, "/login");
        r.capture
            .insert("workspace.token".into(), "$.access_token".into());
        let text = write_request(&r, None);
        assert!(
            text.contains("[capture]\n\"workspace.token\" = \"$.access_token\"\n"),
            "{text}"
        );
        let (back, _) = parse_request(&text, "x").expect("parse");
        assert_eq!(back, r);
    }

    /// R-HTTP-15, R-GIT-3: multipart bodies are readable, stable and round-trip.
    #[test]
    fn multipart_body_file() {
        let mut r = Request::new(HttpMethod::Post, "/upload");
        r.name = "Upload".into();
        let mut avatar = FormPart::file("avatar", "fixtures/me.png");
        avatar.content_type = Some("image/png".into());
        r.body = RequestBody::Multipart(vec![FormPart::text("title", "{{name}}"), avatar]);
        let text = write_request(&r, None);
        assert_eq!(
            text,
            r#"name = "Upload"
method = "POST"
url = "/upload"

[body]
kind = "multipart"

[[body.parts]]
name = "title"
value = "{{name}}"

[[body.parts]]
name = "avatar"
value = "fixtures/me.png"
file = true
content_type = "image/png"
"#
        );
        let (back, _) = parse_request(&text, "x").expect("parse");
        assert_eq!(back, r);
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

    /// R-HTTP-19.
    #[test]
    fn xml_body_is_written_as_kind_xml_and_round_trips() {
        let mut r = Request::new(HttpMethod::Post, "http://a.test");
        r.body = RequestBody::Xml("<order>\n  <item>ü😀 {{sku}}</item>\n</order>\n".into());
        let text = write_request(&r, None);
        assert!(text.contains("[body]\nkind = \"xml\"\n"), "{text}");
        let (back, _) = parse_request(&text, "x").expect("parse");
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
    fn path_variables_round_trip() {
        let mut r = Request::new(HttpMethod::Get, "/users/:id/orders/:order");
        r.path_vars[0].value = "{{user_id}}".into();
        let text = write_request(&r, None);
        assert!(
            text.ends_with("\n[[path_vars]]\nname = \"id\"\nvalue = \"{{user_id}}\"\n"),
            "only filled values are written: {text}"
        );
        assert_eq!(parse_request(&text, "x").expect("parse").0, r);
    }

    #[test]
    fn cookies_off_is_saved_in_the_request_file() {
        let mut r = Request::new(HttpMethod::Get, "https://a.test");
        assert!(
            !write_request(&r, None).contains("cookies"),
            "on is the default"
        );
        r.cookies = false;
        let text = write_request(&r, None);
        assert!(text.contains("\ncookies = false\n"), "{text}");
        assert!(!parse_request(&text, "x").expect("parse").0.cookies);
    }

    #[test]
    fn auth_is_written_only_when_set() {
        let mut r = Request::new(HttpMethod::Get, "https://a.test");
        assert!(
            !write_request(&r, None).contains("[auth]"),
            "inherit is the default"
        );

        r.auth = Some(Auth::Bearer {
            token: "{{token}}".into(),
        });
        let text = write_request(&r, None);
        assert!(
            text.ends_with("\n[auth]\nkind = \"bearer\"\ntoken = \"{{token}}\"\n"),
            "{text}"
        );
        assert_eq!(parse_request(&text, "x").expect("parse").0, r);

        // R-HTTP-22.
        r.auth = Some(Auth::Header {
            name: "X-API-Key".into(),
            value: "{{api_key}}".into(),
        });
        let text = write_request(&r, None);
        assert!(
            text.ends_with(
                "\n[auth]\nkind = \"header\"\nname = \"X-API-Key\"\nvalue = \"{{api_key}}\"\n"
            ),
            "{text}"
        );
        assert_eq!(parse_request(&text, "x").expect("parse").0, r);

        r.auth = Some(Auth::None);
        let text = write_request(&r, None);
        assert!(text.ends_with("\n[auth]\nkind = \"none\"\n"), "{text}");
        assert_eq!(
            parse_request(&text, "x").expect("parse").0.auth,
            Some(Auth::None)
        );
    }

    #[test]
    fn manifest_with_defaults_and_secrets() {
        let mut m = Manifest::new("Acme API");
        m.secrets = vec!["token".into()];
        m.headers.push(KeyValue::new("Accept", "application/json"));
        m.auth = Some(Auth::Basic {
            username: "{{user}}".into(),
            password: "{{token}}".into(),
        });
        let text = write_manifest(&m);
        assert_eq!(
            text,
            r#"format = 1
name = "Acme API"
secrets = ["token"]

[[headers]]
name = "Accept"
value = "application/json"

[auth]
kind = "basic"
username = "{{user}}"
password = "{{token}}"
"#
        );
        assert_eq!(parse_manifest(&text), Ok(m));
    }

    /// R-HTTP-5: a project's "no auth" is written, so it can stop the workspace's auth;
    /// a manifest without `[auth]` inherits.
    #[test]
    fn manifest_no_auth_round_trips() {
        let mut m = Manifest::new("Acme API");
        assert!(
            !write_manifest(&m).contains("[auth]"),
            "inherit is the default"
        );
        assert_eq!(
            parse_manifest(&write_manifest(&m)).expect("parse").auth,
            None
        );
        m.auth = Some(Auth::None);
        let text = write_manifest(&m);
        assert!(text.ends_with("\n[auth]\nkind = \"none\"\n"), "{text}");
        assert_eq!(parse_manifest(&text), Ok(m));
    }

    /// R-SET-1: a project's request settings; only then is the file format 2.
    #[test]
    fn manifest_with_request_settings() {
        let mut m = Manifest::new("Acme API");
        m.settings.timeout_ms = Some(5_000);
        m.settings.cookies = Some(false);
        let text = write_manifest(&m);
        assert_eq!(
            text,
            "format = 2\nname = \"Acme API\"\n\n[settings]\ntimeout_ms = 5000\ncookies = false\n"
        );
        m.format = 2;
        assert_eq!(parse_manifest(&text), Ok(m.clone()));
        m.settings = RequestSettings::default();
        assert!(write_manifest(&m).starts_with("format = 1\n"), "back to 1");
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
            color: None,
            protected: false,
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

    /// R-SRV-9: `protected = true` is written only when on, and files without it load.
    /// R-WS-9, D62: environment names match across projects whatever their case and
    /// surrounding spaces; anything else still counts.
    #[test]
    fn environment_names_match_ignoring_case_and_spaces() {
        assert!(names_match("Staging", " staging "));
        assert!(names_match("ÜBER 😀", "über 😀"));
        assert!(!names_match("Staging", "Stage"));
        assert!(!names_match("Dev 1", "Dev1"));
        assert!(names_match("", "  "));
        let env = Environment {
            name: "Production".into(),
            ..Environment::default()
        };
        assert!(env.is_named("PRODUCTION"));
        assert!(!env.is_named("Prod"));
    }

    #[test]
    fn protected_environments_round_trip() {
        let env = Environment {
            id: "prod".into(),
            name: "Production ü😀".into(),
            protected: true,
            ..Environment::default()
        };
        let text = write_environment(&env);
        assert!(text.contains("protected = true\n"), "{text}");
        assert_eq!(parse_environment(&text, "prod").expect("read"), env);
        let plain = write_environment(&Environment {
            protected: false,
            ..env
        });
        assert!(
            !plain.contains("protected"),
            "only written when on: {plain}"
        );
        assert!(
            !parse_environment(&plain, "prod")
                .expect("old file")
                .protected
        );
    }

    /// R-SRV-8: an environment's colour is saved by name; one a newer build added
    /// reads as the default.
    #[test]
    fn environment_colors_round_trip_and_unknown_ones_are_ignored() {
        for color in EnvironmentColor::ALL {
            let env = Environment {
                id: "prod".into(),
                name: "Production ü😀".into(),
                color: Some(color),
                ..Environment::default()
            };
            let text = write_environment(&env);
            assert_eq!(parse_environment(&text, "prod"), Ok(env), "{text}");
        }
        let env = Environment {
            name: "Production".into(),
            color: Some(EnvironmentColor::Red),
            ..Environment::default()
        };
        assert_eq!(
            write_environment(&env),
            "name = \"Production\"\ncolor = \"red\"\n"
        );
        let newer = parse_environment("name = \"P\"\ncolor = \"ultraviolet\"\n", "p")
            .expect("still readable");
        assert_eq!(newer.color, None);
        let older = parse_environment("name = \"P\"\n", "p").expect("readable");
        assert_eq!(older.color, None);
    }
}
