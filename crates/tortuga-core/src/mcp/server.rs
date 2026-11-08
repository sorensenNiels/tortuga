//! The MCP server (R-MCP-1, R-MCP-5, D74): [`McpProject`]'s tools over `rmcp`, on stdio
//! for `tortuga-cli mcp` and over HTTP for the app ([`http`]). Native only.

pub mod http;

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::Value;

use super::{McpAction, McpProject, SharedProjects};
use crate::format::names_match;
use crate::storage::FsStorage;
use crate::{Client, Error, Result};

/// Called with what an agent did (R-MCP-5), from any thread.
pub type OnAction = Arc<dyn Fn(McpAction) + Send + Sync>;

/// Stack for the thread a send runs on: the script engine needs more than tokio's
/// default 2 MB (the same as `tortuga-cli run`).
const SEND_STACK: usize = 8 * 1024 * 1024;

const INSTRUCTIONS: &str = "\
Tortuga is an API client whose projects are plain files in git. list_project shows the \
project's folders, requests (by path, relative to the project, \
with '/'), workflows and environments. get_request shows a request as saved; send_request \
sends it and returns the response and test results. Edits (create_request, save_request, \
create_folder, rename_folder, move_item, delete_item) change the project's files, which \
the user reviews in git like any change. Secret values are never shown: they appear as \
{{name}}. When the server has several projects, list_project shows them all and the other \
tools take the project's name in the project argument.";

/// Doc for the `project` argument, the same on every tool.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ProjectParams {
    /// The project's name, needed only when the server has several (`list_project` shows them).
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// A request's or folder's path as `list_project` gives it, e.g. "users/get-user.toml".
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The request's path as `list_project` gives it.
    pub path: String,
    /// An environment's name; without one, the server's default environment is used.
    #[serde(default)]
    pub environment: Option<String>,
    /// Variable values for this send only; they win over the project's.
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateRequestParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The folder to add it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub folder: String,
    /// The request, shaped like the "request" object `get_request` returns: name, method
    /// and url are required; params, headers, body, auth, scripts and examples may be given.
    pub request: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveRequestParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The request's path as `list_project` gives it.
    pub path: String,
    /// The whole request as `get_request` returned it, with the changes made. A new name
    /// renames the file.
    pub request: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateFolderParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The folder to add it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub parent: String,
    /// The new folder's name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameFolderParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The folder's path as `list_project` gives it.
    pub path: String,
    /// Its new name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MoveParams {
    /// The project's name, needed only when the server has several.
    #[serde(default)]
    pub project: Option<String>,
    /// The request's or folder's path as `list_project` gives it.
    pub path: String,
    /// The folder to move it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub folder: String,
}

/// The MCP server for the projects in `projects`: their tools and the client their
/// requests go out with.
#[derive(Clone)]
pub struct TortugaMcp {
    projects: SharedProjects,
    client: Client,
    on_action: OnAction,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for TortugaMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TortugaMcp")
            .field("projects", &self.projects)
            .finish_non_exhaustive()
    }
}

/// Serves `project` on stdin and stdout until the agent closes them (R-MCP-1).
pub async fn serve_stdio(project: McpProject, client: Client) -> Result<()> {
    let projects = Arc::new(RwLock::new(vec![project]));
    let service = TortugaMcp::new(projects, client, Arc::new(|_| {}))
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| Error::McpServer(e.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|e| Error::McpServer(e.to_string()))?;
    Ok(())
}

fn text(result: Result<String>) -> std::result::Result<String, String> {
    result.map_err(|e| e.to_string())
}

impl TortugaMcp {
    /// The project called `name`, or the only one.
    fn pick(&self, name: Option<&str>) -> std::result::Result<McpProject, String> {
        let projects = self
            .projects
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        let found = match name {
            None if projects.len() == 1 => projects.first(),
            None => None,
            Some(name) => projects.iter().find(|p| {
                names_match(&p.name, name)
                    || p.root
                        .file_name()
                        .is_some_and(|f| names_match(&f.to_string_lossy(), name))
            }),
        };
        found.cloned().ok_or_else(|| {
            let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
            match (name, names.is_empty()) {
                (_, true) => "No project is open in Tortuga.".to_owned(),
                (None, false) => format!(
                    "Name the project in \"project\": one of {}.",
                    names.join(", ")
                ),
                (Some(name), false) => {
                    format!(
                        "There is no project '{name}'. The projects are: {}.",
                        names.join(", ")
                    )
                }
            }
        })
    }

    /// Runs an edit on the project called `name` and logs it (R-MCP-5).
    fn edit(
        &self,
        name: Option<&str>,
        summary: &str,
        edit: impl FnOnce(&McpProject) -> Result<String>,
    ) -> std::result::Result<String, String> {
        let project = self.pick(name)?;
        let result = text(edit(&project));
        let (outcome, failed) = match &result {
            Ok(done) => (done.clone(), false),
            Err(e) => (e.clone(), true),
        };
        self.log(&project, summary.to_owned(), outcome, failed);
        result
    }

    fn log(&self, project: &McpProject, summary: String, outcome: String, failed: bool) {
        (self.on_action)(McpAction {
            at: std::time::SystemTime::now(),
            project: project.name.clone(),
            summary,
            outcome,
            failed,
        });
    }
}

#[tool_router]
impl TortugaMcp {
    pub fn new(projects: SharedProjects, client: Client, on_action: OnAction) -> Self {
        Self {
            projects,
            client,
            on_action,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List the project's folders and requests (with their paths and methods), workflows and environments, as JSON. With several projects and none named, lists them all."
    )]
    fn list_project(
        &self,
        Parameters(p): Parameters<ProjectParams>,
    ) -> std::result::Result<String, String> {
        if p.project.as_deref().is_some_and(|n| !n.trim().is_empty()) {
            return text(self.pick(p.project.as_deref())?.list_project(&FsStorage));
        }
        let projects = self
            .projects
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match projects.as_slice() {
            [] => Err("No project is open in Tortuga.".to_owned()),
            [one] => text(one.list_project(&FsStorage)),
            several => {
                let listings: std::result::Result<Vec<String>, String> = several
                    .iter()
                    .map(|p| text(p.list_project(&FsStorage)))
                    .collect();
                Ok(format!("[\n{}\n]", listings?.join(",\n")))
            }
        }
    }

    #[tool(
        description = "Show a request as saved: method, url, params, headers, body, auth, scripts, docs and example responses, as JSON."
    )]
    fn get_request(
        &self,
        Parameters(p): Parameters<PathParams>,
    ) -> std::result::Result<String, String> {
        text(
            self.pick(p.project.as_deref())?
                .get_request(&FsStorage, &p.path),
        )
    }

    #[tool(
        description = "List the environments with their servers' base URLs and variables, and the project's variables. Secret variables are listed by name only."
    )]
    fn list_environments(
        &self,
        Parameters(p): Parameters<ProjectParams>,
    ) -> std::result::Result<String, String> {
        text(
            self.pick(p.project.as_deref())?
                .list_environments(&FsStorage),
        )
    }

    #[tool(
        description = "Send a saved request in an environment and return what was sent, the response (status, time, headers, body up to 100 KB) and the results of its tests. Requests that may change data (not GET, HEAD or OPTIONS) are refused in protected environments."
    )]
    async fn send_request(
        &self,
        Parameters(p): Parameters<SendParams>,
    ) -> std::result::Result<String, String> {
        let project = self.pick(p.project.as_deref())?;
        let summary = format!("Sent {}", p.path.trim());
        let prepared =
            match project.prepare_send(&FsStorage, &p.path, p.environment.as_deref(), &p.variables)
            {
                Ok(prepared) => prepared,
                Err(e) => {
                    let e = e.to_string();
                    self.log(&project, summary, e.clone(), true);
                    return Err(e);
                }
            };
        let client = self.client.clone();
        let hidden = project.hidden();
        let (done, result) = tokio::sync::oneshot::channel();
        // Scripts run in the JS engine, which needs a big stack and a runtime of its own.
        std::thread::Builder::new()
            .name("mcp-send".into())
            .stack_size(SEND_STACK)
            .spawn(move || {
                let sent = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map(|runtime| runtime.block_on(super::send(&client, &prepared, &hidden)))
                    .map_err(|e| format!("Could not send: {e}"));
                // The receiver only goes away when the agent stopped waiting for the answer.
                let _ = done.send(sent);
            })
            .map_err(|e| format!("Could not send: {e}"))?;
        let sent = result
            .await
            .map_err(|_| "The send stopped before it finished.".to_owned())
            .and_then(|sent| sent);
        match sent {
            Ok(sent) => {
                self.log(&project, summary, sent.outcome, sent.failed);
                Ok(sent.text)
            }
            Err(e) => {
                self.log(&project, summary, e.clone(), true);
                Err(e)
            }
        }
    }

    #[tool(
        description = "Add a request to a folder. Returns its path. Writes a file to the project."
    )]
    fn create_request(
        &self,
        Parameters(p): Parameters<CreateRequestParams>,
    ) -> std::result::Result<String, String> {
        let name = p.request.get("name").and_then(Value::as_str).unwrap_or("");
        self.edit(
            p.project.as_deref(),
            &format!("Create request {name}"),
            |project| project.create_request(&FsStorage, &p.folder, &p.request),
        )
    }

    #[tool(
        description = "Replace a saved request with a changed version of what get_request returned (also to add, change or remove its example responses). Returns its path, which changes with its name."
    )]
    fn save_request(
        &self,
        Parameters(p): Parameters<SaveRequestParams>,
    ) -> std::result::Result<String, String> {
        self.edit(
            p.project.as_deref(),
            &format!("Save {}", p.path),
            |project| project.save_request(&FsStorage, &p.path, &p.request),
        )
    }

    #[tool(description = "Add a folder. Returns its path.")]
    fn create_folder(
        &self,
        Parameters(p): Parameters<CreateFolderParams>,
    ) -> std::result::Result<String, String> {
        self.edit(
            p.project.as_deref(),
            &format!("Create folder {}", p.name),
            |project| project.create_folder(&FsStorage, &p.parent, &p.name),
        )
    }

    #[tool(description = "Rename a folder. Returns its new path.")]
    fn rename_folder(
        &self,
        Parameters(p): Parameters<RenameFolderParams>,
    ) -> std::result::Result<String, String> {
        self.edit(
            p.project.as_deref(),
            &format!("Rename {}", p.path),
            |project| project.rename_folder(&FsStorage, &p.path, &p.name),
        )
    }

    #[tool(
        description = "Move a request or folder to the end of another folder. Returns its new path."
    )]
    fn move_item(
        &self,
        Parameters(p): Parameters<MoveParams>,
    ) -> std::result::Result<String, String> {
        self.edit(
            p.project.as_deref(),
            &format!("Move {}", p.path),
            |project| project.move_item(&FsStorage, &p.path, &p.folder),
        )
    }

    #[tool(description = "Delete a request, or a folder with everything in it.")]
    fn delete_item(
        &self,
        Parameters(p): Parameters<PathParams>,
    ) -> std::result::Result<String, String> {
        self.edit(
            p.project.as_deref(),
            &format!("Delete {}", p.path),
            |project| project.delete_item(&FsStorage, &p.path),
        )
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "`tool_handler` writes the trait's async functions"
)]
#[tool_handler(router = self.tool_router)]
impl ServerHandler for TortugaMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tortuga", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
