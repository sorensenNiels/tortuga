//! Importing an OpenAPI 3 description as a new project (R-COL-15, D7, D33, D34).
//!
//! The description (JSON or YAML) becomes:
//! - one server named after the API, with a base URL per environment: each entry of
//!   `servers` is an environment, its `{variables}` filled with their defaults;
//! - a folder per tag (an operation goes into the folder of its first tag; untagged
//!   ones stay at the top level) with a request per operation, named by its summary;
//! - path parameters as `:name` path variables, query and header parameters (required
//!   ones on, optional ones off) with their example values, and a body made from the
//!   request body's example or, failing that, its schema;
//! - security schemes as auth that uses secret variables (D35), so no credential is
//!   ever written to the project;
//! - responses as the request's examples for the mock server (R-MOCK-6).
//!
//! Only local references (`#/components/…`) are followed. What can't be imported as
//! written is reported in [`Import::warnings`]. [`crate::import::create_project`] writes
//! the result.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use yaml_rust2::{Yaml, YamlLoader};

use crate::collection::{Collection, NewItem};
use crate::format::{Environment, FolderDefaults, Manifest, Server};
use crate::import::{Import, ImportKind, short_name, unique_name, variable_name};
use crate::model::{
    Auth, FormPart, HttpMethod, KeyValue, Request, RequestBody, is_json_content_type,
};
use crate::{Error, Result};

mod responses;

/// Reads an OpenAPI 3 description (JSON or YAML) into a project (R-COL-15).
pub fn parse(text: &str) -> Result<Import> {
    let doc = read_document(text)?;
    check_version(&doc)?;
    Ok(Importer::new(&doc).run())
}

const METHODS: [(&str, HttpMethod); 7] = [
    ("get", HttpMethod::Get),
    ("post", HttpMethod::Post),
    ("put", HttpMethod::Put),
    ("patch", HttpMethod::Patch),
    ("delete", HttpMethod::Delete),
    ("head", HttpMethod::Head),
    ("options", HttpMethod::Options),
];

/// How deep example bodies are built from nested (or recursive) schemas.
const MAX_DEPTH: usize = 8;

/// Headers the request sets itself, so parameters with these names are left out.
const IGNORED_HEADERS: [&str; 3] = ["accept", "content-type", "authorization"];

static NULL: Value = Value::Null;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidOpenApi(message.into())
}

fn read_document(text: &str) -> Result<Value> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') {
        return serde_json::from_str(text)
            .map_err(|e| invalid(format!("it is not valid JSON: {e}")));
    }
    let docs = YamlLoader::load_from_str(text)
        .map_err(|e| invalid(format!("it is not valid YAML: {e}")))?;
    let first = docs
        .into_iter()
        .next()
        .ok_or_else(|| invalid("the file is empty"))?;
    Ok(yaml_to_json(&first))
}

fn yaml_to_json(yaml: &Yaml) -> Value {
    match yaml {
        Yaml::Real(text) => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| Value::String(text.clone()), Value::Number),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Array(items) => Value::Array(items.iter().map(yaml_to_json).collect()),
        Yaml::Hash(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (yaml_key(k), yaml_to_json(v)))
                .collect(),
        ),
        Yaml::Alias(_) | Yaml::Null | Yaml::BadValue => Value::Null,
    }
}

/// Map keys as text: YAML reads `200:` (a response code) as a number.
fn yaml_key(key: &Yaml) -> String {
    match key {
        Yaml::String(s) | Yaml::Real(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

fn check_version(doc: &Value) -> Result<()> {
    // An unquoted `openapi: 3.1` in YAML is a number.
    if let Some(version) = doc
        .get("openapi")
        .filter(|v| v.is_string() || v.is_number())
        .map(value_text)
    {
        return if version.starts_with("3.") {
            Ok(())
        } else {
            Err(invalid(format!(
                "OpenAPI {version} is not supported, only 3.x"
            )))
        };
    }
    if doc.get("swagger").is_some() {
        return Err(invalid(
            "Swagger 2.0 descriptions are not supported yet. Convert it to OpenAPI 3 first (the \
             Swagger Editor can), then import that",
        ));
    }
    Err(invalid(
        "this is not an OpenAPI description: it has no `openapi` version",
    ))
}

/// Auth and API keys that a security requirement asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Credentials {
    auth: Option<Auth>,
    header: Option<KeyValue>,
    query: Option<KeyValue>,
}

struct Importer<'a> {
    doc: &'a Value,
    server_id: String,
    secrets: BTreeSet<String>,
    warnings: Vec<String>,
    unresolved: BTreeSet<String>,
    /// The description's own `security`, which requests inherit.
    global_security: Option<&'a Value>,
    global: Credentials,
}

impl<'a> Importer<'a> {
    fn new(doc: &'a Value) -> Self {
        Self {
            doc,
            server_id: String::new(),
            secrets: BTreeSet::new(),
            warnings: Vec::new(),
            unresolved: BTreeSet::new(),
            global_security: None,
            global: Credentials::default(),
        }
    }

    fn run(mut self) -> Import {
        let title = self
            .doc
            .pointer("/info/title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Imported API")
            .to_owned();
        let mut manifest = Manifest::new(title.clone());
        self.server_id = Collection::new_server_id(&title, &[]);
        manifest.servers.push(Server {
            id: self.server_id.clone(),
            name: title,
        });
        let environments = self.environments();

        self.global_security = self.doc.get("security");
        if let Some(security) = self.global_security {
            self.global = self.requirement(security);
        }
        manifest.auth = self.global.auth.clone().filter(|a| !a.is_none());
        manifest.headers.extend(self.global.header.clone());

        let (items, requests) = self.operations();
        manifest.secrets = self.secrets.iter().cloned().collect();
        Import {
            kind: ImportKind::OpenApi,
            manifest,
            environments,
            items,
            requests,
            secret_values: BTreeMap::new(),
            environment_secret_values: BTreeMap::new(),
            warnings: self.warnings,
        }
    }

    fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// Follows local `$ref`s; anything else is reported once and read as nothing.
    fn resolve(&mut self, mut value: &'a Value) -> &'a Value {
        for _ in 0..32 {
            let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
                return value;
            };
            if let Some(target) = reference
                .strip_prefix('#')
                .and_then(|pointer| self.doc.pointer(pointer))
            {
                value = target;
                continue;
            }
            if self.unresolved.insert(reference.to_owned()) {
                self.warn(format!(
                    "The reference {reference} could not be followed; only references inside \
                     the file are."
                ));
            }
            return &NULL;
        }
        &NULL
    }

    // ------------------------------------------------------------------ servers

    /// One environment per entry of `servers`, each with the API's base URL.
    fn environments(&mut self) -> Vec<Environment> {
        let servers = self
            .doc
            .get("servers")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut environments: Vec<Environment> = Vec::new();
        for server in servers {
            let Some(url) = server.get("url").and_then(Value::as_str) else {
                continue;
            };
            let url = self.server_url(url, server.get("variables"));
            let base = server
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map_or_else(|| host_name(&url), str::to_owned);
            let name = unique_name(&base, |n| environments.iter().any(|e| e.name == n));
            environments.push(self.environment(name, url, environments.len()));
        }
        if environments.is_empty() {
            self.warn(
                "The description lists no servers: set the API's base URL under Environments….",
            );
            environments.push(self.environment("Default".into(), String::new(), 0));
        }
        environments
    }

    fn environment(&self, name: String, url: String, index: usize) -> Environment {
        let id = Collection::new_environment_id(&name, &[]);
        Environment {
            id,
            seq: i64::try_from(index + 1).ok().map(|n| n * 100),
            servers: [(self.server_id.clone(), url)].into(),
            name,
            ..Environment::default()
        }
    }

    /// The server URL with its `{variables}` filled in, absolute and without a
    /// trailing `/`.
    fn server_url(&mut self, url: &str, variables: Option<&'a Value>) -> String {
        let mut out = String::new();
        let mut rest = url;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else {
                break;
            };
            let name = &rest[start + 1..start + len];
            let value = variables
                .and_then(|v| v.get(name))
                .and_then(|v| v.get("default").or_else(|| v.pointer("/enum/0")))
                .map(value_text);
            out.push_str(&rest[..start]);
            if let Some(value) = value {
                out.push_str(&value);
            } else {
                self.warn(format!(
                    "The server URL {url} uses {{{name}}} without a default; edit it under \
                     Environments…."
                ));
                out.push_str(&rest[start..=start + len]);
            }
            rest = &rest[start + len + 1..];
        }
        out.push_str(rest);
        if !out.contains("://") {
            self.warn(format!(
                "The server URL {url} is relative, so it was made http://localhost{url}; edit it \
                 under Environments…."
            ));
            out = format!(
                "http://localhost{}",
                if out.starts_with('/') { "" } else { "/" }
            ) + &out;
        }
        while out.ends_with('/') && !out.ends_with("://") {
            out.pop();
        }
        out
    }

    // ----------------------------------------------------------------- security

    /// The credentials of the first alternative of a `security` list; an empty list
    /// means "no auth".
    fn requirement(&mut self, security: &'a Value) -> Credentials {
        let Some(first) = security
            .as_array()
            .and_then(|alternatives| alternatives.first())
            .and_then(Value::as_object)
        else {
            return Credentials {
                auth: Some(Auth::None),
                ..Credentials::default()
            };
        };
        let mut out = Credentials::default();
        for name in first.keys() {
            let scheme = self.scheme(name);
            out.auth = out.auth.or(scheme.auth);
            out.header = out.header.or(scheme.header);
            out.query = out.query.or(scheme.query);
        }
        out
    }

    fn scheme(&mut self, name: &str) -> Credentials {
        let Some(scheme) = self
            .doc
            .pointer("/components/securitySchemes")
            .and_then(|s| s.get(name))
        else {
            self.warn(format!(
                "The security scheme “{name}” is not defined, so it was left out."
            ));
            return Credentials::default();
        };
        let scheme = self.resolve(scheme);
        let field = |key| scheme.get(key).and_then(Value::as_str).unwrap_or_default();
        let mut out = Credentials::default();
        match field("type") {
            "http" => match field("scheme").to_ascii_lowercase().as_str() {
                "bearer" => {
                    out.auth = Some(Auth::Bearer {
                        token: self.secret("token"),
                    });
                }
                "basic" => {
                    out.auth = Some(Auth::Basic {
                        username: self.secret("username"),
                        password: self.secret("password"),
                    });
                }
                other => self.warn(format!(
                    "HTTP auth scheme “{other}” ({name}) is not supported, so it was left out."
                )),
            },
            "apiKey" => {
                let key = field("name");
                let value = self.secret(&variable_name(key));
                match field("in") {
                    "header" => out.header = Some(KeyValue::new(key, value)),
                    "query" => out.query = Some(KeyValue::new(key, value)),
                    "cookie" => {
                        out.header = Some(KeyValue::new("Cookie", format!("{key}={value}")));
                    }
                    _ => {}
                }
            }
            "oauth2" | "openIdConnect" => {
                out.auth = Some(Auth::Bearer {
                    token: self.secret("access_token"),
                });
                self.warn(format!(
                    "“{name}” uses OAuth 2 or OpenID Connect: requests send the secret \
                     {{{{access_token}}}} as a bearer token. Get a token and set it under \
                     Environments…."
                ));
            }
            other => self.warn(format!(
                "Security scheme type “{other}” ({name}) is not supported, so it was left out."
            )),
        }
        out
    }

    /// `{{name}}`, with `name` listed as a secret variable (D35).
    fn secret(&mut self, name: &str) -> String {
        self.secrets.insert(name.to_owned());
        format!("{{{{{name}}}}}")
    }

    // --------------------------------------------------------------- operations

    /// Folders per tag (declared tags first, in their order), then untagged requests.
    fn operations(&mut self) -> (Vec<NewItem>, usize) {
        let mut folders: Vec<(String, Vec<NewItem>)> = self
            .doc
            .get("tags")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .map(|name| (name.to_owned(), Vec::new()))
            .collect();
        let mut top = Vec::new();
        let mut count = 0;
        let paths = self
            .doc
            .get("paths")
            .and_then(Value::as_object)
            .into_iter()
            .flatten();
        for (path, item) in paths {
            let item = self.resolve(item);
            let shared = item
                .get("parameters")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            for (key, method) in METHODS {
                let Some(op) = item.get(key) else {
                    continue;
                };
                let request = NewItem::Request(Box::new(self.request(method, path, op, shared)));
                count += 1;
                match op.pointer("/tags/0").and_then(Value::as_str) {
                    None => top.push(request),
                    Some(tag) => match folders.iter_mut().find(|(name, _)| name == tag) {
                        Some((_, items)) => items.push(request),
                        None => folders.push((tag.to_owned(), vec![request])),
                    },
                }
            }
            if item.get("trace").is_some() {
                self.warn(format!(
                    "TRACE {path} was left out: Tortuga does not send TRACE requests."
                ));
            }
        }
        let mut items: Vec<NewItem> = folders
            .into_iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(name, items)| NewItem::Folder {
                name,
                defaults: FolderDefaults::default(),
                items,
            })
            .collect();
        items.extend(top);
        (items, count)
    }

    fn request(
        &mut self,
        method: HttpMethod,
        path: &str,
        op: &'a Value,
        shared: &'a [Value],
    ) -> Request {
        let mut request = Request::new(method, "");
        request.name = request_name(op, method, path);
        request.server = Some(self.server_id.clone());
        request.url = path_template(path);

        let mut path_values = Vec::new();
        let mut cookies = Vec::new();
        for param in self.parameters(op, shared) {
            let name = param
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let required = param
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let value = self.parameter_example(param);
            match param.get("in").and_then(Value::as_str) {
                Some("path") => path_values.push((variable_name(name), value)),
                Some("query") => request.params.push(KeyValue {
                    enabled: required,
                    ..KeyValue::new(name, value)
                }),
                Some("header")
                    if !IGNORED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) =>
                {
                    request.headers.push(KeyValue {
                        enabled: required,
                        ..KeyValue::new(name, value)
                    });
                }
                Some("cookie") => cookies.push(format!("{name}={value}")),
                _ => {}
            }
        }
        if !cookies.is_empty() {
            request
                .headers
                .push(KeyValue::new("Cookie", cookies.join("; ")));
        }
        request.sync_path_variables();
        for var in &mut request.path_vars {
            if let Some((_, value)) = path_values.iter().find(|(name, _)| *name == var.name) {
                var.value.clone_from(value);
            }
        }

        let (body, content_type) = self.body(op);
        request.body = body;
        request.headers.extend(content_type);
        self.apply_security(&mut request, op.get("security"));
        request.examples = self.examples(op);
        request
    }

    /// The path item's parameters with the operation's own replacing those with the
    /// same name and location.
    fn parameters(&mut self, op: &'a Value, shared: &'a [Value]) -> Vec<&'a Value> {
        let own = op
            .get("parameters")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut out: Vec<&'a Value> = Vec::new();
        for param in shared.iter().chain(own) {
            let param = self.resolve(param);
            let key = |p: &Value| (p.get("name").cloned(), p.get("in").cloned());
            out.retain(|p| key(p) != key(param));
            out.push(param);
        }
        out
    }

    fn parameter_example(&mut self, param: &'a Value) -> String {
        if let Some(example) = param.get("example") {
            return value_text(example);
        }
        if let Some(example) = self.first_example(param.get("examples")) {
            return value_text(&example);
        }
        let Some(schema) = param.get("schema") else {
            return String::new();
        };
        let schema = self.resolve(schema);
        ["example", "default"]
            .iter()
            .find_map(|k| schema.get(*k))
            .or_else(|| schema.pointer("/enum/0"))
            .map(value_text)
            .unwrap_or_default()
    }

    /// The value of the first entry of an `examples` map.
    fn first_example(&mut self, examples: Option<&'a Value>) -> Option<Value> {
        let first = examples?.as_object()?.values().next()?;
        self.resolve(first).get("value").cloned()
    }

    /// Project auth applies unless the operation asks for something else: `[]` turns
    /// auth off (and any project-wide API key header), other schemes are set on the
    /// request.
    fn apply_security(&mut self, request: &mut Request, security: Option<&'a Value>) {
        let own = match security {
            Some(security) if Some(security) != self.global_security => self.requirement(security),
            _ => {
                request.params.extend(self.global.query.clone());
                return;
            }
        };
        request.auth = Some(own.auth.unwrap_or_default());
        if let Some(global) = &self.global.header
            && own.header.as_ref().is_none_or(|h| h.name != global.name)
        {
            // A request header with the same name, enabled or not, replaces the project's.
            request.headers.push(KeyValue {
                enabled: false,
                ..global.clone()
            });
        }
        request.headers.extend(own.header);
        request.params.extend(own.query);
    }

    // --------------------------------------------------------------------- body

    /// The body for the request body's preferred media type: JSON, then form, then
    /// multipart, then whatever comes first. Other types than those also get their
    /// `Content-Type` header.
    fn body(&mut self, op: &'a Value) -> (RequestBody, Option<KeyValue>) {
        let Some(body) = op.get("requestBody") else {
            return (RequestBody::None, None);
        };
        let body = self.resolve(body);
        let Some(content) = body.get("content").and_then(Value::as_object) else {
            return (RequestBody::None, None);
        };
        let pick = |want: fn(&str) -> bool| {
            content
                .iter()
                .find(|(ct, _)| want(&ct.to_ascii_lowercase()))
        };
        let Some((content_type, media)) = pick(is_json_content_type)
            .or_else(|| pick(|ct| ct.starts_with("application/x-www-form-urlencoded")))
            .or_else(|| pick(|ct| ct.starts_with("multipart/form-data")))
            .or_else(|| content.iter().next())
        else {
            return (RequestBody::None, None);
        };
        let lower = content_type.to_ascii_lowercase();
        let example = self.media_example(media);
        if is_json_content_type(&lower) {
            let text = serde_json::to_string_pretty(&example).unwrap_or_default();
            let header = (lower != "application/json")
                .then(|| KeyValue::new("Content-Type", content_type.clone()));
            return (RequestBody::Json(text), header);
        }
        let fields = self.form_fields(media, &example);
        if lower.starts_with("application/x-www-form-urlencoded") {
            let fields = fields
                .into_iter()
                .map(|(name, value, _)| KeyValue::new(name, value))
                .collect();
            return (RequestBody::Form(fields), None);
        }
        if lower.starts_with("multipart/form-data") {
            let parts = fields
                .into_iter()
                .map(|(name, value, file)| FormPart {
                    name,
                    value: if file { String::new() } else { value },
                    file,
                    content_type: None,
                    enabled: true,
                })
                .collect();
            return (RequestBody::Multipart(parts), None);
        }
        let text = match &example {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        (
            RequestBody::Text(text),
            Some(KeyValue::new("Content-Type", content_type.clone())),
        )
    }

    fn media_example(&mut self, media: &'a Value) -> Value {
        if let Some(example) = media.get("example") {
            return example.clone();
        }
        if let Some(example) = self.first_example(media.get("examples")) {
            return example;
        }
        media
            .get("schema")
            .map_or(Value::Null, |schema| self.schema_example(schema, 0))
    }

    /// Form fields from the schema's properties: (name, example value, is a file).
    fn form_fields(&mut self, media: &'a Value, example: &Value) -> Vec<(String, String, bool)> {
        let schema = media.get("schema").map_or(&NULL, |s| self.resolve(s));
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        properties
            .iter()
            .map(|(name, property)| {
                let property = self.resolve(property);
                let format = property.get("format").and_then(Value::as_str);
                let file = matches!(format, Some("binary" | "base64"))
                    || property.get("contentMediaType").is_some();
                let value = example.get(name).map(value_text).unwrap_or_default();
                (name.clone(), value, file)
            })
            .collect()
    }

    /// An example value for a schema: its own example, default or first enum value,
    /// else one built from its type (objects with every property).
    fn schema_example(&mut self, schema: &'a Value, depth: usize) -> Value {
        let schema = self.resolve(schema);
        if depth > MAX_DEPTH {
            return Value::Null;
        }
        if let Some(found) = ["example", "default", "const"]
            .iter()
            .find_map(|k| schema.get(*k))
            .or_else(|| schema.pointer("/examples/0"))
            .or_else(|| schema.pointer("/enum/0"))
        {
            return found.clone();
        }
        if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            for part in parts {
                match self.schema_example(part, depth + 1) {
                    Value::Object(fields) => merged.extend(fields),
                    Value::Null => {}
                    other if merged.is_empty() => return other,
                    _ => {}
                }
            }
            return Value::Object(merged);
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(first) = schema.pointer(&format!("/{key}/0")) {
                return self.schema_example(first, depth + 1);
            }
        }
        match schema_type(schema) {
            "object" => {
                let mut fields = Map::new();
                let properties = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten();
                for (name, property) in properties {
                    fields.insert(name.clone(), self.schema_example(property, depth + 1));
                }
                Value::Object(fields)
            }
            "array" => match schema
                .get("items")
                .map(|i| self.schema_example(i, depth + 1))
            {
                Some(Value::Null) | None => Value::Array(Vec::new()),
                Some(item) => Value::Array(vec![item]),
            },
            "string" => Value::String(
                string_example(schema.get("format").and_then(Value::as_str)).to_owned(),
            ),
            "integer" | "number" => Value::from(0),
            "boolean" => Value::Bool(true),
            _ => Value::Null,
        }
    }
}

/// The schema's type, also from 3.1's `["string", "null"]` or from its shape.
fn schema_type(schema: &Value) -> &str {
    match schema.get("type") {
        Some(Value::String(t)) => t,
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or_default(),
        _ if schema.get("properties").is_some() => "object",
        _ if schema.get("items").is_some() => "array",
        _ => "",
    }
}

fn string_example(format: Option<&str>) -> &'static str {
    match format {
        Some("date-time") => "2024-01-01T00:00:00Z",
        Some("date") => "2024-01-01",
        Some("email") => "user@example.com",
        Some("uuid") => "00000000-0000-0000-0000-000000000000",
        Some("uri" | "url") => "https://example.com",
        Some("binary" | "byte" | "password") => "",
        _ => "string",
    }
}

/// A JSON value as the text of a parameter or form field.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(value_text).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    }
}

/// The operation's summary, else its id, else "GET /path", cut to a usable length.
fn request_name(op: &Value, method: HttpMethod, path: &str) -> String {
    let name = ["summary", "operationId"]
        .iter()
        .filter_map(|k| op.get(*k).and_then(Value::as_str))
        .map(|s| s.trim().trim_end_matches('.'))
        .find(|s| !s.is_empty())
        .map_or_else(|| format!("{method} {path}"), str::to_owned);
    short_name(&name)
}

/// `/users/{user-id}` → `/users/:user_id` (R-HTTP-10).
fn path_template(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push(':');
        out.push_str(&variable_name(&rest[start + 1..start + len]));
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

/// "api.example.com" or "localhost:8080/v1" for an environment named after its URL.
fn host_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            let host = u.host_str()?.to_owned();
            let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
            let path = u.path().trim_end_matches('/');
            Some(format!("{host}{port}{path}"))
        })
        .unwrap_or_else(|| "Default".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PETSTORE: &str = r#"
openapi: 3.0.3
info:
  title: Pet Store
  version: 1.0.0
servers:
  - url: https://{env}.pets.test/v1/
    description: Production
    variables:
      env:
        default: api
  - url: http://localhost:8080/v1
tags:
  - name: pets
  - name: owners
security:
  - bearer: []
paths:
  /pets:
    get:
      tags: [pets]
      summary: List pets.
      parameters:
        - name: limit
          in: query
          schema: { type: integer, default: 20 }
        - name: kind
          in: query
          required: true
          example: cat
        - name: Accept
          in: header
          schema: { type: string }
    post:
      tags: [pets]
      operationId: createPet
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/Pet' }
  /pets/{pet-id}:
    parameters:
      - name: pet-id
        in: path
        required: true
        schema: { type: string, example: "42" }
    get:
      tags: [pets]
      summary: Get a pet
    delete:
      tags: [pets]
      summary: Delete a pet
      security: []
  /owners/{id}/photo:
    put:
      tags: [owners]
      summary: Upload a photo
      security:
        - apiKey: []
      parameters:
        - name: id
          in: path
          required: true
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                caption: { type: string, example: Me }
                photo: { type: string, format: binary }
  /health:
    get:
      responses:
        200:
          description: OK
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
    apiKey: { type: apiKey, in: header, name: X-API-Key }
  schemas:
    Pet:
      type: object
      required: [name]
      properties:
        id: { type: integer, readOnly: true }
        name: { type: string, example: Rex }
        born: { type: string, format: date }
        tags: { type: array, items: { type: string } }
        owner: { $ref: '#/components/schemas/Owner' }
    Owner:
      allOf:
        - { $ref: '#/components/schemas/Named' }
        - type: object
          properties:
            pets: { type: array, items: { $ref: '#/components/schemas/Pet' } }
    Named:
      type: object
      properties:
        name: { type: string }
"#;

    fn requests(items: &[NewItem]) -> Vec<&Request> {
        let mut out = Vec::new();
        for item in items {
            match item {
                NewItem::Request(r) => out.push(r.as_ref()),
                NewItem::Folder { items, .. } => out.extend(requests(items)),
            }
        }
        out
    }

    fn find<'a>(import: &'a Import, name: &str) -> &'a Request {
        requests(&import.items)
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    /// R-COL-15: servers become environments of one server, tags become folders.
    #[test]
    fn servers_tags_and_operations() {
        let import = parse(PETSTORE).expect("import");
        assert_eq!(import.manifest.name, "Pet Store");
        assert_eq!(import.manifest.servers.len(), 1);
        let server = &import.manifest.servers[0].id;
        assert_eq!(server, "pet-store");
        let envs: Vec<(&str, &str)> = import
            .environments
            .iter()
            .map(|e| (e.name.as_str(), e.servers[server].as_str()))
            .collect();
        assert_eq!(
            envs,
            [
                ("Production", "https://api.pets.test/v1"),
                ("localhost:8080/v1", "http://localhost:8080/v1"),
            ]
        );
        assert_eq!(import.environments[1].seq, Some(200));

        let folders: Vec<(&str, usize)> = import
            .items
            .iter()
            .map(|i| match i {
                NewItem::Folder { name, items, .. } => (name.as_str(), items.len()),
                NewItem::Request(r) => (r.name.as_str(), 0),
            })
            .collect();
        assert_eq!(folders, [("pets", 4), ("owners", 1), ("GET /health", 0)]);
        assert_eq!(import.requests, 6);
        assert_eq!(import.warnings, Vec::<String>::new());
    }

    /// R-COL-15: parameters, path variables and names.
    #[test]
    fn parameters_and_names() {
        let import = parse(PETSTORE).expect("import");
        let list = find(&import, "List pets");
        assert_eq!(list.method, HttpMethod::Get);
        assert_eq!(list.url, "/pets");
        assert_eq!(list.server.as_deref(), Some("pet-store"));
        let params: Vec<(&str, &str, bool)> = list
            .params
            .iter()
            .map(|p| (p.name.as_str(), p.value.as_str(), p.enabled))
            .collect();
        assert_eq!(params, [("limit", "20", false), ("kind", "cat", true)]);
        assert_eq!(list.headers, [], "Accept is left to the client");

        let get = find(&import, "Get a pet");
        assert_eq!(get.url, "/pets/:pet_id");
        assert_eq!(get.path_vars, [KeyValue::new("pet_id", "42")]);
        assert_eq!(find(&import, "createPet").method, HttpMethod::Post);
    }

    /// R-COL-15: bodies come from schemas, following `$ref`, `allOf` and recursion.
    #[test]
    fn bodies_from_schemas() {
        let import = parse(PETSTORE).expect("import");
        let RequestBody::Json(text) = &find(&import, "createPet").body else {
            panic!("JSON body");
        };
        let body: Value = serde_json::from_str(text).expect("json");
        assert_eq!(body["name"], "Rex");
        assert_eq!(body["id"], 0);
        assert_eq!(body["born"], "2024-01-01");
        assert_eq!(body["tags"], serde_json::json!(["string"]));
        assert_eq!(body["owner"]["name"], "string");
        assert!(body["owner"]["pets"].is_array(), "recursion stops: {body}");

        let upload = find(&import, "Upload a photo");
        let RequestBody::Multipart(parts) = &upload.body else {
            panic!("multipart");
        };
        let parts: Vec<(&str, &str, bool)> = parts
            .iter()
            .map(|p| (p.name.as_str(), p.value.as_str(), p.file))
            .collect();
        assert_eq!(parts, [("caption", "Me", false), ("photo", "", true)]);
    }

    /// R-COL-15, D35: security schemes use secret variables; operations can opt out
    /// or use another scheme.
    #[test]
    fn security_uses_secret_variables() {
        let import = parse(PETSTORE).expect("import");
        assert_eq!(
            import.manifest.auth,
            Some(Auth::Bearer {
                token: "{{token}}".into()
            })
        );
        assert_eq!(import.manifest.secrets, ["token", "x_api_key"]);
        assert_eq!(find(&import, "Get a pet").auth, None, "inherits");
        assert_eq!(find(&import, "Delete a pet").auth, Some(Auth::None));
        let upload = find(&import, "Upload a photo");
        assert_eq!(upload.auth, Some(Auth::None));
        assert_eq!(
            upload.headers,
            [KeyValue::new("X-API-Key", "{{x_api_key}}")]
        );
    }

    /// R-COL-15: an API key in the query is added to every request that uses it, and
    /// a project-wide header key is switched off where an operation needs no auth.
    #[test]
    fn api_keys_in_query_and_header() {
        let query = parse(
            r#"{"openapi": "3.1.0", "info": {"title": "Q"},
                "servers": [{"url": "https://q.test"}],
                "security": [{"key": []}],
                "components": {"securitySchemes": {"key": {"type": "apiKey", "in": "query", "name": "api_key"}}},
                "paths": {"/a": {"get": {}}}}"#,
        )
        .expect("import");
        let a = find(&query, "GET /a");
        assert_eq!(a.params, [KeyValue::new("api_key", "{{api_key}}")]);
        assert_eq!(query.manifest.auth, None);

        let header = parse(
            r#"{"openapi": "3.0.0", "info": {"title": "H"},
                "security": [{"key": []}],
                "components": {"securitySchemes": {"key": {"type": "apiKey", "in": "header", "name": "X-Key"}}},
                "paths": {"/open": {"get": {"security": []}}}}"#,
        )
        .expect("import");
        assert_eq!(
            header.manifest.headers,
            [KeyValue::new("X-Key", "{{x_key}}")]
        );
        let open = find(&header, "GET /open");
        assert_eq!(
            open.headers,
            [KeyValue {
                enabled: false,
                ..KeyValue::new("X-Key", "{{x_key}}")
            }]
        );
    }

    /// R-COL-15, D33: only OpenAPI 3; problems are explained.
    #[test]
    fn versions_and_errors() {
        let message = |text: &str| parse(text).expect_err("error").to_string();
        assert!(message(r#"{"swagger": "2.0"}"#).contains("Swagger 2.0"));
        assert!(message("openapi: 2.5\n").contains("2.5"));
        parse("openapi: 3.1\ninfo: { title: T }\n").expect("an unquoted version is a number");
        assert!(message("name: x\n").contains("no `openapi` version"));
        assert!(message("{ not json").contains("not valid JSON"));
        assert!(message("a: [b").contains("not valid YAML"));
        assert!(message("").contains("empty"));
    }

    /// R-COL-15: what can't be imported as written is reported.
    #[test]
    fn warnings() {
        let import = parse(
            r"
openapi: 3.0.0
info: { title: W }
servers:
  - url: /api
  - url: https://{region}.w.test
security:
  - oauth: []
  - missing: []
paths:
  /a:
    get:
      requestBody:
        content:
          application/json:
            schema: { $ref: 'other.yaml#/Thing' }
    trace: {}
components:
  securitySchemes:
    oauth: { type: oauth2, flows: {} }
",
        )
        .expect("import");
        assert_eq!(import.environments[0].servers["w"], "http://localhost/api");
        assert_eq!(
            import.environments[1].servers["w"],
            "https://{region}.w.test"
        );
        assert_eq!(
            import.manifest.auth,
            Some(Auth::Bearer {
                token: "{{access_token}}".into()
            })
        );
        let text = import.warnings.join("\n");
        for part in [
            "/api is relative",
            "{region} without a default",
            "OAuth 2",
            "other.yaml#/Thing could not be followed",
            "TRACE /a",
        ] {
            assert!(text.contains(part), "{part} missing in:\n{text}");
        }
        assert_eq!(import.warnings.len(), 5);
    }

    #[test]
    fn helpers() {
        assert_eq!(path_template("/a/{b}/c/{d-e}.json"), "/a/:b/c/:d_e.json");
        assert_eq!(value_text(&serde_json::json!([1, "b"])), "1,b");
        let long = serde_json::json!({ "summary": "x".repeat(80) });
        assert_eq!(
            request_name(&long, HttpMethod::Get, "/").chars().count(),
            60
        );
        assert!(is_json_content_type(
            "application/problem+json; charset=utf-8"
        ));
        assert!(!is_json_content_type("application/xml"));
    }
}
