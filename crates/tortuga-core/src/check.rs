//! `tortuga-cli check` (R-CLI-5, R-CLI-6, D76): mistakes in a project and credentials in
//! its shared files, found without sending anything.

pub mod credential;
pub mod request;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::collection::{Broken, Folder, Item, RequestItem};
use crate::model::{Auth, KeyValue};
use crate::storage::Storage;
use crate::workflow::path_text;
use crate::workspace::WorkspaceFile;
use crate::{Collection, vars};
use credential::{Credential, credential_in, credential_in_auth};

/// One thing `check` reports: `<file>: <place>: <message>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Relative to the project, with `/`.
    pub file: String,
    /// Where in the file (`header Authorization`, `step 2`); empty for the whole file.
    pub place: String,
    pub kind: Kind,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.place.is_empty() {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}: {}: {}", self.file, self.place, self.message)
        }
    }
}

impl Finding {
    /// The text an allow-list entry names this finding's place with (R-CLI-6).
    pub fn allow_entry(&self) -> String {
        format!("{}: {}", self.file, self.place)
    }
}

/// What kind of finding it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A file that does not read (R-CLI-5).
    Unreadable,
    /// A workflow step naming a request that does not exist (R-CLI-5).
    MissingRequest,
    /// A `{{variable}}` with no value in an environment (R-CLI-5).
    UndefinedVariable,
    /// A value that looks like a credential (R-CLI-6).
    Credential(Credential),
    /// An allow-list entry that matches nothing (R-CLI-6).
    UnusedAllow,
}

/// Checks `collection` with `workspace` (read from `workspace_path`, if any): every
/// finding, in the order of the project's tree.
pub fn check(
    storage: &dyn Storage,
    collection: &Collection,
    workspace: &WorkspaceFile,
    workspace_path: Option<&Path>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    unreadable(collection, &mut out);
    missing_requests(collection, &mut out);
    undefined_variables(storage, collection, workspace, &mut out);
    let mut credentials = Vec::new();
    project_credentials(collection, &mut credentials);
    if let Some(path) = workspace_path {
        let file = path_text(&crate::workspace::relative_path(&collection.root, path));
        workspace_credentials(&file, workspace, &mut credentials);
    }
    let allow = &collection.manifest.check.allow;
    let allowed = |f: &Finding| allow.iter().any(|a| a.trim() == f.allow_entry());
    for entry in allow {
        if !credentials.iter().any(|f| entry.trim() == f.allow_entry()) {
            out.push(Finding {
                file: crate::collection::MANIFEST_FILE.to_owned(),
                place: "check.allow".to_owned(),
                kind: Kind::UnusedAllow,
                message: format!("allowed place not found: {entry}"),
            });
        }
    }
    out.extend(credentials.into_iter().filter(|f| !allowed(f)));
    out
}

fn finding(
    file: &str,
    place: impl Into<String>,
    kind: Kind,
    message: impl Into<String>,
) -> Finding {
    Finding {
        file: file.to_owned(),
        place: place.into(),
        kind,
        message: message.into(),
    }
}

fn unreadable(collection: &Collection, out: &mut Vec<Finding>) {
    let broken = |b: &Broken| finding(&path_text(&b.path), "", Kind::Unreadable, &b.error);
    let mut items = Vec::new();
    walk(&collection.items, &mut |item| items.push(item));
    for item in items {
        match item {
            Item::Broken(b) => out.push(broken(b)),
            Item::Folder(f) => {
                if let Some(error) = &f.error {
                    out.push(finding(&folder_file(f), "", Kind::Unreadable, error));
                }
            }
            Item::Request(_) => {}
        }
    }
    out.extend(collection.broken_environments.iter().map(broken));
    out.extend(collection.broken_workflows.iter().map(broken));
}

fn missing_requests(collection: &Collection, out: &mut Vec<Finding>) {
    for workflow in &collection.workflows {
        let file = workflow_file(&workflow.id);
        for (i, step) in workflow.steps.iter().enumerate() {
            if collection.request(&step.request_path()).is_none() {
                out.push(finding(
                    &file,
                    format!("step {}", i + 1),
                    Kind::MissingRequest,
                    format!("names {}, which does not exist", step.request),
                ));
            }
        }
    }
}

/// Every `{{variable}}` of every request that some environment gives no value.
fn undefined_variables(
    storage: &dyn Storage,
    collection: &Collection,
    workspace: &WorkspaceFile,
    out: &mut Vec<Finding>,
) {
    let mut set_at_run = set_by_project(collection);
    set_at_run.extend(data_columns(storage, collection, out));
    set_at_run.extend(collection.manifest.secrets.iter().cloned());
    set_at_run.extend(workspace.variables.secrets.iter().cloned());
    let mut names: Vec<String> = collection
        .environments
        .iter()
        .map(|e| e.name.clone())
        .collect();
    for e in &workspace.environments {
        if !names.iter().any(|n| e.is_named(n)) {
            names.push(e.name.clone());
        }
    }
    let environments: Vec<(String, Option<crate::Environment>, BTreeSet<String>)> = if names
        .is_empty()
    {
        vec![(String::new(), None, BTreeSet::new())]
    } else {
        names
            .into_iter()
            .filter_map(|name| {
                let env = crate::workspace::environment_named(collection, workspace, &name).ok()?;
                let secrets = workspace
                    .environments
                    .iter()
                    .find(|e| e.is_named(&name))
                    .map(|e| e.secrets.clone())
                    .unwrap_or_default();
                Some((name, Some(env), secrets))
            })
            .collect()
    };
    let mut requests = Vec::new();
    walk(&collection.items, &mut |item| {
        if let Item::Request(r) = item {
            requests.push(r);
        }
    });
    for item in requests {
        // Variable → the environments it has no value in, in their order.
        let mut missing: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for (name, env, secrets) in &environments {
            let ctx = vars::Context::for_request_in(collection, env.as_ref(), Some(&item.path))
                .with_workspace_defaults(&workspace.defaults)
                .with_workspace(&workspace.variables.values, &workspace.variables.secrets);
            for var in vars::undefined_in(&item.request, &ctx) {
                if !set_at_run.contains(&var) && !secrets.contains(&var) {
                    missing.entry(var).or_default().push(name);
                }
            }
        }
        for (var, envs) in missing {
            let message = if envs.iter().all(|e| e.is_empty()) {
                "has no value".to_owned()
            } else {
                format!("has no value in {}", envs.join(", "))
            };
            out.push(finding(
                &path_text(&item.path),
                format!("{{{{{var}}}}}"),
                Kind::UndefinedVariable,
                message,
            ));
        }
    }
}

/// Names that captures and scripts set while the project runs (R-VAR-11);
/// `workspace.name` also as `name` (R-WS-8).
fn set_by_project(collection: &Collection) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for name in vars::set_by::set_by(collection).into_keys() {
        if let (true, own) = crate::runner::target(&name) {
            names.insert(own.to_owned());
        }
        names.insert(name);
    }
    names
}

/// The columns of the workflows' data files (R-FLOW-4); a file that does not read is a
/// finding.
fn data_columns(
    storage: &dyn Storage,
    collection: &Collection,
    out: &mut Vec<Finding>,
) -> BTreeSet<String> {
    let mut columns = BTreeSet::new();
    for workflow in &collection.workflows {
        let Some(data) = &workflow.data else {
            continue;
        };
        let rows = storage
            .read_to_string(&collection.root.join(data))
            .map_err(|e| e.to_string())
            .and_then(|text| crate::data::parse(data, &text));
        match rows {
            Ok(rows) => columns.extend(rows.into_iter().flat_map(BTreeMap::into_keys)),
            Err(e) => out.push(finding(
                &workflow_file(&workflow.id),
                "data",
                Kind::Unreadable,
                format!("{data}: {e}"),
            )),
        }
    }
    columns
}

/// Credentials in the project's own files (R-CLI-6). Examples are not scanned (D76).
fn project_credentials(collection: &Collection, out: &mut Vec<Finding>) {
    let manifest = &collection.manifest;
    let secrets: BTreeSet<&str> = manifest.secrets.iter().map(String::as_str).collect();
    let file = crate::collection::MANIFEST_FILE;
    variable_credentials(file, "", &manifest.variables, &|n| secrets.contains(n), out);
    header_credentials(file, &manifest.headers, out);
    auth_credentials(file, manifest.auth.as_ref(), out);
    for env in &collection.environments {
        let file = path_text(
            &Path::new(crate::collection::ENVIRONMENTS_DIR).join(format!("{}.toml", env.id)),
        );
        variable_credentials(&file, "", &env.variables, &|n| secrets.contains(n), out);
    }
    let mut items = Vec::new();
    walk(&collection.items, &mut |item| items.push(item));
    for item in items {
        match item {
            Item::Folder(f) => {
                let file = folder_file(f);
                variable_credentials(
                    &file,
                    "",
                    &f.defaults.variables,
                    &|n| secrets.contains(n),
                    out,
                );
                header_credentials(&file, &f.defaults.headers, out);
                auth_credentials(&file, f.defaults.auth.as_ref(), out);
            }
            Item::Request(r) => request_credentials(r, out),
            Item::Broken(_) => {}
        }
    }
}

fn request_credentials(item: &RequestItem, out: &mut Vec<Finding>) {
    let file = path_text(&item.path);
    for flag in request::flags(&item.request) {
        out.push(credential_finding(&file, flag.place, flag.found.kind));
    }
}

/// Credentials in the workspace file's shared values (R-CLI-6).
fn workspace_credentials(file: &str, workspace: &WorkspaceFile, out: &mut Vec<Finding>) {
    let secrets = &workspace.variables.secrets;
    variable_credentials(
        file,
        "",
        &workspace.variables.values,
        &|n| secrets.contains(n),
        out,
    );
    header_credentials(file, &workspace.defaults.headers, out);
    auth_credentials(file, Some(&workspace.defaults.auth), out);
    for env in &workspace.environments {
        let prefix = format!("environment {}: ", env.name);
        let secret = |n: &str| env.secrets.contains(n) || secrets.contains(n);
        variable_credentials(file, &prefix, &env.variables, &secret, out);
    }
}

fn variable_credentials(
    file: &str,
    prefix: &str,
    variables: &BTreeMap<String, String>,
    is_secret: &dyn Fn(&str) -> bool,
    out: &mut Vec<Finding>,
) {
    for (name, value) in variables.iter().filter(|(n, _)| !is_secret(n)) {
        value_credential(file, format!("{prefix}variable {name}"), value, out);
    }
}

fn header_credentials(file: &str, headers: &[KeyValue], out: &mut Vec<Finding>) {
    for h in headers {
        value_credential(file, format!("header {}", h.name), &h.value, out);
    }
}

fn auth_credentials(file: &str, auth: Option<&Auth>, out: &mut Vec<Finding>) {
    for (_, place, field, value) in request::auth_fields(auth) {
        if let Some(kind) = credential_in_auth(field, value) {
            out.push(credential_finding(file, place, kind));
        }
    }
}

fn value_credential(file: &str, place: impl Into<String>, value: &str, out: &mut Vec<Finding>) {
    if let Some(kind) = credential_in(value) {
        out.push(credential_finding(file, place, kind));
    }
}

fn credential_finding(file: &str, place: impl Into<String>, kind: Credential) -> Finding {
    finding(file, place, Kind::Credential(kind), kind.describe())
}

/// Calls `f` for every item of the tree, parents before children.
fn walk<'a>(items: &'a [Item], f: &mut dyn FnMut(&'a Item)) {
    for item in items {
        f(item);
        if let Item::Folder(folder) = item {
            walk(&folder.items, f);
        }
    }
}

fn folder_file(folder: &Folder) -> String {
    path_text(&folder.path.join(crate::collection::FOLDER_FILE))
}

fn workflow_file(id: &str) -> String {
    format!("{}/{id}.toml", crate::workflow::WORKFLOWS_DIR)
}
