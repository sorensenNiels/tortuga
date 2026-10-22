//! Importing an Insomnia export (v4 JSON, v5 YAML) as a new project (R-COL-32, D34,
//! D60).
//!
//! Both formats are read into one tree ([`Export`]): v4 lists resources that point to
//! their parent, v5 nests them. The collection becomes:
//! - its folders and requests, in Insomnia's order;
//! - folder headers, auth and environments as folder defaults (R-COL-11); auth as the
//!   Postman import maps it ([`crate::import::auth`]), credentials as secret variables
//!   (D35);
//! - the base environment as project variables, each sub-environment as an
//!   environment (a private one's values secret); a variable that starts request URLs
//!   as a server (D36); nested values as `parent.child` variables, as Insomnia reads
//!   them (`{{ _.parent.child }}`);
//! - `{{ _.name }}` as `{{name}}`; the `uuid` and `now` tags as dynamic variables
//!   (R-VAR-8); other template tags are kept as written and listed;
//! - scripts translated as Postman's are (R-COL-19), since Insomnia's `insomnia` API is
//!   Postman's `pm` API by another name.
//!
//! What can't be imported as written is reported in [`Import::warnings`].

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::collection::NewItem;
use crate::format::{FolderDefaults, Manifest};
use crate::import::auth::{AuthSpec, Credentials, Inherited};
use crate::import::{
    Collected, EnvValue, Import, ImportKind, ImportedEnvironment, graphql_body, json_or_yaml,
    json_text as string, short_name,
};
use crate::model::{
    FormPart, HttpMethod, KeyValue, Request, RequestBody, Scripts, is_json_content_type,
    is_xml_content_type,
};
use crate::postman::script::translate_insomnia;
use crate::{Error, Result};

#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInsomnia(message.into())
}

/// Whether `text` (with `doc`, when it is JSON) is an Insomnia export: v4
/// (`"_type": "export"`) or v5 (`type: collection.insomnia.rest/5.0`).
pub(crate) fn looks_like_export(text: &str, doc: Option<&Value>) -> bool {
    if let Some(doc) = doc {
        return doc.get("_type").and_then(Value::as_str) == Some("export")
            || v5_type(doc).is_some();
    }
    text.lines().take(20).any(|line| {
        line.strip_prefix("type:")
            .is_some_and(|t| t.contains(".insomnia.rest/"))
    })
}

/// The `type` of a v5 export.
fn v5_type(doc: &Value) -> Option<&str> {
    doc.get("type")
        .and_then(Value::as_str)
        .filter(|t| t.contains(".insomnia.rest/"))
}

/// Reads an Insomnia export into a project (R-COL-32).
pub fn parse(text: &str) -> Result<Import> {
    let doc = json_or_yaml(text).map_err(invalid)?;
    let export = Export::read(&doc)?;
    Ok(Importer::new(export.format).run(&export))
}

// ----------------------------------------------------------------- the tree

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    V4,
    V5,
}

impl Format {
    /// A field that v4 has at `v4` and v5 at the JSON pointer `v5`.
    fn field<'a>(self, item: &'a Value, v4: &str, v5: &str) -> Option<&'a Value> {
        match self {
            Self::V4 => item.get(v4),
            Self::V5 => item.pointer(v5),
        }
    }

    fn description(self, item: &Value) -> String {
        string(self.field(item, "description", "/meta/description"))
            .trim()
            .to_owned()
    }

    fn sort_key(self, item: &Value) -> f64 {
        self.field(item, "metaSortKey", "/meta/sortKey")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    }

    /// The pre-request script, or the after-response one.
    fn script(self, item: &Value, after: bool) -> String {
        if after {
            string(self.field(item, "afterResponseScript", "/scripts/afterResponse"))
        } else {
            string(self.field(item, "preRequestScript", "/scripts/preRequest"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Folder,
    Request,
    /// A request Tortuga doesn't send, by what it is ("gRPC").
    Other(&'static str),
}

/// A folder or request of the export.
struct Node<'a> {
    item: &'a Value,
    kind: NodeKind,
    children: Vec<Node<'a>>,
}

/// What an export holds, in the shape both formats share.
struct Export<'a> {
    format: Format,
    name: String,
    description: String,
    items: Vec<Node<'a>>,
    /// The base environment's values, which every environment has.
    base: Option<&'a Value>,
    sub_environments: Vec<&'a Value>,
    has_cookies: bool,
    has_spec: bool,
    test_suites: Vec<String>,
    /// The names of the collections an export of several has besides the imported one.
    others: Vec<String>,
}

impl<'a> Export<'a> {
    fn read(doc: &'a Value) -> Result<Self> {
        if doc.get("_type").and_then(Value::as_str) == Some("export") {
            return Self::v4(doc);
        }
        match v5_type(doc) {
            Some(t) if t.starts_with("collection.") || t.starts_with("spec.") => Ok(Self::v5(doc)),
            Some(t) if t.starts_with("environment.") => Err(invalid(
                "this is an export of Insomnia environments: export the collection instead; \
                 its environments come with it",
            )),
            Some(t) => Err(invalid(format!(
                "Tortuga imports Insomnia collections, and this is a {t} export"
            ))),
            None => Err(invalid(
                "it is not an Insomnia export (v4 JSON or v5 YAML): export the collection \
                 from Insomnia",
            )),
        }
    }

    fn v4(doc: &'a Value) -> Result<Self> {
        let resources = doc
            .get("resources")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("it has no resources"))?;
        let of_type = |kind: &'static str| {
            resources
                .iter()
                .filter(move |r| r.get("_type").and_then(Value::as_str) == Some(kind))
        };
        let parent = |r: &Value| r.get("parentId").and_then(Value::as_str).map(str::to_owned);
        let id = |r: &Value| r.get("_id").and_then(Value::as_str).map(str::to_owned);
        let workspaces: Vec<&Value> = of_type("workspace").collect();
        let workspace = workspaces.first().copied();
        let root = workspace.and_then(id);
        // Without a workspace, the resources whose parent isn't in the export are the top.
        let known: BTreeSet<String> = resources.iter().filter_map(id).collect();
        let at_top = |r: &Value| match (&root, parent(r)) {
            (Some(root), p) => p.as_ref() == Some(root),
            (None, p) => p.is_none_or(|p| !known.contains(&p)),
        };

        let base = of_type("environment").find(|e| at_top(e));
        let base_id = base.and_then(id);
        let mut sub_environments: Vec<&Value> = of_type("environment")
            .filter(|e| base_id.is_some() && parent(e) == base_id)
            .collect();
        sub_environments.sort_by(|a, b| Format::V4.sort_key(a).total_cmp(&Format::V4.sort_key(b)));
        let has_cookies = of_type("cookie_jar").filter(|j| at_top(j)).any(|j| {
            j.get("cookies")
                .and_then(Value::as_array)
                .is_some_and(|c| !c.is_empty())
        });
        let has_spec = of_type("api_spec")
            .filter(|s| at_top(s))
            .any(|s| !string(s.get("contents")).trim().is_empty());
        let test_suites = of_type("unit_test_suite")
            .filter(|s| at_top(s))
            .map(|s| string(s.get("name")))
            .collect();

        let items = v4_children(resources, &|r| at_top(r));
        Ok(Self {
            format: Format::V4,
            name: workspace.map(|w| string(w.get("name"))).unwrap_or_default(),
            description: workspace
                .map(|w| Format::V4.description(w))
                .unwrap_or_default(),
            items,
            base: base.and_then(|e| e.get("data")),
            sub_environments,
            has_cookies,
            has_spec,
            test_suites,
            others: workspaces
                .iter()
                .skip(1)
                .map(|w| string(w.get("name")))
                .collect(),
        })
    }

    fn v5(doc: &'a Value) -> Self {
        let environments = doc.get("environments");
        let mut sub_environments: Vec<&Value> = environments
            .and_then(|e| e.get("subEnvironments"))
            .and_then(Value::as_array)
            .map(|list| list.iter().collect())
            .unwrap_or_default();
        sub_environments.sort_by(|a, b| Format::V5.sort_key(a).total_cmp(&Format::V5.sort_key(b)));
        Self {
            format: Format::V5,
            name: string(doc.get("name")),
            description: Format::V5.description(doc),
            items: v5_nodes(doc.get("collection")),
            base: environments.and_then(|e| e.get("data")),
            sub_environments,
            has_cookies: doc
                .pointer("/cookieJar/cookies")
                .and_then(Value::as_array)
                .is_some_and(|c| !c.is_empty()),
            has_spec: !string(doc.pointer("/spec/contents")).trim().is_empty(),
            test_suites: Vec::new(),
            others: Vec::new(),
        }
    }
}

/// The folders and requests that `is_child` picks, with theirs, in Insomnia's order.
fn v4_children<'a>(resources: &'a [Value], is_child: &dyn Fn(&Value) -> bool) -> Vec<Node<'a>> {
    let mut nodes: Vec<Node<'a>> = resources
        .iter()
        .filter(|r| is_child(r))
        .filter_map(|item| {
            let kind = match item.get("_type").and_then(Value::as_str)? {
                "request_group" => NodeKind::Folder,
                "request" => NodeKind::Request,
                "grpc_request" => NodeKind::Other("gRPC"),
                "websocket_request" => NodeKind::Other("WebSocket"),
                _ => return None,
            };
            let children = match (kind, item.get("_id").and_then(Value::as_str)) {
                (NodeKind::Folder, Some(id)) => v4_children(resources, &|r| {
                    r.get("parentId").and_then(Value::as_str) == Some(id)
                }),
                _ => Vec::new(),
            };
            Some(Node {
                item,
                kind,
                children,
            })
        })
        .collect();
    nodes.sort_by(|a, b| {
        Format::V4
            .sort_key(a.item)
            .total_cmp(&Format::V4.sort_key(b.item))
    });
    nodes
}

fn v5_nodes(list: Option<&Value>) -> Vec<Node<'_>> {
    let mut nodes: Vec<Node<'_>> = list
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            let kind = if item.get("children").is_some() {
                NodeKind::Folder
            } else if item.get("method").is_some() {
                NodeKind::Request
            } else if string(item.get("url")).trim_start().starts_with("ws") {
                NodeKind::Other("WebSocket")
            } else {
                NodeKind::Other("gRPC")
            };
            Node {
                item,
                kind,
                children: v5_nodes(item.get("children")),
            }
        })
        .collect();
    nodes.sort_by(|a, b| {
        Format::V5
            .sort_key(a.item)
            .total_cmp(&Format::V5.sort_key(b.item))
    });
    nodes
}

// -------------------------------------------------------------- the mapping

struct Importer {
    format: Format,
    c: Collected,
    /// Template tags kept as written: tag name → where.
    tags: BTreeMap<String, BTreeSet<String>>,
}

impl Importer {
    fn new(format: Format) -> Self {
        Self {
            format,
            c: Collected::default(),
            tags: BTreeMap::new(),
        }
    }

    fn run(mut self, export: &Export<'_>) -> Import {
        let name = Some(export.name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or("Insomnia collection");
        let mut manifest = Manifest::new(name.to_owned());
        manifest.description.clone_from(&export.description);
        self.c.variables = self.variables(export.base, "the base environment");
        self.export_warnings(export);
        let top = Inherited::default();
        let items = self.items(&export.items, &top);
        let environments: Vec<ImportedEnvironment> = export
            .sub_environments
            .iter()
            .map(|e| self.environment(e))
            .collect();
        self.tag_warnings();
        self.c
            .finish(ImportKind::Insomnia, manifest, items)
            .with_environments(&environments)
    }

    fn export_warnings(&mut self, export: &Export<'_>) {
        if !export.others.is_empty() {
            self.c.warn(format!(
                "The export has more collections: {} {} not imported. Export each from \
                 Insomnia on its own to import it.",
                quoted_list(&export.others),
                if export.others.len() == 1 {
                    "was"
                } else {
                    "were"
                }
            ));
        }
        if export.has_cookies {
            self.c.warn(
                "The collection's cookies were not imported: Tortuga keeps the cookies \
                 responses set on this computer.",
            );
        }
        if export.has_spec {
            self.c.warn(
                "The collection's API description was not imported: export it from Insomnia \
                 as a file and use Import OpenAPI….",
            );
        }
        if !export.test_suites.is_empty() {
            self.c.warn(format!(
                "The test suites {} were left out: in Tortuga, tests belong to a request's \
                 scripts or a workflow.",
                quoted_list(&export.test_suites)
            ));
        }
    }

    /// Insomnia's template syntax in Tortuga's, for text at `place`.
    fn fix(&mut self, raw: &str, place: &str) -> String {
        let text = variables_fixed(raw);
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find("{%") {
            let Some(len) = rest[start..].find("%}") else {
                break;
            };
            let tag = &rest[start..start + len + 2];
            out.push_str(&rest[..start]);
            let inner = tag[2..tag.len() - 2].trim();
            let name = inner.split_whitespace().next().unwrap_or_default();
            let arg = inner
                .get(name.len()..)
                .unwrap_or_default()
                .trim()
                .trim_matches(['\'', '"']);
            match (name, arg) {
                ("uuid", _) => out.push_str("{{$guid}}"),
                ("now", "" | "iso-8601") => out.push_str("{{$isoTimestamp}}"),
                ("now", "unix") => out.push_str("{{$timestamp}}"),
                _ => {
                    out.push_str(tag);
                    self.tags
                        .entry(name.to_owned())
                        .or_default()
                        .insert(place.to_owned());
                }
            }
            rest = &rest[start + len + 2..];
        }
        out.push_str(rest);
        out
    }

    fn tag_warnings(&mut self) {
        for (tag, places) in std::mem::take(&mut self.tags) {
            let places: Vec<String> = places.into_iter().collect();
            let hint = match tag.as_str() {
                "response" => {
                    "Tortuga takes values from other responses with captures (R-TEST-4): add one \
                     to the request it reads"
                }
                "prompt" => "use a variable for the value it asked for",
                _ => "Tortuga has no such tag, so replace it with a variable",
            };
            self.c.warn(format!(
                "Insomnia's {{% {tag} %}} tag was kept as written in {}: {hint}.",
                places.join(", ")
            ));
        }
    }

    /// An environment's `data`, nested values as `parent.child` names.
    fn variables(&mut self, data: Option<&Value>, place: &str) -> BTreeMap<String, String> {
        let mut flat = Vec::new();
        if let Some(data) = data {
            flatten("", data, &mut flat);
        }
        flat.into_iter()
            .map(|(name, value)| (name, self.fix(&value, place)))
            .collect()
    }

    fn environment(&mut self, env: &Value) -> ImportedEnvironment {
        let name = Some(string(env.get("name")))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Environment".to_owned());
        let private = env.get("isPrivate").and_then(Value::as_bool) == Some(true)
            || env.pointer("/meta/isPrivate").and_then(Value::as_bool) == Some(true);
        let values = self
            .variables(env.get("data"), &format!("environment “{name}”"))
            .into_iter()
            .map(|(key, value)| EnvValue {
                key,
                value,
                secret: private,
            })
            .collect();
        ImportedEnvironment {
            name,
            values,
            warnings: Vec::new(),
        }
    }

    // ------------------------------------------------------------------ items

    fn items(&mut self, nodes: &[Node<'_>], inherited: &Inherited) -> Vec<NewItem> {
        let mut out = Vec::new();
        for node in nodes {
            match node.kind {
                NodeKind::Folder => out.push(self.folder(node, inherited)),
                NodeKind::Request => {
                    out.extend(
                        self.request(node.item, inherited)
                            .map(|r| NewItem::Request(Box::new(r))),
                    );
                }
                NodeKind::Other(kind) => {
                    let name = item_name(node.item).unwrap_or("A request");
                    self.c.warn(format!(
                        "“{name}” is a {kind} request, which Tortuga does not send, so it was \
                         left out."
                    ));
                }
            }
        }
        out
    }

    fn folder(&mut self, node: &Node<'_>, inherited: &Inherited) -> NewItem {
        let item = node.item;
        let name = item_name(item).unwrap_or("Folder").to_owned();
        let place = format!("folder “{name}”");
        let has_scripts = [false, true]
            .into_iter()
            .any(|after| !self.format.script(item, after).trim().is_empty());
        if has_scripts {
            self.c.warn(format!(
                "The scripts of {place} were left out: Tortuga runs scripts per request."
            ));
        }
        let mut defaults = FolderDefaults {
            description: self.format.description(item),
            headers: self.key_values(item.get("headers"), &place),
            auth: None,
            variables: self.variables(item.get("environment"), &place),
        };
        let creds = self.credentials(item.get("authentication"), &place);
        let below = inherited.folder(creds, &mut defaults);
        NewItem::Folder {
            name,
            defaults,
            items: self.items(&node.children, &below),
        }
    }

    fn request(&mut self, item: &Value, inherited: &Inherited) -> Option<Request> {
        let method = Some(string(item.get("method")))
            .map(|m| m.trim().to_owned())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "GET".to_owned());
        let raw_url = string(item.get("url"));
        let name = short_name(
            &item_name(item).map_or_else(|| format!("{method} {raw_url}"), str::to_owned),
        );
        let place = format!("“{name}”");
        let Ok(method) = method.parse::<HttpMethod>() else {
            self.c.warn(format!(
                "{place} was left out: Tortuga does not send {} requests.",
                method.to_ascii_uppercase()
            ));
            return None;
        };
        let url = self.fix(raw_url.trim(), &place);
        let mut request = Request::new(method, &url);
        request.name = name;
        request.description = self.format.description(item);
        let params = self.key_values(item.get("parameters"), &place);
        request.params.extend(params);
        if let Some((server, rest)) = self.c.server_for(&request.url) {
            request.server = Some(server);
            request.url = rest;
        }
        request.sync_path_variables();
        let path_values = self.key_values(item.get("pathParameters"), &place);
        for var in &mut request.path_vars {
            if let Some(value) = path_values.iter().find(|v| v.name == var.name) {
                var.value.clone_from(&value.value);
            }
        }
        request.headers = self.key_values(item.get("headers"), &place);
        let (body, content_type) = self.body(item.get("body"), &place);
        request.body = body;
        if let Some(content_type) = content_type
            && !request.has_header("content-type")
        {
            request.headers.push(content_type);
        }
        let own = self.credentials(item.get("authentication"), &place);
        inherited.apply(own, &mut request);
        self.apply_settings(item, &mut request);
        request.scripts = Scripts {
            pre_request: self.script(&self.format.script(item, false)),
            tests: self.script(&self.format.script(item, true)),
        };
        Some(request)
    }

    /// Enabled and disabled `{name, value}` entries; ones without a name are left out.
    fn key_values(&mut self, list: Option<&Value>, place: &str) -> Vec<KeyValue> {
        let mut out = Vec::new();
        for entry in list.and_then(Value::as_array).into_iter().flatten() {
            let name = string(entry.get("name"));
            if name.trim().is_empty() {
                continue;
            }
            let value = self.fix(&string(entry.get("value")), place);
            out.push(KeyValue {
                enabled: !is_disabled(entry),
                ..KeyValue::new(name.trim(), value)
            });
        }
        out
    }

    /// The body, and a `Content-Type` for a type that Tortuga's body kinds don't set.
    fn body(&mut self, body: Option<&Value>, place: &str) -> (RequestBody, Option<KeyValue>) {
        let Some(body) = body.filter(|b| b.as_object().is_some_and(|o| !o.is_empty())) else {
            return (RequestBody::None, None);
        };
        let mime = string(body.get("mimeType")).trim().to_ascii_lowercase();
        let text = self.fix(&string(body.get("text")), place);
        if body.get("params").is_some() {
            if mime.starts_with("multipart/") {
                return (
                    RequestBody::Multipart(self.form_parts(body.get("params"), place)),
                    None,
                );
            }
            return (
                RequestBody::Form(self.key_values(body.get("params"), place)),
                None,
            );
        }
        if !string(body.get("fileName")).trim().is_empty() {
            self.c.warn(format!(
                "The body of {place} is a file, which Tortuga can't send as a whole body yet, \
                 so it was left out."
            ));
            return (RequestBody::None, None);
        }
        if mime == "application/graphql" {
            let graphql: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let variables = match graphql.get("variables") {
                Some(v @ (Value::Object(_) | Value::Array(_))) => v.to_string(),
                v => string(v),
            };
            return (
                graphql_body(&string(graphql.get("query")), &variables),
                None,
            );
        }
        if is_json_content_type(&mime) {
            return (RequestBody::Json(text), None);
        }
        if is_xml_content_type(&mime) {
            return (RequestBody::Xml(text), None);
        }
        if mime.is_empty() {
            let body = if text.is_empty() {
                RequestBody::None
            } else {
                RequestBody::Text(text)
            };
            return (body, None);
        }
        (
            RequestBody::Text(text),
            Some(KeyValue::new(
                "Content-Type",
                string(body.get("mimeType")).trim(),
            )),
        )
    }

    fn form_parts(&mut self, params: Option<&Value>, place: &str) -> Vec<FormPart> {
        let mut files = false;
        let mut parts = Vec::new();
        for entry in params.and_then(Value::as_array).into_iter().flatten() {
            let name = string(entry.get("name"));
            if name.trim().is_empty() {
                continue;
            }
            let file = entry.get("type").and_then(Value::as_str) == Some("file");
            files |= file;
            let value = if file {
                string(entry.get("fileName"))
            } else {
                self.fix(&string(entry.get("value")), place)
            };
            parts.push(FormPart {
                name: name.trim().to_owned(),
                value,
                file,
                content_type: None,
                enabled: !is_disabled(entry),
            });
        }
        if files {
            self.c.warn(format!(
                "{place} uploads files: check their paths, which are from the computer the \
                 collection was exported on."
            ));
        }
        parts
    }

    /// The credentials of an `authentication` object; empty means inherit.
    fn credentials(&mut self, auth: Option<&Value>, place: &str) -> Credentials {
        let Some(auth) = auth.filter(|a| a.get("type").is_some()) else {
            return Credentials::default();
        };
        let param = |key: &str| string(auth.get(key));
        let kind = param("type");
        let spec = if is_disabled(auth) {
            AuthSpec::NoAuth
        } else {
            match kind.as_str() {
                "none" => AuthSpec::NoAuth,
                "bearer" => {
                    let prefix = param("prefix");
                    let prefix = prefix.trim();
                    if !prefix.is_empty() && !prefix.eq_ignore_ascii_case("bearer") {
                        self.c.warn(format!(
                            "{} sends its token after “{prefix}”: Tortuga's Bearer auth sends \
                             it after “Bearer”.",
                            crate::error::sentence(place)
                        ));
                    }
                    AuthSpec::Bearer {
                        token: self.fix(&param("token"), place),
                    }
                }
                "basic" => AuthSpec::Basic {
                    username: self.fix(&param("username"), place),
                    password: self.fix(&param("password"), place),
                },
                "apikey" if param("addTo") == "cookie" => {
                    AuthSpec::Other("cookie API key".to_owned())
                }
                "apikey" => AuthSpec::ApiKey {
                    key: self.fix(&param("key"), place),
                    value: self.fix(&param("value"), place),
                    in_query: param("addTo") == "queryParams",
                },
                // Insomnia keeps the token it got apart from the request.
                "oauth2" => AuthSpec::OAuth2 {
                    access_token: String::new(),
                },
                "iam" => AuthSpec::Other("AWS IAM".to_owned()),
                "ntlm" => AuthSpec::Other("NTLM".to_owned()),
                "hawk" => AuthSpec::Other("Hawk".to_owned()),
                "asap" => AuthSpec::Other("ASAP".to_owned()),
                other => AuthSpec::Other(other.to_owned()),
            }
        };
        self.c.credentials(spec, place)
    }

    /// Cookies off when Insomnia neither sends nor stores them for the request;
    /// redirects are set for the whole project in Tortuga.
    fn apply_settings(&mut self, item: &Value, request: &mut Request) {
        let flag = |v4: &str, v5: &str| self.format.field(item, v4, v5).and_then(Value::as_bool);
        let send = flag("settingSendCookies", "/settings/cookies/send");
        let store = flag("settingStoreCookies", "/settings/cookies/store");
        if send == Some(false) || store == Some(false) {
            request.cookies = false;
        }
        let redirects = string(self.format.field(
            item,
            "settingFollowRedirects",
            "/settings/followRedirects",
        ));
        if redirects == "off" {
            self.c.warn(format!(
                "“{}” doesn't follow redirects in Insomnia: in Tortuga that is set for the \
                 whole project (Defaults).",
                request.name
            ));
        }
    }

    /// A script in Tortuga's API, or kept as comments when it can't be translated.
    fn script(&mut self, code: &str) -> String {
        if code.trim().is_empty() {
            return String::new();
        }
        self.c
            .translated_script(translate_insomnia(code), ImportKind::Insomnia)
    }
}

/// `{{ _.name }}`, `{{_['name']}}` and `{{ name }}` as `{{name}}`.
fn variables_fixed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let inner = after[..end].trim();
        let name = inner
            .strip_prefix("_.")
            .or_else(|| {
                inner
                    .strip_prefix("_[")
                    .and_then(|s| s.strip_suffix(']'))
                    .map(|s| s.trim_matches(['\'', '"']))
            })
            .unwrap_or(inner);
        out.push_str(&rest[..start]);
        out.push_str("{{");
        out.push_str(name.trim());
        out.push_str("}}");
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// `data` as (name, value) pairs, nested objects' values as `parent.child`.
fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&name, value, out);
            }
        }
        other if !prefix.is_empty() => out.push((prefix.to_owned(), string(Some(other)))),
        _ => {}
    }
}

fn item_name(item: &Value) -> Option<&str> {
    item.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
}

fn is_disabled(value: &Value) -> bool {
    value.get("disabled").and_then(Value::as_bool) == Some(true)
}

/// “a”, “b” and “c”.
fn quoted_list(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("“{n}”")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => quoted.join(""),
    }
}
