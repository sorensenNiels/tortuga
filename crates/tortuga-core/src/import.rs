//! What an import becomes, and writing it as a new project (D34): from an OpenAPI
//! description (R-COL-15, [`crate::openapi`]), a Postman collection (R-COL-16,
//! [`crate::postman`]), a `.http` file (R-COL-17, [`crate::http_file`]), a Bruno
//! collection (R-COL-31, [`crate::bruno`]) or an Insomnia export (R-COL-32,
//! [`crate::insomnia`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

use crate::collection::{Collection, NewItem};
use crate::format::{self, Environment, Manifest, Server};
use crate::model::{Request, RequestBody};
use crate::storage::Storage;
use crate::{Result, bruno, http_file, insomnia, openapi, postman};

pub(crate) mod auth;
mod environments;
pub(crate) mod js;

pub(crate) use environments::EnvValue;
pub use environments::{EnvironmentImport, ImportedEnvironment, import_environments};

/// What a file was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    OpenApi,
    Postman,
    /// A `.http` file of the VS Code REST Client or the JetBrains HTTP Client.
    Http,
    /// A Bruno collection: a folder of `.bru` files (R-COL-31).
    Bruno,
    /// An Insomnia export, v4 JSON or v5 YAML (R-COL-32).
    Insomnia,
}

impl ImportKind {
    /// Every importer, in the order menus list them.
    pub const ALL: [Self; 5] = [
        Self::OpenApi,
        Self::Postman,
        Self::Insomnia,
        Self::Http,
        Self::Bruno,
    ];

    /// Stable id, used in the settings file (R-COL-18).
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenApi => "openapi",
            Self::Postman => "postman",
            Self::Http => "http",
            Self::Bruno => "bruno",
            Self::Insomnia => "insomnia",
        }
    }

    /// What a file of this kind is called, e.g. "Postman collection".
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenApi => "OpenAPI description",
            Self::Postman => "Postman collection",
            Self::Http => "HTTP file",
            Self::Bruno => "Bruno collection",
            Self::Insomnia => "Insomnia export",
        }
    }

    /// The script API of the tool the file comes from, for the note above imported
    /// scripts.
    fn script_api(self) -> &'static str {
        match self {
            Self::OpenApi => "",
            Self::Postman => "Postman's `pm` API",
            Self::Http => "the HTTP Client's `client` API",
            Self::Bruno => "Bruno's `bru` API",
            Self::Insomnia => "Insomnia's `insomnia` API",
        }
    }
}

/// A project made from an imported file, ready for [`create_project`].
#[derive(Clone, PartialEq, Eq)]
pub struct Import {
    pub kind: ImportKind,
    pub manifest: Manifest,
    pub environments: Vec<Environment>,
    pub items: Vec<NewItem>,
    /// Number of requests.
    pub requests: usize,
    /// Values for secret variables found in the file (R-COL-16, D35): the app keeps
    /// them in this machine's secret store as the default for every environment.
    /// They are never written to the project.
    pub secret_values: BTreeMap<String, String>,
    /// Values for secret variables in one environment, from Postman environment files
    /// (R-COL-20): (environment id, variable name) → value. Kept like `secret_values`.
    pub environment_secret_values: BTreeMap<(String, String), String>,
    /// What could not be imported as written, as sentences for the user.
    pub warnings: Vec<String>,
}

// Hand-written: the secret values must not show.
impl std::fmt::Debug for Import {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Import")
            .field("kind", &self.kind)
            .field("manifest", &self.manifest)
            .field("environments", &self.environments)
            .field("items", &self.items)
            .field("requests", &self.requests)
            .field(
                "secret_values",
                &self.secret_values.keys().collect::<Vec<_>>(),
            )
            .field(
                "environment_secret_values",
                &self.environment_secret_values.keys().collect::<Vec<_>>(),
            )
            .field("warnings", &self.warnings)
            .finish()
    }
}

impl Import {
    /// Whether the secret variable `name` has a value from the file(s) for every
    /// environment: one for all of them, or one in each.
    pub fn has_secret_value(&self, name: &str) -> bool {
        self.secret_values.get(name).is_some_and(|v| !v.is_empty())
            || self.environments.iter().all(|e| {
                self.environment_secret_values
                    .get(&(e.id.clone(), name.to_owned()))
                    .is_some_and(|v| !v.is_empty())
            })
    }
}

/// Reads a Postman collection, a `.http` file or an OpenAPI description, whichever the
/// file `file_name` with `text` is. When it is none of them, the error is the one for
/// `expected`, the kind the user asked for.
pub fn parse(text: &str, file_name: &str, expected: ImportKind) -> Result<Import> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let doc = trimmed
        .starts_with('{')
        .then(|| serde_json::from_str::<Value>(trimmed).ok())
        .flatten();
    if insomnia::looks_like_export(trimmed, doc.as_ref()) {
        return insomnia::parse(text);
    }
    if doc.as_ref().is_some_and(postman::looks_like_collection) {
        return postman::parse(text);
    }
    if doc.as_ref().is_some_and(postman::looks_like_environment) {
        return Err(postman::environment_file_error());
    }
    if http_file::is_environment_file_name(file_name)
        || doc
            .as_ref()
            .is_some_and(|d| http_file::looks_like_environments(file_name, d))
    {
        return Err(http_file::environment_file_error());
    }
    let extension = Path::new(file_name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("http" | "rest")) || http_file::looks_like_http(text) {
        return http_file::parse(text, file_name);
    }
    match expected {
        ImportKind::OpenApi => openapi::parse(text),
        ImportKind::Postman => openapi::parse(text).or_else(|_| postman::parse(text)),
        ImportKind::Http => openapi::parse(text).or_else(|_| http_file::parse(text, file_name)),
        ImportKind::Bruno => Err(bruno::not_a_folder_error()),
        ImportKind::Insomnia => openapi::parse(text).or_else(|_| insomnia::parse(text)),
    }
}

/// Writes `import` as a new project in `root` and opens it (D34). Secret values are
/// not written here: they belong in the app's secret store.
pub fn create_project(storage: &dyn Storage, root: &Path, import: &Import) -> Result<Collection> {
    let created = Collection::create(storage, root, &import.manifest.name)?;
    created.save_config(storage, &import.manifest, &import.environments)?;
    Collection::open(storage, root)?.add_items(storage, Path::new(""), &import.items)?;
    Collection::open(storage, root)
}

/// A base URL variable that became a server (D36).
struct BaseUrl {
    variable: String,
    server: Server,
    url: String,
}

/// What the Postman (R-COL-16) and `.http` (R-COL-17) importers collect while they
/// read: variables, servers for base URL variables (D36), secret variables with the
/// values found for them (D35), counts of scripts translated (R-COL-19) and kept as
/// comments, and warnings.
#[derive(Default)]
pub(crate) struct Collected {
    /// The project's variables.
    pub variables: BTreeMap<String, String>,
    servers: Vec<BaseUrl>,
    pub secrets: BTreeSet<String>,
    pub secret_values: BTreeMap<String, String>,
    /// Scripts kept as comments.
    pub scripts: usize,
    /// Scripts translated to Tortuga's script API (R-COL-19).
    pub translated: usize,
    pub warnings: Vec<String>,
}

impl Collected {
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// `{{baseUrl}}/users` → the server for `baseUrl` and `/users`, when the variable
    /// holds a URL or is not defined (it lives in the other tool's environment then).
    pub fn server_for(&mut self, url: &str) -> Option<(String, String)> {
        let (variable, rest) = url.strip_prefix("{{")?.split_once("}}")?;
        let variable = variable.trim();
        if variable.is_empty()
            || variable.starts_with('$')
            || self.secrets.contains(variable)
            || !(rest.is_empty() || rest.starts_with(['/', '?', '#']))
        {
            return None;
        }
        let value = self.variables.get(variable).map(|v| v.trim().to_owned());
        if let Some(value) = &value
            && !(value.is_empty() || value.contains("://") || value.starts_with("localhost"))
        {
            return None;
        }
        let rest = rest.to_owned();
        if let Some(base) = self.servers.iter().find(|b| b.variable == variable) {
            return Some((base.server.id.clone(), rest));
        }
        let url = base_url(&value.unwrap_or_default());
        if url.is_empty() {
            self.warn(no_base_url(variable));
        }
        let servers: Vec<Server> = self.servers.iter().map(|b| b.server.clone()).collect();
        let server = Server {
            id: Collection::new_server_id(variable, &servers),
            name: variable.to_owned(),
        };
        let id = server.id.clone();
        self.servers.push(BaseUrl {
            variable: variable.to_owned(),
            server,
            url,
        });
        Some((id, rest))
    }

    /// Whether the variable `name` became a server's base URL (D36).
    pub fn is_server_variable(&self, name: &str) -> bool {
        self.servers.iter().any(|b| b.variable == name)
    }

    /// A credential as written to the project: `{{name}}` of a secret variable (D35).
    /// A value written in the file is kept for the secret store; one that uses
    /// variables stays as it is and those variables become secret.
    pub fn credential(&mut self, value: &str, name: &str) -> String {
        let value = value.trim();
        let used = variables_in(value);
        if !used.is_empty() {
            self.secrets.extend(used);
            return value.to_owned();
        }
        let mut candidate = name.to_owned();
        let mut n = 1;
        while !value.is_empty()
            && (self
                .secret_values
                .get(&candidate)
                .is_some_and(|v| v != value)
                || (self.variables.contains_key(&candidate) && !self.secrets.contains(&candidate)))
        {
            n += 1;
            candidate = format!("{name}_{n}");
        }
        if !value.is_empty() {
            self.secret_values
                .insert(candidate.clone(), value.to_owned());
        }
        self.secrets.insert(candidate.clone());
        format!("{{{{{candidate}}}}}")
    }

    /// A translated script under a note to check it, or one that couldn't be translated
    /// fully as comments to port; counted for the warnings (R-COL-19).
    pub fn translated_script(&mut self, translation: js::Translation, kind: ImportKind) -> String {
        match translation {
            js::Translation::Complete(code) => {
                self.translated += 1;
                format!("{}{}\n", translated_note(kind), code.trim_end())
            }
            js::Translation::Partial(code) => self.commented_script(&code, kind),
        }
    }

    /// `code` as comments under a note to port it, counted for the warning.
    pub fn commented_script(&mut self, code: &str, kind: ImportKind) -> String {
        self.scripts += 1;
        commented(code, kind)
    }

    /// The import: variables of base URLs that only the URLs used are dropped, secret
    /// variables' values move to the secret store, and one "Default" environment holds
    /// the servers' base URLs.
    pub fn finish(
        mut self,
        kind: ImportKind,
        mut manifest: Manifest,
        mut items: Vec<NewItem>,
    ) -> Import {
        let mut variables = std::mem::take(&mut self.variables);
        let texts: String = requests(&items)
            .into_iter()
            .map(|r| format::write_request(r, None))
            .chain(manifest.headers.iter().map(|h| h.value.clone()))
            .collect();
        for base in &self.servers {
            if !texts.contains(&format!("{{{{{}}}}}", base.variable)) {
                variables.remove(&base.variable);
            }
        }
        for name in &self.secrets {
            if let Some(value) = variables.remove(name)
                && !value.is_empty()
            {
                self.secret_values.entry(name.clone()).or_insert(value);
            }
        }
        strip_secrets(&mut items, &self.secrets, &mut self.secret_values);
        self.final_warnings(&texts, kind);

        let environment = Environment {
            id: Collection::new_environment_id("Default", &[]),
            name: "Default".into(),
            seq: Some(100),
            servers: self
                .servers
                .iter()
                .map(|b| (b.server.id.clone(), b.url.clone()))
                .collect(),
            ..Environment::default()
        };
        manifest.variables = variables;
        manifest.servers = self.servers.into_iter().map(|b| b.server).collect();
        manifest.secrets = self.secrets.into_iter().collect();
        Import {
            kind,
            manifest,
            environments: vec![environment],
            requests: requests(&items).len(),
            items,
            secret_values: self.secret_values,
            environment_secret_values: BTreeMap::new(),
            warnings: self.warnings,
        }
    }

    fn final_warnings(&mut self, texts: &str, kind: ImportKind) {
        // Tortuga fills in the dynamic variables the tools share (R-VAR-8), not the rest.
        let unknown = crate::vars::names(texts)
            .into_iter()
            .find(|n| n.starts_with('$') && !crate::vars::dynamic::is_dynamic(n));
        if let Some(name) = unknown {
            self.warn(format!(
                "Tortuga has no dynamic variable {{{{{name}}}}} (it knows $guid, $timestamp, \
                 $isoTimestamp and $randomInt): set it with vars.set(…) in a pre-request script."
            ));
        }
        let api = kind.script_api().replace('`', "");
        match self.translated {
            0 => {}
            1 => self.warn(format!(
                "1 script was translated from {api} to Tortuga's: check that it tests what it did."
            )),
            n => self.warn(format!(
                "{n} scripts were translated from {api} to Tortuga's: check that they test what \
                 they did."
            )),
        }
        match self.scripts {
            0 => {}
            1 => self.warn(format!(
                "1 script was kept as a comment: {api} is not Tortuga's, so port it to vars, \
                 test and expect."
            )),
            n => self.warn(format!(
                "{n} scripts were kept as comments: {api} is not Tortuga's, so port them to \
                 vars, test and expect."
            )),
        }
    }
}

/// A base URL variable's value as a server's base URL: without trailing slashes, and
/// with `http://` when it has no scheme, as Postman and the HTTP clients send it.
pub(crate) fn base_url(value: &str) -> String {
    let mut url = value.trim().to_owned();
    if !url.is_empty() && !url.contains("://") && !url.starts_with("{{") {
        url = format!("http://{url}");
    }
    while url.ends_with('/') && !url.ends_with("://") {
        url.pop();
    }
    url
}

/// The warning for a base URL variable without a value.
pub(crate) fn no_base_url(variable: &str) -> String {
    format!(
        "{{{{{variable}}}}} has no value in the file: set the base URL of server \
         “{variable}” under Environments…."
    )
}

/// A JSON or YAML document as JSON; the error says which it isn't.
pub(crate) fn json_or_yaml(text: &str) -> std::result::Result<Value, String> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') {
        return serde_json::from_str(text).map_err(|e| format!("it is not valid JSON: {e}"));
    }
    let docs = YamlLoader::load_from_str(text).map_err(|e| format!("it is not valid YAML: {e}"))?;
    let first = docs
        .into_iter()
        .next()
        .ok_or_else(|| "the file is empty".to_owned())?;
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

/// A JSON value as text; `null` and missing are empty.
pub(crate) fn json_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// A GraphQL query and its variables (JSON text) as the JSON body that sends them.
/// Variables that are not JSON are sent as the text they are.
pub(crate) fn graphql_body(query: &str, variables: &str) -> RequestBody {
    let variables = if variables.trim().is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(variables).unwrap_or_else(|_| Value::String(variables.to_owned()))
    };
    let json = serde_json::json!({ "query": query, "variables": variables });
    RequestBody::Json(serde_json::to_string_pretty(&json).unwrap_or_default())
}

/// Every request in `items`, depth first.
pub(crate) fn requests(items: &[NewItem]) -> Vec<&Request> {
    let mut out = Vec::new();
    for item in items {
        match item {
            NewItem::Request(request) => out.push(request.as_ref()),
            NewItem::Folder { items, .. } => out.extend(requests(items)),
        }
    }
    out
}

/// Takes secret variables out of folder defaults; their values go to the secret store.
fn strip_secrets(
    items: &mut [NewItem],
    secrets: &BTreeSet<String>,
    values: &mut BTreeMap<String, String>,
) {
    for item in items {
        if let NewItem::Folder {
            defaults, items, ..
        } = item
        {
            for name in secrets {
                if let Some(value) = defaults.variables.remove(name)
                    && !value.is_empty()
                {
                    values.entry(name.clone()).or_insert(value);
                }
            }
            strip_secrets(items, secrets, values);
        }
    }
}

/// The note put above an imported script, which is kept but not run.
pub(crate) fn script_note(kind: ImportKind) -> String {
    format!(
        "// Imported from the {} and not run: {} is not Tortuga's.\n// Port it to `vars`, \
         `req`, `res`, `test` and `expect`, then remove the `//`.\n",
        kind.label(),
        kind.script_api()
    )
}

/// The note above a script the import translated to Tortuga's API (R-COL-19).
pub(crate) fn translated_note(kind: ImportKind) -> String {
    format!(
        "// Translated from {} by the import: check that it tests what it did.\n",
        kind.script_api()
    )
}

/// The script as comments under [`script_note`].
fn commented(code: &str, kind: ImportKind) -> String {
    let mut out = script_note(kind);
    for line in code.trim_end().lines() {
        if line.trim().is_empty() {
            out.push_str("//\n");
        } else {
            out.push_str("// ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The names of the `{{variables}}` in `text`, without dynamic `{{$…}}` ones.
pub(crate) fn variables_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start + 2..].find("}}") else {
            break;
        };
        let name = rest[start + 2..start + 2 + len].trim();
        if !name.is_empty() && !name.starts_with('$') {
            out.push(name.to_owned());
        }
        rest = &rest[start + 2 + len + 2..];
    }
    out
}

/// Request names longer than this are cut (they also name the file).
const MAX_NAME: usize = 60;

/// `name` cut to at most [`MAX_NAME`] characters.
pub(crate) fn short_name(name: &str) -> String {
    if name.chars().count() <= MAX_NAME {
        name.to_owned()
    } else {
        let cut: String = name.chars().take(MAX_NAME - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

/// A variable name from any text: letters, digits and `_` (`X-API-Key` → `x_api_key`).
pub(crate) fn variable_name(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches('_');
    if name.is_empty() {
        "value".into()
    } else {
        name.to_owned()
    }
}

/// `base`, or `base 2`, `base 3`, … when it is taken.
pub fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..=usize::MAX)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(text: &str, expected: ImportKind) -> String {
        crate::error::message(&parse(text, "file.json", expected).expect_err("refused"))
    }

    /// R-COL-15, R-COL-16, R-COL-17: every format is read whichever was asked for; a
    /// file that is none of them gets the error of the one asked for.
    #[test]
    fn parse_tells_the_formats_apart() {
        let postman = r#"{"info": {"name": "P", "schema":
            "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"}, "item": []}"#;
        let openapi = r#"{"openapi": "3.0.0", "info": {"title": "O"}, "paths": {}}"#;
        let yaml = "openapi: 3.1.0\ninfo:\n  title: Y\npaths: {}\n";
        let http = "@host = https://a.test\n\n### Ping\nGET {{host}}/ping\n";
        for expected in [ImportKind::OpenApi, ImportKind::Postman, ImportKind::Http] {
            let kind = |text, name| parse(text, name, expected).expect("parse").kind;
            assert_eq!(kind(postman, "p.json"), ImportKind::Postman);
            assert_eq!(kind(openapi, "o.json"), ImportKind::OpenApi);
            assert_eq!(kind(yaml, "o.yaml"), ImportKind::OpenApi);
            assert_eq!(kind(http, "requests.txt"), ImportKind::Http);
        }
        assert!(error(r#"{"hello": 1}"#, ImportKind::OpenApi).contains("OpenAPI"));
        assert!(error(r#"{"hello": 1}"#, ImportKind::Postman).contains("Postman"));
        assert!(error("hello: 1", ImportKind::Postman).contains("JSON"));
        assert!(error("hello: 1", ImportKind::Http).contains("HTTP file"));
        let empty = parse("# nothing yet\n", "empty.http", ImportKind::OpenApi);
        assert!(matches!(empty, Err(crate::Error::InvalidHttpFile(_))));
    }

    #[test]
    fn helpers() {
        assert_eq!(variable_name("X-API-Key"), "x_api_key");
        assert_eq!(variable_name("--"), "value");
        assert_eq!(unique_name("a", |n| n == "a" || n == "a 2"), "a 3");
        assert_eq!(short_name("short"), "short");
        assert_eq!(short_name(&"x".repeat(80)).chars().count(), 60);
        assert_eq!(variables_in("Bearer {{a}} {{ b }} {{$guid}}"), ["a", "b"]);
        let note = script_note(ImportKind::Postman);
        assert!(note.starts_with("// Imported from the Postman collection and not run"));
        assert_eq!(
            commented("a\n\nb", ImportKind::Http),
            format!("{}// a\n//\n// b\n", script_note(ImportKind::Http))
        );
    }
}
