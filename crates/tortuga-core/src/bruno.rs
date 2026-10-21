//! Importing a Bruno collection, a folder of `.bru` files, as a new project (R-COL-31,
//! D34, D59).
//!
//! The collection becomes:
//! - its folders and requests, in the order of their `seq`;
//! - the headers, auth, variables and docs of `collection.bru` and `folder.bru` as the
//!   project's and folders' (R-COL-11, R-COL-22); auth as the Postman import maps it
//!   ([`crate::import::auth`]);
//! - a variable that starts request URLs (`{{baseUrl}}/users`) as a server (D36);
//! - the environments in `environments/`, whose `vars:secret` names become secret
//!   variables; Bruno keeps their values on its user's computer, so they are asked for
//!   after the import (D35);
//! - `{{process.env.NAME}}` as the secret variable `NAME`, with its value from the
//!   collection's `.env` file for this machine's secret store;
//! - a request's `assert` lines as tests, and the `vars:post-response` values it reads
//!   from the response as captures (R-TEST-4) ([`assert`]); its own variables
//!   (`vars:pre-request`) as `vars.set(…)` in its pre-request script;
//! - scripts kept as comments to port by hand: Bruno's `bru` API is not Tortuga's (D28).
//!
//! What can't be imported as written is reported in [`Import::warnings`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use crate::collection::NewItem;
use crate::format::{FolderDefaults, Manifest};
use crate::import::auth::{AuthSpec, Credentials, Inherited};
use crate::import::{
    Collected, EnvValue, Import, ImportKind, ImportedEnvironment, graphql_body, short_name,
};
use crate::model::{FormPart, HttpMethod, KeyValue, Request, RequestBody, Scripts};
use crate::storage::Storage;
use crate::{Error, Result};

mod assert;
mod lang;

use lang::{BruFile, Pair};

/// The file that makes a folder a Bruno collection.
pub const COLLECTION_FILE: &str = "bruno.json";

/// The block names of the methods a request can have.
const METHODS: [&str; 9] = [
    "get", "post", "put", "delete", "patch", "options", "head", "connect", "trace",
];

/// Whether `dir` is a Bruno collection.
pub fn is_collection(storage: &dyn Storage, dir: &Path) -> bool {
    storage.exists(&dir.join(COLLECTION_FILE))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidBruno(message.into())
}

/// The error for a file given where a Bruno collection's folder was expected.
pub(crate) fn not_a_folder_error() -> Error {
    invalid("a Bruno collection is a folder: choose the folder that has its bruno.json")
}

/// Reads the Bruno collection in `dir` into a project (R-COL-31).
pub fn read(storage: &dyn Storage, dir: &Path) -> Result<Import> {
    let config_path = dir.join(COLLECTION_FILE);
    if !storage.exists(&config_path) {
        return Err(invalid(format!(
            "{} has no {COLLECTION_FILE}: choose the folder of a Bruno collection",
            dir.display()
        )));
    }
    let text = storage
        .read_to_string(&config_path)
        .map_err(|e| invalid(format!("{}: {e}", config_path.display())))?;
    let config: Value = serde_json::from_str(&text)
        .map_err(|e| invalid(format!("{COLLECTION_FILE} is not valid JSON: {e}")))?;
    let ignore = config.get("ignore").and_then(Value::as_array).map_or_else(
        || vec!["node_modules".to_owned()],
        |list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        },
    );
    let importer = Importer {
        storage,
        root: dir,
        ignore,
        c: Collected::default(),
        process_env: BTreeSet::new(),
    };
    Ok(importer.run(&config))
}

/// Reads a Bruno environment file (`environments/<name>.bru`) as the environment
/// `name` (R-COL-31): its enabled `vars`, and its `vars:secret` names as secret
/// variables without a value, since Bruno keeps those on its user's computer.
pub fn parse_environment(text: &str, name: &str) -> Result<ImportedEnvironment> {
    let mut process_env = BTreeSet::new();
    let text = without_process_env(text, &mut process_env);
    let file = lang::parse(&text)
        .map_err(|e| Error::InvalidBrunoEnvironment(format!("{name}.bru, {e}")))?;
    if file.block("vars").is_none() && file.block("vars:secret").is_none() {
        return Err(Error::InvalidBrunoEnvironment(format!(
            "{name}.bru has no vars: is it a Bruno environment?"
        )));
    }
    let secrets: Vec<&str> = file
        .list("vars:secret")
        .iter()
        .filter(|s| !s.starts_with('~'))
        .map(String::as_str)
        .collect();
    let mut values: Vec<EnvValue> = file
        .dict("vars")
        .iter()
        .filter(|p| p.enabled)
        .map(|p| EnvValue {
            key: p.key.clone(),
            value: p.value.clone(),
            secret: secrets.contains(&p.key.as_str()),
        })
        .collect();
    let names = secrets
        .iter()
        .map(|s| (*s).to_owned())
        .chain(process_env)
        .collect::<Vec<_>>();
    for key in names {
        if !values.iter().any(|v| v.key == key) {
            values.push(EnvValue {
                key,
                value: String::new(),
                secret: true,
            });
        }
    }
    Ok(ImportedEnvironment {
        name: name.to_owned(),
        values,
        warnings: Vec::new(),
    })
}

/// `{{process.env.NAME}}` as `{{NAME}}`; the names go to `names`.
fn without_process_env(text: &str, names: &mut BTreeSet<String>) -> String {
    const PREFIX: &str = "{{process.env.";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let after = &rest[at + PREFIX.len()..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        out.push_str(&rest[..at]);
        if name.is_empty() {
            out.push_str(&rest[at..at + PREFIX.len() + end + 2]);
        } else {
            out.push_str("{{");
            out.push_str(name);
            out.push_str("}}");
            names.insert(name.to_owned());
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The `NAME=value` lines of a `.env` file; `#` comments, `export` and quotes allowed.
fn dotenv(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let unquoted = ['"', '\'']
            .into_iter()
            .find_map(|q| value.strip_prefix(q).and_then(|v| v.strip_suffix(q)));
        out.insert(name.trim().to_owned(), unquoted.unwrap_or(value).to_owned());
    }
    out
}

/// An item of a folder, with what orders it.
struct Found {
    seq: Option<i64>,
    is_folder: bool,
    name: String,
    item: NewItem,
}

struct Importer<'a> {
    storage: &'a dyn Storage,
    root: &'a Path,
    /// Folder names `bruno.json` says to skip.
    ignore: Vec<String>,
    c: Collected,
    /// Names read with `{{process.env.NAME}}`.
    process_env: BTreeSet<String>,
}

impl Importer<'_> {
    fn run(mut self, config: &Value) -> Import {
        let name = config
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| folder_name(self.root), str::to_owned);
        let mut manifest = Manifest::new(name);
        self.config_warnings(config);
        let collection = self.optional_file(&self.root.join("collection.bru"));
        manifest.description = docs(&collection);
        manifest.headers = key_values(collection.dict("headers"));
        self.c.variables = variables(collection.dict("vars:pre-request"));
        self.level_scripts(&collection, "the collection");
        let creds = self.credentials(
            &collection,
            collection.get("auth", "mode").unwrap_or("none"),
            "the collection",
        );
        let top = Inherited::top(creds, &mut manifest);
        let items = self.items(self.root, &top);
        let environments = self.environments();
        let values = self.dotenv_values();
        for name in std::mem::take(&mut self.process_env) {
            if let Some(value) = values.get(&name).filter(|v| !v.is_empty()) {
                self.c.secret_values.insert(name.clone(), value.clone());
            }
            self.c.secrets.insert(name);
        }
        self.c
            .finish(ImportKind::Bruno, manifest, items)
            .with_environments(&environments)
    }

    /// What `bruno.json` sets that Tortuga keeps elsewhere or doesn't have.
    fn config_warnings(&mut self, config: &Value) {
        let has_certificates = config
            .pointer("/clientCertificates/certs")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty());
        if has_certificates {
            self.c.warn(
                "The collection's client certificates were not imported: Tortuga does not \
                 send client certificates yet.",
            );
        }
        let proxy = config.get("proxy");
        let proxy_on = proxy
            .and_then(|p| p.get("enabled"))
            .is_some_and(|e| e.as_bool() == Some(true) || e.as_str() == Some("true"));
        if proxy_on {
            self.c.warn(
                "The collection's proxy was not imported: set a proxy on this computer in \
                 Settings → Requests.",
            );
        }
    }

    /// A `.bru` file with `{{process.env.…}}` rewritten, or why it can't be read.
    fn file(&mut self, path: &Path) -> std::result::Result<BruFile, String> {
        let text = self
            .storage
            .read_to_string(path)
            .map_err(|e| e.to_string())?;
        let text = without_process_env(&text, &mut self.process_env);
        lang::parse(&text)
    }

    /// `collection.bru` or `folder.bru`: empty when there is none, and left out with a
    /// warning when it can't be read.
    fn optional_file(&mut self, path: &Path) -> BruFile {
        if !self.storage.exists(path) {
            return BruFile::default();
        }
        self.file(path).unwrap_or_else(|e| {
            let shown = self.relative(path);
            self.c.warn(format!(
                "{shown} could not be read ({e}), so its settings were left out."
            ));
            BruFile::default()
        })
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn level_scripts(&mut self, file: &BruFile, place: &str) {
        let has_code = ["script:pre-request", "script:post-response", "tests"]
            .into_iter()
            .any(|block| file.text(block).is_some());
        if has_code {
            self.c.warn(format!(
                "The scripts of {place} were left out: Tortuga runs scripts per request."
            ));
        }
    }

    /// The credentials of auth `mode`, with its values from the `auth:<mode>` block.
    fn credentials(&mut self, file: &BruFile, mode: &str, place: &str) -> Credentials {
        let block = format!("auth:{mode}");
        let param = |key: &str| file.get(&block, key).unwrap_or_default().to_owned();
        let spec = match mode {
            "inherit" => AuthSpec::Inherit,
            "none" => AuthSpec::NoAuth,
            "bearer" => AuthSpec::Bearer {
                token: param("token"),
            },
            "basic" => AuthSpec::Basic {
                username: param("username"),
                password: param("password"),
            },
            "apikey" => AuthSpec::ApiKey {
                key: param("key"),
                value: param("value"),
                in_query: param("placement") == "queryparams",
            },
            // Bruno gets the token itself; the file only says how.
            "oauth2" => AuthSpec::OAuth2 {
                access_token: String::new(),
            },
            other => AuthSpec::Other(other.to_owned()),
        };
        self.c.credentials(spec, place)
    }

    // ------------------------------------------------------------------ items

    /// The folders and requests in `dir`, in Bruno's order: by `seq`, then by name.
    fn items(&mut self, dir: &Path, inherited: &Inherited) -> Vec<NewItem> {
        let entries = match self.storage.list(dir) {
            Ok(entries) => entries,
            Err(e) => {
                let shown = self.relative(dir);
                self.c.warn(format!(
                    "{shown} could not be read ({e}), so it was left out."
                ));
                return Vec::new();
            }
        };
        let mut found = Vec::new();
        for entry in entries {
            let path = dir.join(&entry.name);
            if entry.is_dir {
                let skip = entry.name.starts_with('.')
                    || self.ignore.contains(&entry.name)
                    || (dir == self.root && entry.name == "environments");
                if !skip {
                    found.push(self.folder(&path, &entry.name, inherited));
                }
            } else if Path::new(&entry.name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("bru"))
                && !matches!(entry.name.as_str(), "folder.bru" | "collection.bru")
            {
                found.extend(self.request(&path, inherited));
            }
        }
        found.sort_by(|a, b| {
            (a.seq.is_none(), a.seq, !a.is_folder, &a.name).cmp(&(
                b.seq.is_none(),
                b.seq,
                !b.is_folder,
                &b.name,
            ))
        });
        found.into_iter().map(|f| f.item).collect()
    }

    fn folder(&mut self, dir: &Path, dir_name: &str, inherited: &Inherited) -> Found {
        let file = self.optional_file(&dir.join("folder.bru"));
        let name = file
            .get("meta", "name")
            .filter(|n| !n.is_empty())
            .unwrap_or(dir_name)
            .to_owned();
        let place = format!("folder “{name}”");
        self.level_scripts(&file, &place);
        let mut defaults = FolderDefaults {
            description: docs(&file),
            headers: key_values(file.dict("headers")),
            auth: None,
            variables: variables(file.dict("vars:pre-request")),
        };
        let mode = file.get("auth", "mode").unwrap_or("inherit");
        let creds = self.credentials(&file, mode, &place);
        let below = inherited.folder(creds, &mut defaults);
        let items = self.items(dir, &below);
        Found {
            seq: seq(&file),
            is_folder: true,
            name: name.clone(),
            item: NewItem::Folder {
                name,
                defaults,
                items,
            },
        }
    }

    fn request(&mut self, path: &Path, inherited: &Inherited) -> Option<Found> {
        let file = match self.file(path) {
            Ok(file) => file,
            Err(e) => {
                let shown = self.relative(path);
                self.c.warn(format!(
                    "{shown} could not be read ({e}), so it was left out."
                ));
                return None;
            }
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = short_name(
            file.get("meta", "name")
                .filter(|n| !n.is_empty())
                .unwrap_or(stem.as_str()),
        );
        match file.get("meta", "type") {
            None | Some("http" | "graphql") => {}
            Some(other) => {
                self.c.warn(format!(
                    "“{name}” is a {other} request, which Tortuga does not send, so it was left \
                     out."
                ));
                return None;
            }
        }
        let Some(block) = file
            .blocks
            .iter()
            .find(|b| METHODS.contains(&b.name.as_str()))
        else {
            self.c.warn(format!(
                "“{name}” has no method (get, post, …), so it was left out."
            ));
            return None;
        };
        let method_block = block.name.clone();
        let Ok(method) = method_block.parse::<HttpMethod>() else {
            self.c.warn(format!(
                "“{name}” was left out: Tortuga does not send {} requests.",
                method_block.to_ascii_uppercase()
            ));
            return None;
        };
        let http = |key: &str| file.get(&method_block, key);

        let mut request = Request::new(method, http("url").unwrap_or_default());
        request.name = name;
        request.description = docs(&file);
        if file.block("params:query").is_some() {
            request.params = key_values(file.dict("params:query"));
        }
        if let Some((server, rest)) = self.c.server_for(&request.url) {
            request.server = Some(server);
            request.url = rest;
        }
        request.sync_path_variables();
        for var in &mut request.path_vars {
            if let Some(value) = file.get("params:path", &var.name) {
                var.value = value.to_owned();
            }
        }
        request.headers = key_values(file.dict("headers"));
        let (body, content_type) = self.body(&file, http("body").unwrap_or("none"), &request.name);
        request.body = body;
        if let Some(content_type) = content_type
            && !request.has_header("content-type")
        {
            request.headers.push(content_type);
        }
        let place = format!("“{}”", request.name);
        let own = self.credentials(&file, http("auth").unwrap_or("none"), &place);
        inherited.apply(own, &mut request);
        self.apply_settings(&file, &mut request);
        self.checks_and_scripts(&file, &mut request);
        Some(Found {
            seq: seq(&file),
            is_folder: false,
            name: request.name.clone(),
            item: NewItem::Request(Box::new(request)),
        })
    }

    /// The body of `mode`, and a `Content-Type` for SPARQL. Bruno keeps the bodies of
    /// other modes in the file too; only the chosen one counts.
    fn body(
        &mut self,
        file: &BruFile,
        mode: &str,
        request: &str,
    ) -> (RequestBody, Option<KeyValue>) {
        let text = |block: &str| file.text(block).unwrap_or_default().to_owned();
        match mode {
            "none" | "" => (RequestBody::None, None),
            "json" => (RequestBody::Json(text("body:json")), None),
            "text" => (RequestBody::Text(text("body:text")), None),
            "xml" => (RequestBody::Xml(text("body:xml")), None),
            "sparql" => (
                RequestBody::Text(text("body:sparql")),
                Some(KeyValue::new("Content-Type", "application/sparql-query")),
            ),
            "formUrlEncoded" => (
                RequestBody::Form(key_values(file.dict("body:form-urlencoded"))),
                None,
            ),
            "multipartForm" => (
                RequestBody::Multipart(self.form_parts(file.dict("body:multipart-form"), request)),
                None,
            ),
            "graphql" => (
                graphql_body(&text("body:graphql"), &text("body:graphql:vars")),
                None,
            ),
            "file" => {
                self.c.warn(format!(
                    "The body of “{request}” is a file, which Tortuga can't send as a whole \
                     body yet, so it was left out."
                ));
                (RequestBody::None, None)
            }
            other => {
                self.c.warn(format!(
                    "The {other} body of “{request}” is not supported, so it was left out."
                ));
                (RequestBody::None, None)
            }
        }
    }

    /// Multipart parts: `name: text`, or `name: @file(path) @contentType(type)`.
    fn form_parts(&mut self, pairs: &[Pair], request: &str) -> Vec<FormPart> {
        let mut files = false;
        let parts = pairs
            .iter()
            .map(|pair| {
                let mut value = pair.value.trim();
                let mut content_type = None;
                if let Some(at) = value.rfind("@contentType(")
                    && let Some(inner) = value[at..].strip_prefix("@contentType(")
                    && let Some(inner) = inner.strip_suffix(')')
                {
                    content_type = Some(inner.trim().to_owned()).filter(|c| !c.is_empty());
                    value = value[..at].trim_end();
                }
                let file = value
                    .strip_prefix("@file(")
                    .and_then(|v| v.strip_suffix(')'));
                files |= file.is_some();
                FormPart {
                    name: pair.key.clone(),
                    // Bruno sends several files for `a|b`; a part here has one.
                    value: file
                        .map_or(value, |paths| paths.split('|').next().unwrap_or_default())
                        .to_owned(),
                    file: file.is_some(),
                    content_type,
                    enabled: pair.enabled,
                }
            })
            .collect();
        if files {
            self.c.warn(format!(
                "“{request}” uploads files: check their paths, which are relative to the Bruno \
                 collection's folder."
            ));
        }
        parts
    }

    /// A request's own `settings`: its timeout, and redirects, which Tortuga sets for
    /// the whole project.
    fn apply_settings(&mut self, file: &BruFile, request: &mut Request) {
        if let Some(timeout) = file
            .get("settings", "timeout")
            .and_then(|t| t.parse::<u64>().ok())
            .filter(|t| *t > 0)
        {
            request.timeout_ms = Some(timeout);
        }
        if file.get("settings", "followRedirects") == Some("false") {
            self.c.warn(format!(
                "“{}” doesn't follow redirects in Bruno: in Tortuga that is set for the whole \
                 project (Defaults).",
                request.name
            ));
        }
    }

    /// The request's variables, assertions, captures and scripts.
    fn checks_and_scripts(&mut self, file: &BruFile, request: &mut Request) {
        let mut scripts = Scripts::default();
        for pair in file.dict("vars:pre-request").iter().filter(|p| p.enabled) {
            let value = serde_json::Value::String(pair.value.clone()).to_string();
            let value = if pair.value.contains("{{") {
                format!("vars.replaceIn({value})")
            } else {
                value
            };
            let name = serde_json::Value::String(pair.key.clone()).to_string();
            // Writing to a String can't fail.
            let _ = writeln!(scripts.pre_request, "vars.set({name}, {value});");
        }
        if let Some(code) = file.text("script:pre-request") {
            scripts
                .pre_request
                .push_str(&self.c.commented_script(code, ImportKind::Bruno));
        }

        for pair in file.dict("vars:post-response").iter().filter(|p| p.enabled) {
            match assert::capture(&pair.value) {
                Some(capture) => {
                    request.capture.insert(pair.key.clone(), capture);
                }
                None => self.c.warn(format!(
                    "“{}” sets {} to {} after the response, which is no capture Tortuga has: \
                     set it with vars.set(…) in its tests.",
                    request.name, pair.key, pair.value
                )),
            }
        }
        let (tests, left) = assert::tests(file.dict("assert"));
        if !tests.is_empty() {
            self.c.translated += 1;
            scripts.tests.push_str(&tests);
        }
        if !left.is_empty() {
            self.c.warn(format!(
                "“{}” has assertions the import could not translate; they are kept as comments \
                 in its tests.",
                request.name
            ));
            scripts.tests.push_str("// Bruno assertions to port:\n");
            for line in left {
                // Writing to a String can't fail.
                let _ = writeln!(scripts.tests, "// {line}");
            }
        }
        for block in ["script:post-response", "tests"] {
            if let Some(code) = file.text(block) {
                scripts
                    .tests
                    .push_str(&self.c.commented_script(code, ImportKind::Bruno));
            }
        }
        request.scripts = scripts;
    }

    // ----------------------------------------------------------- environments

    /// The environments in `environments/`, by name.
    fn environments(&mut self) -> Vec<ImportedEnvironment> {
        let dir = self.root.join("environments");
        let Ok(mut entries) = self.storage.list(&dir) else {
            // No environments folder: the import keeps its "Default" environment.
            return Vec::new();
        };
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = Vec::new();
        for entry in entries.iter().filter(|e| !e.is_dir) {
            let Some(name) = entry.name.strip_suffix(".bru") else {
                continue;
            };
            let path = dir.join(&entry.name);
            // `{{process.env.…}}` names become the project's secrets, with `.env` values.
            let read = self
                .storage
                .read_to_string(&path)
                .map_err(|e| invalid(e.to_string()))
                .and_then(|text| {
                    let text = without_process_env(&text, &mut self.process_env);
                    parse_environment(&text, name)
                });
            match read {
                Ok(env) => out.push(env),
                Err(e) => self.c.warn(format!(
                    "The environment “{name}” was left out: {}.",
                    crate::error::message(&e)
                )),
            }
        }
        out
    }

    /// The values of the collection's `.env` file, for `{{process.env.…}}`.
    fn dotenv_values(&self) -> BTreeMap<String, String> {
        self.storage
            .read_to_string(&self.root.join(".env"))
            .map(|text| dotenv(&text))
            .unwrap_or_default()
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Bruno collection".to_owned())
}

fn seq(file: &BruFile) -> Option<i64> {
    file.get("meta", "seq").and_then(|s| s.trim().parse().ok())
}

fn docs(file: &BruFile) -> String {
    file.text("docs").unwrap_or_default().trim().to_owned()
}

fn key_values(pairs: &[Pair]) -> Vec<KeyValue> {
    pairs
        .iter()
        .map(|p| KeyValue {
            enabled: p.enabled,
            ..KeyValue::new(&p.key, &p.value)
        })
        .collect()
}

fn variables(pairs: &[Pair]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
