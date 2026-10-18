//! What an import becomes, and writing it as a new project (D34): from an OpenAPI
//! description (R-COL-15, [`crate::openapi`]), a Postman collection (R-COL-16,
//! [`crate::postman`]) or a `.http` file (R-COL-17, [`crate::http_file`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::collection::{Collection, NewItem};
use crate::format::{self, Environment, Manifest, Server};
use crate::model::Request;
use crate::storage::Storage;
use crate::{Result, http_file, openapi, postman};

/// What a file was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    OpenApi,
    Postman,
    /// A `.http` file of the VS Code REST Client or the JetBrains HTTP Client.
    Http,
}

impl ImportKind {
    /// "OpenAPI description", "Postman collection" or "HTTP file".
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenApi => "OpenAPI description",
            Self::Postman => "Postman collection",
            Self::Http => "HTTP file",
        }
    }

    /// The script API of the tool the file comes from, for the note above imported
    /// scripts.
    fn script_api(self) -> &'static str {
        match self {
            Self::OpenApi => "",
            Self::Postman => "Postman's `pm` API",
            Self::Http => "the HTTP Client's `client` API",
        }
    }
}

/// A project made from an imported file, ready for [`create_project`].
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// What could not be imported as written, as sentences for the user.
    pub warnings: Vec<String>,
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
    if doc.as_ref().is_some_and(postman::looks_like_collection) {
        return postman::parse(text);
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
/// values found for them (D35), scripts kept as comments, and warnings.
#[derive(Default)]
pub(crate) struct Collected {
    /// The project's variables.
    pub variables: BTreeMap<String, String>,
    servers: Vec<BaseUrl>,
    pub secrets: BTreeSet<String>,
    pub secret_values: BTreeMap<String, String>,
    /// Scripts kept as comments.
    pub scripts: usize,
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
        let mut url = value.unwrap_or_default();
        if url.is_empty() {
            self.warn(format!(
                "{{{{{variable}}}}} has no value in the file: set the base URL of server \
                 “{variable}” under Environments…."
            ));
        } else if !url.contains("://") {
            // Postman and the HTTP clients send a URL without a scheme over http.
            url = format!("http://{url}");
        }
        while url.ends_with('/') && !url.ends_with("://") {
            url.pop();
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
            warnings: self.warnings,
        }
    }

    fn final_warnings(&mut self, texts: &str, kind: ImportKind) {
        if let Some(start) = texts.find("{{$") {
            let name = texts[start + 2..]
                .split("}}")
                .next()
                .unwrap_or_default()
                .to_owned();
            self.warn(format!(
                "Dynamic variables such as {{{{{name}}}}} don't exist in Tortuga: set them with \
                 vars.set(…) in a pre-request script."
            ));
        }
        let api = kind.script_api().replace('`', "");
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
pub(crate) fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
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
