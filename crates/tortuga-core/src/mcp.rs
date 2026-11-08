//! The MCP server's tools (R-MCP-1 to R-MCP-4, D72 to D74): what an AI agent can do with a
//! project through the Model Context Protocol. Each tool is a plain function on
//! [`McpProject`] that opens the project from disk, so it always sees what is saved (and
//! never the app's unsaved edits), and returns text for the agent. [`server`] wraps them
//! in `rmcp`. Native only, like the mock server.

pub mod server;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

use crate::model::{BodyView, Request, Response};
use crate::runner::StepOutcome;
use crate::storage::Storage;
use crate::workflow::{self, RunOptions, Workflow, path_text};
use crate::workspace::{self, WorkspaceFile};
use crate::{Client, Collection, Error, Item, Result};

/// How much of a response body an agent gets (R-MCP-3), so one big answer doesn't fill its
/// context; the full size is stated.
pub const BODY_LIMIT: usize = 100 * 1024;

/// Secret values shorter than this are not hidden in what an agent gets: replacing every
/// `1` or `ok` in a response would garble it and hide nothing worth hiding.
const MIN_HIDDEN_LEN: usize = 4;

/// The project an MCP server serves, and how its requests are sent (R-MCP-1).
#[derive(Clone, Default)]
pub struct McpProject {
    /// The project's folder (the one with `tortuga.toml`).
    pub root: PathBuf,
    /// Shared environments, variables, settings, headers and auth (`--workspace`).
    pub workspace: WorkspaceFile,
    /// The environment a send uses when the tool names none (`--env`).
    pub environment: Option<String>,
    /// This machine's values for variables, e.g. secrets from `TORTUGA_VAR_<name>` (R-CLI-2).
    /// They win over the project's in a send and are never shown to the agent (P6).
    pub values: BTreeMap<String, String>,
}

impl std::fmt::Debug for McpProject {
    // Hand-written so `values`, which may hold secrets, never reach a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpProject")
            .field("root", &self.root)
            .field("environment", &self.environment)
            .field("values", &self.values.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// A send ready to run: the project with its environment, and one step for the request.
#[derive(Debug, Clone)]
pub struct PreparedSend {
    pub collection: Collection,
    pub workflow: Workflow,
    pub options: RunOptions,
}

impl McpProject {
    fn open(&self, storage: &dyn Storage) -> Result<Collection> {
        Collection::open(storage, &self.root)
    }

    /// The project's folders, requests, workflows and environments (R-MCP-2).
    pub fn list_project(&self, storage: &dyn Storage) -> Result<String> {
        Ok(crate::listing::to_json(
            &self.open(storage)?,
            &self.workspace,
        ))
    }

    /// A request as saved, with its examples (R-MCP-2): the object [`Self::save_request`] takes.
    pub fn get_request(&self, storage: &dyn Storage, path: &str) -> Result<String> {
        let collection = self.open(storage)?;
        let path = request_path(&collection, path)?;
        let request = &collection
            .request(&path)
            .ok_or_else(|| Error::NotFound(path.clone()))?
            .request;
        pretty(&json!({ "path": path_text(&path), "request": request }))
    }

    /// The environments with their servers' base URLs and their variables (R-MCP-2):
    /// secret variables by name only.
    pub fn list_environments(&self, storage: &dyn Storage) -> Result<String> {
        let collection = self.open(storage)?;
        let secrets = &collection.manifest.secrets;
        let mut names: Vec<&str> = collection
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for shared in &self.workspace.environments {
            if !names.iter().any(|n| shared.is_named(n)) {
                names.push(&shared.name);
            }
        }
        let servers = crate::collection::server_names(&collection.manifest);
        let environments: Vec<Value> = names
            .iter()
            .filter_map(|name| {
                workspace::environment_named(&collection, &self.workspace, name).ok()
            })
            .map(|env| {
                let urls: BTreeMap<&str, &str> = env
                    .servers
                    .iter()
                    .map(|(id, url)| {
                        (
                            servers.get(id).map_or(id.as_str(), String::as_str),
                            url.as_str(),
                        )
                    })
                    .collect();
                let variables: BTreeMap<&str, &str> = env
                    .variables
                    .iter()
                    .filter(|(name, _)| !secrets.contains(name))
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                json!({
                    "name": env.name,
                    "protected": env.protected,
                    "servers": urls,
                    "variables": variables,
                })
            })
            .collect();
        pretty(&json!({
            "default": self.environment,
            "environments": environments,
            "project_variables": collection.manifest.variables.iter()
                .filter(|(name, _)| !secrets.contains(name))
                .collect::<BTreeMap<_, _>>(),
            "secret_variables": secrets,
        }))
    }

    /// Gets a request ready to send (R-MCP-3) in `environment` (else the default), with
    /// `values` winning over the project's for this send only. Refused when the request
    /// may change data in a protected environment (R-SRV-9): there is no one to ask.
    pub fn prepare_send(
        &self,
        storage: &dyn Storage,
        path: &str,
        environment: Option<&str>,
        values: &BTreeMap<String, String>,
    ) -> Result<PreparedSend> {
        let mut collection = self.open(storage)?;
        let path = request_path(&collection, path)?;
        let workflow = Workflow::single(&collection, &path);
        let mut environment_id = None;
        if let Some(name) = environment.or(self.environment.as_deref()) {
            let env = workspace::environment_named(&collection, &self.workspace, name)?;
            let writes = workflow.writes(&collection);
            if env.protected && !writes.is_empty() {
                return Err(Error::InvalidOperation(format!(
                    "Environment {} is protected: an agent may only send GET, HEAD and OPTIONS \
                     requests to it, not {}. Send it from Tortuga instead.",
                    env.name,
                    writes.join(", ")
                )));
            }
            environment_id = Some(env.id.clone());
            collection = collection.with_environment(env);
        }
        let mut overrides = self.values.clone();
        overrides.extend(values.iter().map(|(k, v)| (k.clone(), v.clone())));
        let options = RunOptions {
            environment: environment_id,
            overrides,
            workspace: self.workspace.variables.values.clone(),
            secret_names: self.workspace.variables.secrets.clone(),
            workspace_defaults: self.workspace.defaults.clone(),
            ..RunOptions::default()
        };
        Ok(PreparedSend {
            collection,
            workflow,
            options,
        })
    }

    /// Adds a request to `folder` (`""`: the project's top level). Returns its path (R-MCP-4).
    pub fn create_request(
        &self,
        storage: &dyn Storage,
        folder: &str,
        request: &Value,
    ) -> Result<String> {
        let collection = self.open(storage)?;
        let folder = folder_path(&collection, folder)?;
        let request = parse_request(request)?;
        let path = collection.new_request(storage, &folder, &request)?;
        Ok(format!("Created {}", path_text(&path)))
    }

    /// Replaces a saved request with `request` (from [`Self::get_request`], changed); a new
    /// name renames the file and the workflow steps that use it (R-MCP-4).
    pub fn save_request(
        &self,
        storage: &dyn Storage,
        path: &str,
        request: &Value,
    ) -> Result<String> {
        let collection = self.open(storage)?;
        let path = request_path(&collection, path)?;
        let request = parse_request(request)?;
        let new_path = collection.save_request(storage, &path, &request)?;
        if new_path != path {
            collection.retarget_workflows(storage, &path, &new_path)?;
        }
        Ok(format!("Saved {}", path_text(&new_path)))
    }

    /// Adds a folder to `parent`. Returns its path (R-MCP-4).
    pub fn create_folder(&self, storage: &dyn Storage, parent: &str, name: &str) -> Result<String> {
        let collection = self.open(storage)?;
        let parent = folder_path(&collection, parent)?;
        let path = collection.new_folder(storage, &parent, non_empty(name)?)?;
        Ok(format!("Created {}", path_text(&path)))
    }

    /// Renames a folder, and the workflow steps in it (R-MCP-4).
    pub fn rename_folder(&self, storage: &dyn Storage, path: &str, name: &str) -> Result<String> {
        let collection = self.open(storage)?;
        let path = folder_path(&collection, path)?;
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidOperation(
                "The project's top level is not a folder that can be renamed.".into(),
            ));
        }
        let new_path = collection.rename_folder(storage, &path, non_empty(name)?)?;
        collection.retarget_workflows(storage, &path, &new_path)?;
        Ok(format!("Renamed to {}", path_text(&new_path)))
    }

    /// Moves a request or folder to the end of `folder`, and the workflow steps that use it
    /// (R-MCP-4).
    pub fn move_item(&self, storage: &dyn Storage, path: &str, folder: &str) -> Result<String> {
        let collection = self.open(storage)?;
        let path = item_path(&collection, path)?;
        let folder = folder_path(&collection, folder)?;
        let new_path = collection.move_to(storage, &path, &folder)?;
        collection.retarget_workflows(storage, &path, &new_path)?;
        Ok(format!("Moved to {}", path_text(&new_path)))
    }

    /// Deletes a request or a folder with everything in it (R-MCP-4).
    pub fn delete_item(&self, storage: &dyn Storage, path: &str) -> Result<String> {
        let collection = self.open(storage)?;
        let path = item_path(&collection, path)?;
        collection.delete(storage, &path)?;
        Ok(format!("Deleted {}", path_text(&path)))
    }
}

/// Sends a prepared request and words the outcome for the agent (R-MCP-3): status, time,
/// headers, body (at most [`BODY_LIMIT`]), test results, script errors and log lines.
/// `hidden` values (this machine's) are replaced by `{{name}}` everywhere in it (P6).
pub async fn send(
    client: &Client,
    prepared: &PreparedSend,
    hidden: &BTreeMap<String, String>,
) -> String {
    let result = workflow::run(
        client,
        &prepared.collection,
        &prepared.workflow,
        &prepared.options,
        |_| {},
    )
    .await;
    let outcome = result
        .iterations
        .first()
        .and_then(|i| i.steps.first())
        .map(|s| &s.outcome);
    let value = outcome.map_or_else(
        || json!({ "error": "The request was not sent." }),
        outcome_json,
    );
    pretty(&hide(value, hidden)).unwrap_or_else(|e| e.to_string())
}

fn outcome_json(outcome: &StepOutcome) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(sent) = outcome.shown() {
        out.insert(
            "sent".into(),
            json!({
                "method": sent.method.as_str(),
                "url": sent.url,
                "headers": sent.headers.iter().filter(|h| h.enabled)
                    .map(|h| json!({ "name": h.name, "value": h.value }))
                    .collect::<Vec<_>>(),
            }),
        );
    }
    match &outcome.response {
        Some(Ok(response)) => {
            out.insert("response".into(), response_json(response));
        }
        Some(Err(error)) => {
            out.insert("error".into(), json!(error));
        }
        None => {}
    }
    if !outcome.tests.is_empty() {
        out.insert("tests".into(), json!(outcome.tests));
    }
    if !outcome.errors.is_empty() {
        out.insert("script_errors".into(), json!(outcome.errors));
    }
    let console: Vec<String> = outcome
        .console
        .iter()
        .filter_map(|(_, e)| e.log_text())
        .collect();
    if !console.is_empty() {
        out.insert("console".into(), json!(console));
    }
    Value::Object(out)
}

fn response_json(response: &Response) -> Value {
    let body = match response.body_view() {
        BodyView::Empty => Value::Null,
        BodyView::Binary => json!(format!(
            "(binary, {} bytes{})",
            response.body.len(),
            response
                .content_type()
                .map(|t| format!(", {t}"))
                .unwrap_or_default()
        )),
        // The body as received, not pretty-printed: fewer tokens for the agent.
        BodyView::Json(_) | BodyView::Xml(_) | BodyView::Text(_) => {
            let text = String::from_utf8_lossy(&response.body);
            json!(cut(&text, BODY_LIMIT))
        }
    };
    let mut value = json!({
        "status": response.status,
        "status_text": response.status_text,
        "time_ms": response.elapsed.as_millis(),
        "headers": response.headers.iter()
            .map(|h| json!({ "name": h.name, "value": h.value }))
            .collect::<Vec<_>>(),
        "body": body,
    });
    if response.body.len() > BODY_LIMIT {
        value["body_truncated"] = json!(format!(
            "The body is {} bytes; only the first {} KB are shown.",
            response.body.len(),
            BODY_LIMIT / 1024
        ));
    }
    value
}

/// The first `max` bytes of `text`, cut at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    &text[..text.floor_char_boundary(max)]
}

/// `value` with every `hidden` value (long enough to matter) in its strings replaced by
/// `{{name}}`, longest first so a value inside another can't leave part of it (P6).
fn hide(value: Value, hidden: &BTreeMap<String, String>) -> Value {
    let mut secrets: Vec<(&str, &str)> = hidden
        .iter()
        .filter(|(_, v)| v.chars().count() >= MIN_HIDDEN_LEN)
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    secrets.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    hide_in(value, &secrets)
}

fn hide_in(value: Value, secrets: &[(&str, &str)]) -> Value {
    match value {
        Value::String(mut text) => {
            for (name, secret) in secrets {
                if text.contains(secret) {
                    text = text.replace(secret, &format!("{{{{{name}}}}}"));
                }
            }
            Value::String(text)
        }
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| hide_in(v, secrets)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, hide_in(v, secrets)))
                .collect(),
        ),
        other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => other,
    }
}

fn pretty(value: &Value) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|e| Error::InvalidOperation(e.to_string()))
}

fn non_empty(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidOperation("The name is empty.".into()));
    }
    Ok(name)
}

fn parse_request(value: &Value) -> Result<Request> {
    let mut request: Request = serde_json::from_value(value.clone()).map_err(|e| {
        Error::InvalidOperation(format!(
            "That is not a request ({e}). Use the \"request\" object get_request returns as \
             a model: name, method and url are required."
        ))
    })?;
    non_empty(&request.name)?;
    request.sync_path_variables();
    Ok(request)
}

/// A path an agent gave, relative to the project with `/`: never absolute and never out of
/// it with `..`. `""` and `.` are the project's top level.
fn relative(text: &str) -> Result<PathBuf> {
    let text = text.trim().replace('\\', "/");
    let path = Path::new(text.trim_end_matches('/'));
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::InvalidOperation(format!(
                    "'{text}' is not a path inside the project. Use paths as list_project gives them."
                )));
            }
        }
    }
    Ok(out)
}

/// A request's path, with or without `.toml`.
fn request_path(collection: &Collection, text: &str) -> Result<PathBuf> {
    let path = relative(text)?;
    let with_extension = path.with_extension("toml");
    [path, with_extension]
        .into_iter()
        .find(|p| collection.request(p).is_some())
        .ok_or_else(|| not_found(text, "request"))
}

/// A folder's path; `""` is the project's top level.
fn folder_path(collection: &Collection, text: &str) -> Result<PathBuf> {
    let path = relative(text)?;
    if path.as_os_str().is_empty() || matches!(collection.find(&path), Some(Item::Folder(_))) {
        Ok(path)
    } else {
        Err(not_found(text, "folder"))
    }
}

/// A request's or folder's path.
fn item_path(collection: &Collection, text: &str) -> Result<PathBuf> {
    request_path(collection, text).or_else(|_| {
        let path = relative(text)?;
        if path.as_os_str().is_empty() || collection.find(&path).is_none() {
            Err(not_found(text, "request or folder"))
        } else {
            Ok(path)
        }
    })
}

fn not_found(text: &str, what: &str) -> Error {
    Error::InvalidOperation(format!(
        "There is no {what} '{text}' in the project. list_project shows the paths."
    ))
}

#[cfg(test)]
mod tests;
