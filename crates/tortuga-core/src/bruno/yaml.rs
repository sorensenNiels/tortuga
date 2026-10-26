//! Bruno's YAML collections (`OpenCollection`, Bruno 3 and later; R-COL-34), read into
//! the blocks of a `.bru` file ([`BruFile`]) so both are mapped the same way.
//!
//! `opencollection.yml` marks the collection: its `info`, the `request` defaults every
//! request gets, `docs` and `config`. A folder has `folder.yml` (`info`, `request`,
//! `docs`); a request file has `info`, `http` (or `graphql`) with the method, URL,
//! params, headers, body and auth, `runtime` (variables, actions, assertions, scripts),
//! `settings`, `docs` and saved `examples`. Environments are `environments/*.yml` with
//! a list of `variables`.

use serde_json::Value;

use super::lang::{Block, Body, BruFile, Pair};
use crate::import::{EnvValue, ImportedEnvironment, json_text as text, short_name, unique_name};
use crate::mock::Example;
use crate::model::{KeyValue, Response};

/// The `.bru` blocks for any of Bruno's YAML files.
pub(super) fn to_bru(doc: &Value) -> BruFile {
    let mut out = Builder::default();
    out.meta(doc.get("info"));
    let kind = doc.pointer("/info/type").and_then(Value::as_str);
    let section = match kind {
        Some("graphql") => doc.get("graphql"),
        _ => doc.get("http"),
    };
    if let Some(section) = section {
        out.request(section, kind == Some("graphql"));
    }
    if let Some(defaults) = doc.get("request") {
        out.defaults(defaults);
    }
    if let Some(runtime) = doc.get("runtime") {
        out.runtime(runtime);
    }
    out.settings(doc.get("settings"));
    out.text("docs", &description(doc.get("docs")));
    BruFile {
        blocks: out.blocks,
        examples: examples(doc.get("examples")),
    }
}

/// `opencollection.yml`'s settings in the shape of `bruno.json`, which the import
/// reads for the name, the folders to skip, the proxy and client certificates.
pub(super) fn config(doc: &Value) -> Value {
    let proxy = doc.pointer("/config/proxy");
    let proxy_on = proxy.is_some_and(|p| {
        p.get("inherit").and_then(Value::as_bool) == Some(false)
            && p.get("disabled").and_then(Value::as_bool) != Some(true)
    });
    serde_json::json!({
        "name": text(doc.pointer("/info/name")),
        "ignore": doc.pointer("/extensions/bruno/ignore").cloned().unwrap_or(Value::Null),
        "proxy": { "enabled": proxy_on },
        "clientCertificates": {
            "certs": doc.pointer("/config/clientCertificates").cloned().unwrap_or(Value::Null),
        },
    })
}

/// An environment file (`environments/<name>.yml`): its enabled variables, secret
/// ones without a value.
pub(super) fn environment(doc: &Value, file_stem: &str) -> Option<ImportedEnvironment> {
    let variables = doc.get("variables")?.as_array()?;
    let name = Some(text(doc.get("name")))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file_stem.to_owned());
    let values = variables
        .iter()
        .filter(|v| !is_disabled(v))
        .filter_map(|v| {
            let key = text(v.get("name"));
            let secret = v.get("secret").and_then(Value::as_bool) == Some(true);
            (!key.trim().is_empty()).then(|| EnvValue {
                value: if secret {
                    String::new()
                } else {
                    variable_value(v.get("value"))
                },
                key,
                secret,
            })
        })
        .collect();
    Some(ImportedEnvironment {
        name,
        values,
        warnings: Vec::new(),
    })
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
}

impl Builder {
    fn dict(&mut self, name: &str, pairs: Vec<Pair>) {
        if !pairs.is_empty() {
            self.blocks.push(Block {
                name: name.to_owned(),
                body: Body::Dict(pairs),
            });
        }
    }

    fn text(&mut self, name: &str, text: &str) {
        if !text.trim().is_empty() {
            self.blocks.push(Block {
                name: name.to_owned(),
                body: Body::Text(text.to_owned()),
            });
        }
    }

    fn meta(&mut self, info: Option<&Value>) {
        let Some(info) = info else { return };
        let pairs = ["name", "type", "seq"]
            .into_iter()
            .filter_map(|key| {
                let value = text(info.get(key));
                (!value.is_empty()).then(|| pair(key, value, true))
            })
            .collect();
        self.dict("meta", pairs);
    }

    /// The method block with the URL and the body's and auth's modes, and the blocks
    /// they point to.
    fn request(&mut self, section: &Value, graphql: bool) {
        let default_method = if graphql { "post" } else { "get" };
        let method = Some(text(section.get("method")).to_ascii_lowercase())
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| default_method.to_owned());
        let body_mode = if graphql {
            self.graphql_body(section.get("body"))
        } else {
            self.body(section.get("body"))
        };
        let auth_mode = self.auth(section.get("auth"));
        self.blocks.insert(
            0,
            Block {
                name: method,
                body: Body::Dict(vec![
                    pair("url", text(section.get("url")), true),
                    pair("body", body_mode.to_owned(), true),
                    pair("auth", auth_mode, true),
                ]),
            },
        );
        let params = list(section.get("params"));
        for (kind, block) in [("query", "params:query"), ("path", "params:path")] {
            let pairs = params
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str).unwrap_or("query") == kind)
                .map(named_pair)
                .collect();
            self.dict(block, pairs);
        }
        self.dict(
            "headers",
            list(section.get("headers"))
                .iter()
                .map(named_pair)
                .collect(),
        );
    }

    /// The body's blocks; returns its mode as `.bru` names it.
    fn body(&mut self, body: Option<&Value>) -> &'static str {
        let Some(body) = body else { return "none" };
        let data = body.get("data");
        match body.get("type").and_then(Value::as_str).unwrap_or_default() {
            raw @ ("json" | "text" | "xml" | "sparql") => {
                self.text(&format!("body:{raw}"), &text(data));
                match raw {
                    "json" => "json",
                    "text" => "text",
                    "xml" => "xml",
                    _ => "sparql",
                }
            }
            "form-urlencoded" => {
                let pairs = list(data).iter().map(named_pair).collect();
                self.dict("body:form-urlencoded", pairs);
                "formUrlEncoded"
            }
            "multipart-form" => {
                let pairs = list(data).iter().map(multipart_pair).collect();
                self.dict("body:multipart-form", pairs);
                "multipartForm"
            }
            "file" => "file",
            _ => "none",
        }
    }

    fn graphql_body(&mut self, body: Option<&Value>) -> &'static str {
        let Some(body) = body else { return "none" };
        self.text("body:graphql", &text(body.get("query")));
        self.text("body:graphql:vars", &text(body.get("variables")));
        "graphql"
    }

    /// The auth's block; returns its mode. Without auth Bruno sends none.
    fn auth(&mut self, auth: Option<&Value>) -> String {
        let auth = match auth {
            None | Some(Value::Null) => return "none".to_owned(),
            Some(Value::String(mode)) => return mode.clone(),
            Some(auth) => auth,
        };
        let mode = text(auth.get("type"));
        let keys: &[&str] = match mode.as_str() {
            "bearer" => &["token"],
            "basic" => &["username", "password"],
            "apikey" => &["key", "value"],
            _ => &[],
        };
        let mut pairs: Vec<Pair> = keys
            .iter()
            .map(|key| pair(key, text(auth.get(*key)), true))
            .collect();
        if mode == "apikey" {
            let placement = match auth.get("placement").and_then(Value::as_str) {
                Some("query") => "queryparams",
                _ => "header",
            };
            pairs.push(pair("placement", placement.to_owned(), true));
        }
        self.dict(&format!("auth:{mode}"), pairs);
        mode
    }

    /// The `request` defaults of `opencollection.yml` and `folder.yml`.
    fn defaults(&mut self, defaults: &Value) {
        let headers = list(defaults.get("headers"))
            .iter()
            .map(named_pair)
            .collect();
        self.dict("headers", headers);
        if let Some(auth) = defaults.get("auth") {
            let mode = self.auth(Some(auth));
            self.dict("auth", vec![pair("mode", mode, true)]);
        }
        self.variables(defaults.get("variables"));
        self.scripts(defaults.get("scripts"));
    }

    fn runtime(&mut self, runtime: &Value) {
        self.variables(runtime.get("variables"));
        let captures = list(runtime.get("actions"))
            .iter()
            .filter(|a| {
                a.get("type").and_then(Value::as_str) == Some("set-variable")
                    && a.get("phase").and_then(Value::as_str) == Some("after-response")
            })
            .map(|a| {
                pair(
                    &text(a.pointer("/variable/name")),
                    text(a.pointer("/selector/expression")),
                    !is_disabled(a),
                )
            })
            .collect();
        self.dict("vars:post-response", captures);
        let asserts = list(runtime.get("assertions"))
            .iter()
            .map(|a| {
                let operator = text(a.get("operator"));
                let value = match a.get("value") {
                    None | Some(Value::Null) => operator,
                    v => format!("{operator} {}", text(v)),
                };
                pair(&text(a.get("expression")), value, !is_disabled(a))
            })
            .collect();
        self.dict("assert", asserts);
        self.scripts(runtime.get("scripts"));
    }

    fn variables(&mut self, variables: Option<&Value>) {
        let pairs = list(variables)
            .iter()
            .map(|v| {
                pair(
                    &text(v.get("name")),
                    variable_value(v.get("value")),
                    !is_disabled(v),
                )
            })
            .collect();
        self.dict("vars:pre-request", pairs);
    }

    fn scripts(&mut self, scripts: Option<&Value>) {
        for script in list(scripts) {
            let block = match script.get("type").and_then(Value::as_str) {
                Some("before-request") => "script:pre-request",
                Some("after-response") => "script:post-response",
                Some("tests") => "tests",
                _ => continue,
            };
            self.text(block, &text(script.get("code")));
        }
    }

    fn settings(&mut self, settings: Option<&Value>) {
        let Some(settings) = settings else { return };
        let mut pairs = Vec::new();
        if let Some(timeout) = settings.get("timeout").filter(|t| t.is_number()) {
            pairs.push(pair("timeout", text(Some(timeout)), true));
        }
        if settings.get("followRedirects").and_then(Value::as_bool) == Some(false) {
            pairs.push(pair("followRedirects", "false".to_owned(), true));
        }
        self.dict("settings", pairs);
    }
}

/// Saved examples (R-MOCK-1): each response, named as in Bruno and made unique,
/// filtered as a response saved in Tortuga is.
fn examples(list_value: Option<&Value>) -> Vec<Example> {
    let mut out: Vec<Example> = Vec::new();
    for saved in list(list_value) {
        let Some(response) = saved.get("response") else {
            continue;
        };
        let base = short_name(
            Some(text(saved.get("name")))
                .filter(|n| !n.trim().is_empty())
                .as_deref()
                .unwrap_or("Example"),
        );
        let name = unique_name(&base, |n| out.iter().any(|e| e.name == n));
        let status = match response.get("status") {
            Some(Value::Number(n)) => n.as_u64(),
            other => text(other).trim().parse().ok(),
        }
        .and_then(|s| u16::try_from(s).ok())
        .filter(|s| (100..=999).contains(s))
        .unwrap_or(200);
        let response = Response {
            status,
            status_text: String::new(),
            headers: list(response.get("headers"))
                .iter()
                .map(|h| KeyValue::new(text(h.get("name")), text(h.get("value"))))
                .collect(),
            body: text(response.pointer("/body/data")).into_bytes(),
            elapsed: std::time::Duration::ZERO,
            timing: None,
        };
        out.push(Example::from_response(name, &response));
    }
    out
}

fn pair(key: &str, value: String, enabled: bool) -> Pair {
    Pair {
        key: key.to_owned(),
        value,
        enabled,
    }
}

/// A `{name, value, disabled}` entry.
fn named_pair(entry: &Value) -> Pair {
    pair(
        &text(entry.get("name")),
        text(entry.get("value")),
        !is_disabled(entry),
    )
}

/// A multipart entry as `.bru` writes it: text, or `@file(a|b) @contentType(type)`.
fn multipart_pair(entry: &Value) -> Pair {
    let mut value = if entry.get("type").and_then(Value::as_str) == Some("file") {
        let paths: Vec<String> = match entry.get("value") {
            Some(Value::Array(paths)) => paths.iter().map(|p| text(Some(p))).collect(),
            other => vec![text(other)],
        };
        format!("@file({})", paths.join("|"))
    } else {
        text(entry.get("value"))
    };
    let content_type = text(entry.get("contentType"));
    if !content_type.trim().is_empty() {
        value = format!("{value} @contentType({})", content_type.trim());
    }
    pair(&text(entry.get("name")), value, !is_disabled(entry))
}

/// A variable's value: text, or `{type, data}` for a typed one.
fn variable_value(value: Option<&Value>) -> String {
    match value {
        Some(typed @ Value::Object(_)) if typed.get("data").is_some() => text(typed.get("data")),
        other => text(other),
    }
}

/// `docs` and descriptions: text, or `{content}`.
fn description(value: Option<&Value>) -> String {
    match value {
        Some(Value::Object(o)) => text(o.get("content")),
        other => text(other),
    }
}

fn list(value: Option<&Value>) -> &[Value] {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn is_disabled(value: &Value) -> bool {
    value.get("disabled").and_then(Value::as_bool) == Some(true)
}
