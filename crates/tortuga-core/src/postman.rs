//! Importing a Postman collection (v2.0 or v2.1 JSON) as a new project (R-COL-16, D7,
//! D34, D36).
//!
//! The collection becomes:
//! - its folders and requests, in the same order and nesting;
//! - collection variables as project variables, folder variables as folder defaults
//!   (R-COL-11);
//! - a variable that starts request URLs (`{{baseUrl}}/users`) as a server, with the
//!   variable's value as its base URL in a "Default" environment (D36);
//! - the auth of the collection, its folders and requests as project, folder and
//!   request auth. Credentials become secret variables (D35): values written in the
//!   collection go to [`Import::secret_values`], for this machine's secret store, and
//!   never into the project;
//! - scripts translated to Tortuga's script API (R-COL-19, [`script`]); one that uses
//!   more of Postman's `pm` API than the translation knows is kept as comments to port
//!   by hand (D28, D36).
//!
//! What can't be imported as written is reported in [`Import::warnings`].
//! [`crate::import::create_project`] writes the result.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::collection::NewItem;
use crate::error::sentence;
use crate::format::{FolderDefaults, Manifest};
use crate::import::{Collected, Import, ImportKind, short_name, variable_name};
use crate::model::{
    Auth, FormPart, HttpMethod, KeyValue, Request, RequestBody, Scripts, is_json_content_type,
};
use crate::{Error, Result};

mod environment;
mod script;

pub(crate) use environment::looks_like_environment;
pub use environment::{
    EnvironmentImport, PostmanEnvironment, import_environments, parse_environment,
};

/// Reads a Postman collection (v2.0 or v2.1 JSON) into a project (R-COL-16).
pub fn parse(text: &str) -> Result<Import> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let doc: Value =
        serde_json::from_str(text).map_err(|e| invalid(format!("it is not valid JSON: {e}")))?;
    let doc = unwrap(&doc);
    check_version(doc)?;
    Ok(Importer::new(doc).run())
}

/// Whether `doc` is a Postman collection.
pub(crate) fn looks_like_collection(doc: &Value) -> bool {
    let doc = unwrap(doc);
    doc.pointer("/info/schema")
        .and_then(Value::as_str)
        .is_some_and(|s| s.contains("getpostman.com"))
        || (doc.get("info").is_some() && doc.get("item").is_some_and(Value::is_array))
}

/// Postman's API returns the collection wrapped in `{"collection": …}`.
fn unwrap(doc: &Value) -> &Value {
    match doc.get("collection") {
        Some(inner) if inner.get("info").is_some() => inner,
        _ => doc,
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidPostman(message.into())
}

/// The error for a Postman environment file given where a collection was expected.
pub(crate) fn environment_file_error() -> Error {
    invalid(
        "this is a Postman environment, not a collection. Add it with “Add environment \
         files…” while importing its collection, or with “Import Postman environments…” in \
         a project's menu",
    )
}

fn check_version(doc: &Value) -> Result<()> {
    if looks_like_environment(doc) {
        return Err(environment_file_error());
    }
    let schema = doc
        .pointer("/info/schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema.contains("/v1.") || (doc.get("requests").is_some() && doc.get("item").is_none()) {
        return Err(invalid(
            "collections in the old v1 format are not supported. Export it again from Postman \
             as Collection v2.1, then import that",
        ));
    }
    if doc.get("info").is_none() || !doc.get("item").is_some_and(Value::is_array) {
        return Err(invalid(
            "this is not a Postman collection: it has no `info` and `item`",
        ));
    }
    Ok(())
}

/// Auth and an API key that one level (collection, folder or request) sets.
#[derive(Debug, Clone, Default)]
struct Credentials {
    /// `None` = inherit.
    auth: Option<Auth>,
    header: Option<KeyValue>,
    query: Option<KeyValue>,
}

/// What a request gets from the levels above it that Tortuga's inheritance doesn't
/// carry by itself.
#[derive(Debug, Clone, Default)]
struct Inherited {
    /// API keys the collection or a folder sends as query parameters.
    query: Vec<KeyValue>,
    /// API key headers of the nearest level with auth, which own auth turns off.
    headers: Vec<KeyValue>,
    /// The project or a folder has auth that Tortuga passes down.
    auth: bool,
    /// A folder turned that auth off, which Tortuga's folders can't (R-COL-11).
    no_auth: bool,
}

struct Importer<'a> {
    doc: &'a Value,
    c: Collected,
}

impl<'a> Importer<'a> {
    fn new(doc: &'a Value) -> Self {
        Self {
            doc,
            c: Collected::default(),
        }
    }

    fn run(mut self) -> Import {
        let name = self
            .doc
            .pointer("/info/name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Imported collection")
            .to_owned();
        let mut manifest = Manifest::new(name);
        manifest.description = description(self.doc.pointer("/info/description"));
        self.c.variables = self.variables_of(self.doc.get("variable"));
        self.level_scripts(self.doc.get("event"), "the collection");
        let creds = self.credentials(self.doc.get("auth"), "the collection");
        let auth = creds.auth.unwrap_or_default();
        manifest.headers.extend(creds.header.clone());
        let top = Inherited {
            query: creds.query.into_iter().collect(),
            headers: creds.header.into_iter().collect(),
            auth: !auth.is_none(),
            no_auth: false,
        };
        manifest.auth = Some(auth).filter(|a| !a.is_none());
        let items = self.items(self.doc.get("item"), &top);
        self.c.finish(ImportKind::Postman, manifest, items)
    }

    /// Enabled variables of a `variable` list; those of type `secret` become secret
    /// variables.
    fn variables_of(&mut self, list: Option<&Value>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for var in list.and_then(Value::as_array).into_iter().flatten() {
            if is_disabled(var) {
                continue;
            }
            let Some(key) = ["key", "id"]
                .iter()
                .find_map(|k| var.get(*k).and_then(Value::as_str))
                .filter(|k| !k.is_empty())
            else {
                continue;
            };
            if var.get("type").and_then(Value::as_str) == Some("secret") {
                self.c.secrets.insert(key.to_owned());
            }
            out.insert(key.to_owned(), text(var.get("value")));
        }
        out
    }

    /// Scripts on the collection or a folder, which Tortuga doesn't have.
    fn level_scripts(&mut self, events: Option<&Value>, place: &str) {
        let has_code = events
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|e| !is_disabled(e) && !script_code(e.get("script")).trim().is_empty());
        if has_code {
            self.c.warn(format!(
                "The scripts of {place} were left out: Tortuga runs scripts per request."
            ));
        }
    }

    // ------------------------------------------------------------------ items

    fn items(&mut self, list: Option<&Value>, inherited: &Inherited) -> Vec<NewItem> {
        let mut out = Vec::new();
        for item in list.and_then(Value::as_array).into_iter().flatten() {
            if let Some(children) = item.get("item") {
                out.push(self.folder(item, children, inherited));
            } else if item.get("request").is_some()
                && let Some(request) = self.request(item, inherited)
            {
                out.push(NewItem::Request(Box::new(request)));
            }
        }
        out
    }

    fn folder(&mut self, item: &Value, children: &Value, inherited: &Inherited) -> NewItem {
        let name = item_name(item).unwrap_or("Folder").to_owned();
        let place = format!("folder “{name}”");
        self.level_scripts(item.get("event"), &place);
        let creds = self.credentials(item.get("auth"), &place);
        let mut defaults = FolderDefaults {
            description: description(item.get("description")),
            variables: self.variables_of(item.get("variable")),
            ..FolderDefaults::default()
        };
        let below = match creds.auth {
            None => inherited.clone(),
            Some(auth) => {
                defaults.headers = turned_off(&inherited.headers, creds.header.as_ref());
                defaults.headers.extend(creds.header.clone());
                let below = Inherited {
                    query: creds.query.into_iter().collect(),
                    headers: creds.header.into_iter().collect(),
                    auth: inherited.auth || !auth.is_none(),
                    no_auth: auth.is_none(),
                };
                // "No auth" reaches the requests through `no_auth`.
                defaults.auth = Some(auth).filter(|a| !a.is_none());
                below
            }
        };
        let items = self.items(Some(children), &below);
        NewItem::Folder {
            name,
            defaults,
            items,
        }
    }

    fn request(&mut self, item: &Value, inherited: &Inherited) -> Option<Request> {
        let req = item.get("request")?;
        // v2 also allows a request that is just its URL.
        let (method, url) = match req {
            Value::String(_) => ("GET", Some(req)),
            _ => (
                req.get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .trim(),
                req.get("url"),
            ),
        };
        let raw = url.map(raw_url).unwrap_or_default();
        let name =
            short_name(&item_name(item).map_or_else(|| format!("{method} {raw}"), str::to_owned));
        let Ok(method) = method.parse::<HttpMethod>() else {
            self.c.warn(format!(
                "“{name}” was left out: Tortuga does not send {} requests.",
                method.to_ascii_uppercase()
            ));
            return None;
        };

        let mut request = Request::new(method, &raw);
        request.name = name;
        // The request's own description, else the item's (R-COL-22).
        request.description = [req.get("description"), item.get("description")]
            .into_iter()
            .map(description)
            .find(|d| !d.is_empty())
            .unwrap_or_default();
        if let Some(query) = url.and_then(|u| u.get("query")).and_then(Value::as_array) {
            request.params = query.iter().filter_map(key_value).collect();
        }
        if let Some((server, rest)) = self.c.server_for(&request.url) {
            request.server = Some(server);
            request.url = rest;
        }
        request.sync_path_variables();
        let values = url
            .and_then(|u| u.get("variable"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for var in &mut request.path_vars {
            if let Some(value) = values.iter().find(|v| key(v) == Some(var.name.as_str())) {
                var.value = text(value.get("value"));
            }
        }

        request.headers = headers(req.get("header"));
        let (body, content_type) = self.body(req.get("body"), &request.name);
        request.body = match body {
            RequestBody::Text(text)
                if request
                    .header("content-type")
                    .is_some_and(is_json_content_type) =>
            {
                RequestBody::Json(text)
            }
            body => body,
        };
        if let Some(content_type) = content_type
            && !request.has_header("content-type")
        {
            request.headers.push(content_type);
        }
        self.apply_auth(&mut request, req.get("auth"), inherited);
        request.scripts = self.request_scripts(item.get("event"));
        Some(request)
    }

    // ------------------------------------------------------------------- auth

    /// A request's own auth, or what it gets from above when it inherits.
    fn apply_auth(&mut self, request: &mut Request, auth: Option<&Value>, inherited: &Inherited) {
        let own = self.credentials(auth, &format!("“{}”", request.name));
        let Some(auth) = own.auth else {
            if inherited.no_auth && inherited.auth {
                request.auth = Some(Auth::None);
            }
            request.params.extend(inherited.query.iter().cloned());
            return;
        };
        request.auth = Some(auth);
        request
            .headers
            .extend(turned_off(&inherited.headers, own.header.as_ref()));
        request.headers.extend(own.header);
        request.params.extend(own.query);
    }

    /// The credentials of an `auth` object; `None` (or `null`) means inherit.
    fn credentials(&mut self, auth: Option<&Value>, place: &str) -> Credentials {
        let Some(auth) = auth.filter(|a| a.is_object()) else {
            return Credentials::default();
        };
        let kind = auth.get("type").and_then(Value::as_str).unwrap_or("noauth");
        let params = auth_params(auth, kind);
        let param = |key: &str| params.get(key).cloned().unwrap_or_default();
        let mut out = Credentials::default();
        match kind {
            "inherit" => {}
            "noauth" => out.auth = Some(Auth::None),
            "bearer" => {
                out.auth = Some(Auth::Bearer {
                    token: self.c.credential(&param("token"), "token"),
                });
            }
            "basic" => {
                out.auth = Some(Auth::Basic {
                    username: self.c.credential(&param("username"), "username"),
                    password: self.c.credential(&param("password"), "password"),
                });
            }
            "apikey" => {
                let key = Some(param("key"))
                    .filter(|k| !k.trim().is_empty())
                    .unwrap_or_else(|| "X-API-Key".into());
                let value = self.c.credential(&param("value"), &variable_name(&key));
                let key_value = KeyValue::new(key.trim(), value);
                if param("in") == "query" {
                    out.query = Some(key_value);
                } else {
                    out.header = Some(key_value);
                }
                out.auth = Some(Auth::None);
            }
            "oauth2" => {
                out.auth = Some(Auth::Bearer {
                    token: self.c.credential(&param("accessToken"), "access_token"),
                });
                self.c.warn(format!(
                    "{} uses OAuth 2, which became Bearer {{{{access_token}}}}: get a token and \
                     put it in access_token under Environments….",
                    sentence(place)
                ));
            }
            other => {
                out.auth = Some(Auth::None);
                self.c.warn(format!(
                    "{} uses {other} auth, which Tortuga does not support, so it was left out.",
                    sentence(place)
                ));
            }
        }
        out
    }

    // ------------------------------------------------------------------- body

    /// The body, and a `Content-Type` for raw HTML and JavaScript.
    fn body(&mut self, body: Option<&Value>, request: &str) -> (RequestBody, Option<KeyValue>) {
        let Some(body) = body.filter(|b| b.is_object() && !is_disabled(b)) else {
            return (RequestBody::None, None);
        };
        let entries = |key: &str| {
            body.get(key)
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
        };
        match body.get("mode").and_then(Value::as_str).unwrap_or_default() {
            "raw" => {
                let raw = text(body.get("raw"));
                let content_type = match body
                    .pointer("/options/raw/language")
                    .and_then(Value::as_str)
                {
                    Some("json") => return (RequestBody::Json(raw), None),
                    Some("xml") => return (RequestBody::Xml(raw), None),
                    Some("html") => Some("text/html"),
                    Some("javascript") => Some("application/javascript"),
                    _ => None,
                };
                (
                    RequestBody::Text(raw),
                    content_type.map(|ct| KeyValue::new("Content-Type", ct)),
                )
            }
            "urlencoded" => (
                RequestBody::Form(entries("urlencoded").iter().filter_map(key_value).collect()),
                None,
            ),
            "formdata" => (
                RequestBody::Multipart(self.form_parts(entries("formdata"), request)),
                None,
            ),
            "graphql" => {
                let graphql = body.get("graphql").unwrap_or(&Value::Null);
                let variables = match graphql.get("variables") {
                    Some(Value::String(s)) if !s.trim().is_empty() => {
                        serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
                    }
                    Some(v @ Value::Object(_)) => v.clone(),
                    _ => Value::Object(serde_json::Map::new()),
                };
                let json = serde_json::json!({
                    "query": text(graphql.get("query")),
                    "variables": variables,
                });
                (
                    RequestBody::Json(serde_json::to_string_pretty(&json).unwrap_or_default()),
                    None,
                )
            }
            "file" => {
                self.c.warn(format!(
                    "The body of “{request}” is a file, which Tortuga can't send as a whole \
                     body yet, so it was left out."
                ));
                (RequestBody::None, None)
            }
            "" => (RequestBody::None, None),
            other => {
                self.c.warn(format!(
                    "The {other} body of “{request}” is not supported, so it was left out."
                ));
                (RequestBody::None, None)
            }
        }
    }

    fn form_parts(&mut self, entries: &[Value], request: &str) -> Vec<FormPart> {
        let mut files = false;
        let parts = entries
            .iter()
            .filter_map(|entry| {
                let name = key(entry)?.to_owned();
                let file = entry.get("type").and_then(Value::as_str) == Some("file");
                files |= file;
                let value = if file {
                    match entry.get("src") {
                        Some(Value::Array(paths)) => paths.first().map(|p| text(Some(p))),
                        src => Some(text(src)),
                    }
                    .unwrap_or_default()
                } else {
                    text(entry.get("value"))
                };
                Some(FormPart {
                    name,
                    value,
                    file,
                    content_type: entry
                        .get("contentType")
                        .and_then(Value::as_str)
                        .filter(|ct| !ct.trim().is_empty())
                        .map(str::to_owned),
                    enabled: !is_disabled(entry),
                })
            })
            .collect();
        if files {
            self.c.warn(format!(
                "“{request}” uploads files: check their paths, which are from the computer the \
                 collection was exported on."
            ));
        }
        parts
    }

    // ---------------------------------------------------------------- scripts

    fn request_scripts(&mut self, events: Option<&Value>) -> Scripts {
        let mut scripts = Scripts::default();
        for event in events.and_then(Value::as_array).into_iter().flatten() {
            let code = script_code(event.get("script"));
            if is_disabled(event) || code.trim().is_empty() {
                continue;
            }
            let slot = match event.get("listen").and_then(Value::as_str) {
                Some("prerequest") => &mut scripts.pre_request,
                Some("test") => &mut scripts.tests,
                _ => continue,
            };
            *slot = match script::translate(&code) {
                script::Translation::Complete(code) => {
                    self.c.translated += 1;
                    format!("{}{}\n", script::NOTE, code.trim_end())
                }
                script::Translation::Partial(code) => {
                    self.c.commented_script(&code, ImportKind::Postman)
                }
            };
        }
        scripts
    }
}

/// Disabled copies of `headers`, which turn the inherited ones off (R-COL-11), except
/// one with the name of `own`, which replaces it anyway.
fn turned_off(headers: &[KeyValue], own: Option<&KeyValue>) -> Vec<KeyValue> {
    headers
        .iter()
        .filter(|h| own.is_none_or(|o| !o.is_named(&h.name)))
        .map(|h| KeyValue {
            enabled: false,
            ..h.clone()
        })
        .collect()
}

/// The parameters of one auth type: a list of `{key, value}` in v2.1, an object in v2.0.
fn auth_params(auth: &Value, kind: &str) -> BTreeMap<String, String> {
    match auth.get(kind) {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|p| Some((key(p)?.to_owned(), text(p.get("value")))))
            .collect(),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), text(Some(v))))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// The URL as text: a string, the `raw` of a URL object, or put together from its parts.
fn raw_url(url: &Value) -> String {
    if let Some(raw) = url
        .as_str()
        .or_else(|| url.get("raw").and_then(Value::as_str))
    {
        return raw.trim().to_owned();
    }
    let joined = |key: &str, separator: &str| match url.get(key) {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|p| {
                p.as_str()
                    .map_or_else(|| text(p.get("value")), str::to_owned)
            })
            .collect::<Vec<_>>()
            .join(separator),
        other => text(other),
    };
    let mut out = String::new();
    if let Some(protocol) = url.get("protocol").and_then(Value::as_str) {
        out.push_str(protocol);
        out.push_str("://");
    }
    out.push_str(&joined("host", "."));
    if let Some(port) = url.get("port").filter(|p| !p.is_null()) {
        out.push(':');
        out.push_str(&text(Some(port)));
    }
    let path = joined("path", "/");
    if !path.is_empty() {
        out.push('/');
        out.push_str(path.trim_start_matches('/'));
    }
    out
}

fn headers(headers: Option<&Value>) -> Vec<KeyValue> {
    match headers {
        Some(Value::Array(list)) => list.iter().filter_map(key_value).collect(),
        // v2 also allows headers as one text, a `Name: value` per line.
        Some(Value::String(text)) => text
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, value)| KeyValue::new(name.trim(), value.trim()))
            .collect(),
        _ => Vec::new(),
    }
}

/// A `{key, value, disabled}` entry; ones without a key are left out.
fn key_value(entry: &Value) -> Option<KeyValue> {
    let name = key(entry)?;
    Some(KeyValue {
        enabled: !is_disabled(entry),
        ..KeyValue::new(name, text(entry.get("value")))
    })
}

fn key(entry: &Value) -> Option<&str> {
    entry
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|k| !k.is_empty())
}

fn item_name(item: &Value) -> Option<&str> {
    item.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
}

/// A description (R-COL-22): text, or `{"content": …, "type": "text/markdown"}`.
fn description(value: Option<&Value>) -> String {
    let text = match value {
        Some(Value::Object(o)) => o.get("content").and_then(Value::as_str),
        Some(v) => v.as_str(),
        None => None,
    };
    text.map(str::trim).unwrap_or_default().to_owned()
}

fn is_disabled(value: &Value) -> bool {
    value.get("disabled").and_then(Value::as_bool) == Some(true)
}

/// A JSON value as text; `null` and missing are empty.
fn text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// A script's `exec`: a list of lines or one text.
fn script_code(script: Option<&Value>) -> String {
    match script.and_then(|s| s.get("exec")) {
        Some(Value::Array(lines)) => lines
            .iter()
            .map(|l| text(Some(l)))
            .collect::<Vec<_>>()
            .join("\n"),
        other => text(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;
    use crate::import::requests;

    /// A v2.1 export with what teams typically have in a collection.
    const COLLECTION: &str = r#"{
  "info": {
    "_postman_id": "8b0c",
    "name": "Shop API",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "auth": {
    "type": "bearer",
    "bearer": [{ "key": "token", "value": "abc123", "type": "string" }]
  },
  "variable": [
    { "key": "baseUrl", "value": "https://shop.test/api/" },
    { "key": "userId", "value": "42" },
    { "key": "apiSecret", "value": "s3cret", "type": "secret" },
    { "key": "off", "value": "x", "disabled": true }
  ],
  "event": [{ "listen": "prerequest", "script": { "exec": ["console.log(1)"] } }],
  "item": [
    {
      "name": "Users",
      "variable": [{ "key": "page", "value": "1" }],
      "item": [
        {
          "name": "List users",
          "request": {
            "method": "GET",
            "header": [{ "key": "Accept", "value": "application/json" }],
            "url": {
              "raw": "{{baseUrl}}/users?page={{page}}&debug=1",
              "host": ["{{baseUrl}}"],
              "path": ["users"],
              "query": [
                { "key": "page", "value": "{{page}}" },
                { "key": "debug", "value": "1", "disabled": true }
              ]
            }
          },
          "event": [{
            "listen": "test",
            "script": { "exec": ["pm.test(\"ok\", () => {", "  pm.response.to.have.status(200);", "});"] }
          }],
          "response": []
        },
        {
          "name": "Get user",
          "request": {
            "method": "GET",
            "url": {
              "raw": "{{baseUrl}}/users/:id",
              "variable": [{ "key": "id", "value": "{{userId}}" }]
            }
          },
          "event": [{
            "listen": "prerequest",
            "script": { "exec": ["pm.variables.set(\"n\", 1);", "pm.sendRequest(\"https://shop.test\");"] }
          }]
        },
        {
          "name": "Create user",
          "request": {
            "method": "POST",
            "header": [{ "key": "X-Request-Id", "value": "{{$guid}}" }],
            "body": {
              "mode": "raw",
              "raw": "{\n  \"name\": \"Ann\"\n}",
              "options": { "raw": { "language": "json" } }
            },
            "url": "{{baseUrl}}/users"
          }
        }
      ]
    },
    {
      "name": "Public",
      "auth": { "type": "noauth" },
      "item": [
        {
          "name": "Health",
          "request": { "method": "GET", "url": "https://status.shop.test/health" }
        },
        {
          "name": "Login",
          "request": {
            "auth": {
              "type": "basic",
              "basic": [
                { "key": "username", "value": "{{user}}" },
                { "key": "password", "value": "hunter2" }
              ]
            },
            "method": "POST",
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                { "key": "remember", "value": "true" },
                { "key": "debug", "value": "1", "disabled": true }
              ]
            },
            "url": "{{baseUrl}}/login"
          }
        }
      ]
    },
    {
      "name": "Partners",
      "auth": {
        "type": "apikey",
        "apikey": [
          { "key": "key", "value": "X-Partner-Key" },
          { "key": "value", "value": "{{apiSecret}}" },
          { "key": "in", "value": "header" }
        ]
      },
      "item": [
        {
          "name": "Upload avatar",
          "request": {
            "method": "PUT",
            "body": {
              "mode": "formdata",
              "formdata": [
                { "key": "title", "value": "me", "type": "text" },
                { "key": "file", "type": "file", "src": "/home/ann/avatar.png" }
              ]
            },
            "url": "{{baseUrl}}/avatar"
          }
        },
        {
          "name": "Search",
          "request": {
            "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "other" }] },
            "method": "POST",
            "body": {
              "mode": "graphql",
              "graphql": { "query": "{ users { id } }", "variables": "{\"n\": 1}" }
            },
            "url": "{{baseUrl}}/graphql"
          }
        }
      ]
    },
    { "name": "Trace", "request": { "method": "TRACE", "url": "{{baseUrl}}/" } },
    {
      "name": "Feed",
      "request": {
        "method": "POST",
        "body": { "mode": "raw", "raw": "<a/>", "options": { "raw": { "language": "xml" } } },
        "url": "{{otherHost}}/feed"
      }
    }
  ]
}"#;

    fn find<'a>(import: &'a Import, name: &str) -> &'a Request {
        requests(&import.items)
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    fn folder<'a>(import: &'a Import, name: &str) -> &'a FolderDefaults {
        import
            .items
            .iter()
            .find_map(|i| match i {
                NewItem::Folder {
                    name: n, defaults, ..
                } if n == name => Some(defaults),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no folder {name}"))
    }

    fn import() -> Import {
        parse(COLLECTION).expect("parse")
    }

    /// R-COL-16: folders and requests keep their order and nesting; variables go to
    /// the project and folders; `{{baseUrl}}` becomes a server (D36).
    #[test]
    fn structure_variables_and_servers() {
        let import = import();
        assert_eq!(import.kind, ImportKind::Postman);
        assert_eq!(import.manifest.name, "Shop API");
        let top: Vec<&str> = import
            .items
            .iter()
            .map(|i| match i {
                NewItem::Folder { name, .. } => name.as_str(),
                NewItem::Request(r) => r.name.as_str(),
            })
            .collect();
        assert_eq!(top, ["Users", "Public", "Partners", "Feed"]);
        assert_eq!(import.requests, 8);

        let ids: Vec<&str> = import
            .manifest
            .servers
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, ["baseurl", "otherhost"]);
        assert_eq!(import.environments.len(), 1);
        assert_eq!(import.environments[0].name, "Default");
        assert_eq!(
            import.environments[0].servers,
            BTreeMap::from([
                ("baseurl".to_owned(), "https://shop.test/api".to_owned()),
                ("otherhost".to_owned(), String::new()),
            ])
        );
        // baseUrl is the server now; disabled variables are left out.
        assert_eq!(
            import.manifest.variables,
            BTreeMap::from([("userId".to_owned(), "42".to_owned())])
        );
        assert_eq!(
            folder(&import, "Users").variables,
            BTreeMap::from([("page".to_owned(), "1".to_owned())])
        );

        let list = find(&import, "List users");
        assert_eq!(list.server.as_deref(), Some("baseurl"));
        assert_eq!(list.url, "/users");
        assert_eq!(list.params[0], KeyValue::new("page", "{{page}}"));
        assert!(!list.params[1].enabled);
        assert_eq!(list.headers, [KeyValue::new("Accept", "application/json")]);

        let get = find(&import, "Get user");
        assert_eq!(get.url, "/users/:id");
        assert_eq!(get.path_vars, [KeyValue::new("id", "{{userId}}")]);

        let health = find(&import, "Health");
        assert_eq!(health.server, None);
        assert_eq!(health.url, "https://status.shop.test/health");
    }

    /// R-COL-16, D35: auth at every level, with credentials as secret variables whose
    /// values are kept for the secret store only.
    #[test]
    fn auth_and_secrets() {
        let import = import();
        assert_eq!(
            import.manifest.auth,
            Some(Auth::Bearer {
                token: "{{token}}".into()
            })
        );
        assert_eq!(
            import.manifest.secrets,
            ["apiSecret", "password", "token", "token_2", "user"]
        );
        assert_eq!(
            import.secret_values,
            BTreeMap::from([
                ("apiSecret".to_owned(), "s3cret".to_owned()),
                ("password".to_owned(), "hunter2".to_owned()),
                ("token".to_owned(), "abc123".to_owned()),
                ("token_2".to_owned(), "other".to_owned()),
            ])
        );
        let toml: String = requests(&import.items)
            .into_iter()
            .map(|r| format::write_request(r, None))
            .collect();
        for secret in ["abc123", "hunter2", "s3cret", "other\""] {
            assert!(!toml.contains(secret), "{secret} written");
        }
        assert!(!import.manifest.variables.contains_key("apiSecret"));

        // "noauth" on a folder: its requests turn the project's auth off.
        assert_eq!(find(&import, "Health").auth, Some(Auth::None));
        assert_eq!(folder(&import, "Public").auth, None);
        assert_eq!(
            find(&import, "Login").auth,
            Some(Auth::Basic {
                username: "{{user}}".into(),
                password: "{{password}}".into()
            })
        );
        // Folder API key: a header the folder's requests inherit, without bearer auth.
        let partners = folder(&import, "Partners");
        assert_eq!(
            partners.headers,
            [KeyValue::new("X-Partner-Key", "{{apiSecret}}")]
        );
        assert_eq!(find(&import, "Upload avatar").auth, Some(Auth::None));
        // A request with its own auth turns the folder's key off.
        let search = find(&import, "Search");
        assert_eq!(
            search.auth,
            Some(Auth::Bearer {
                token: "{{token_2}}".into()
            })
        );
        assert_eq!(search.headers.len(), 1);
        assert!(!search.headers[0].enabled);
        assert_eq!(search.headers[0].name, "X-Partner-Key");
        // Requests that inherit don't repeat it.
        assert_eq!(find(&import, "List users").auth, None);
    }

    /// R-COL-16: raw, form, multipart and GraphQL bodies. R-COL-19: scripts translated, or
    /// kept as comments when they use more of Postman's API.
    #[test]
    fn bodies_and_scripts() {
        let import = import();
        assert_eq!(
            find(&import, "Create user").body,
            RequestBody::Json("{\n  \"name\": \"Ann\"\n}".into())
        );
        let RequestBody::Form(fields) = &find(&import, "Login").body else {
            panic!("form body");
        };
        assert_eq!(fields[0], KeyValue::new("remember", "true"));
        assert!(!fields[1].enabled);
        let RequestBody::Multipart(parts) = &find(&import, "Upload avatar").body else {
            panic!("multipart body");
        };
        assert!(!parts[0].file);
        assert!(parts[1].file);
        assert_eq!(parts[1].value, "/home/ann/avatar.png");
        let RequestBody::Json(graphql) = &find(&import, "Search").body else {
            panic!("graphql as json");
        };
        let graphql: Value = serde_json::from_str(graphql).expect("json");
        assert_eq!(graphql["query"], "{ users { id } }");
        assert_eq!(graphql["variables"]["n"], 1);
        let feed = find(&import, "Feed");
        // R-HTTP-19: an XML body, which sends application/xml by itself.
        assert_eq!(feed.body, RequestBody::Xml("<a/>".into()));
        assert_eq!(feed.headers, []);

        let tests = &find(&import, "List users").scripts.tests;
        assert_eq!(
            *tests,
            format!(
                "{}test(\"ok\", () => {{\n  expect(res.status).toBe(200);\n}});\n",
                script::NOTE
            )
        );
        assert_eq!(find(&import, "List users").scripts.pre_request, "");
        let pre_request = &find(&import, "Get user").scripts.pre_request;
        assert!(pre_request.starts_with(&crate::import::script_note(ImportKind::Postman)));
        assert!(
            pre_request
                .ends_with("// vars.set(\"n\", 1);\n// pm.sendRequest(\"https://shop.test\");\n"),
            "{pre_request}"
        );
    }

    #[test]
    fn warnings() {
        let warnings = import().warnings;
        let expect = [
            "The scripts of the collection were left out",
            "“Trace” was left out: Tortuga does not send TRACE requests.",
            "{{otherHost}} has no value in the file",
            "“Upload avatar” uploads files",
            "1 script was kept as a comment",
            "1 script was translated from Postman's pm API to Tortuga's",
        ];
        for text in expect {
            assert!(
                warnings.iter().any(|w| w.contains(text)),
                "missing {text:?} in {warnings:#?}"
            );
        }
        assert_eq!(warnings.len(), expect.len(), "{warnings:#?}");
    }

    /// R-COL-22: the collection's, folders' and requests' descriptions, as text or as
    /// `{content, type}`, and on the request or its item.
    #[test]
    fn descriptions() {
        let import = parse(
            r##"{"info": {"name": "Shop", "description": {"content": "# Shop\n\nThe **shop** API ü😀.\n", "type": "text/markdown"}},
                "item": [
                  {"name": "Orders", "description": "  Everything about orders.  ",
                   "item": [
                     {"name": "List", "request": {"method": "GET", "url": "https://shop.test/orders", "description": "Lists *all* orders."}},
                     {"name": "Get", "description": "On the item.", "request": {"method": "GET", "url": "https://shop.test/orders/1"}}
                   ]},
                  {"name": "Ping", "request": "https://shop.test/ping"}
                ]}"##,
        )
        .expect("parse");
        assert_eq!(
            import.manifest.description,
            "# Shop\n\nThe **shop** API ü😀."
        );
        assert_eq!(
            folder(&import, "Orders").description,
            "Everything about orders."
        );
        assert_eq!(find(&import, "List").description, "Lists *all* orders.");
        assert_eq!(find(&import, "Get").description, "On the item.");
        assert_eq!(find(&import, "Ping").description, "");
    }

    /// v2.0 auth parameters are objects; Postman's API wraps the collection; a request
    /// may be just a URL; URL objects without `raw` are put together.
    #[test]
    fn older_and_other_shapes() {
        let text = r#"{"collection": {
            "info": {"name": "Old", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json"},
            "auth": {"type": "basic", "basic": {"username": "ann", "password": "{{pw}}"}},
            "item": [
                {"name": "Plain", "request": "https://a.test/x"},
                {"name": "Parts", "request": {"method": "delete", "header": "X-A: 1\nX-B: two",
                  "url": {"protocol": "http", "host": ["localhost"], "port": "8080",
                          "path": ["v1", "items"]}}},
                {"name": "Digest", "request": {"auth": {"type": "digest"}, "url": "https://a.test"}},
                {"name": "OAuth", "request": {"url": "https://a.test",
                  "auth": {"type": "oauth2", "oauth2": [{"key": "accessToken", "value": ""}]}}}
            ]}}"#;
        let import = parse(text).expect("parse");
        assert_eq!(
            import.manifest.auth,
            Some(Auth::Basic {
                username: "{{username}}".into(),
                password: "{{pw}}".into()
            })
        );
        assert_eq!(
            import.secret_values,
            BTreeMap::from([("username".to_owned(), "ann".to_owned())])
        );
        assert_eq!(find(&import, "Plain").url, "https://a.test/x");
        let parts = find(&import, "Parts");
        assert_eq!(parts.method, HttpMethod::Delete);
        assert_eq!(parts.url, "http://localhost:8080/v1/items");
        assert_eq!(
            parts.headers,
            [KeyValue::new("X-A", "1"), KeyValue::new("X-B", "two")]
        );
        assert_eq!(find(&import, "Digest").auth, Some(Auth::None));
        assert_eq!(
            find(&import, "OAuth").auth,
            Some(Auth::Bearer {
                token: "{{access_token}}".into()
            })
        );
        assert!(import.manifest.secrets.contains(&"access_token".to_owned()));
        assert!(import.warnings.iter().any(|w| w.contains("digest auth")));
        assert!(
            import
                .warnings
                .iter()
                .any(|w| w.starts_with("“OAuth” uses OAuth 2"))
        );
        // No base URL variable: an environment without servers.
        assert_eq!(import.manifest.servers, []);
        assert_eq!(import.environments.len(), 1);
    }

    #[test]
    fn versions_and_errors() {
        let message = |text: &str| crate::error::message(&parse(text).expect_err("refused"));
        assert!(message("not json").contains("not valid JSON"));
        assert!(
            message(r#"{"id": "1", "name": "Old", "requests": [], "order": []}"#)
                .contains("v1 format")
        );
        assert!(message(r#"{"openapi": "3.0.0"}"#).contains("not a Postman collection"));
        assert!(looks_like_collection(
            &serde_json::json!({"info": {"name": "x"}, "item": []})
        ));
        assert!(!looks_like_collection(
            &serde_json::json!({"openapi": "3.0.0"})
        ));
    }
}
