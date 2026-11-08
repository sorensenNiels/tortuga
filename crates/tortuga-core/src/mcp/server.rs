//! The MCP server (R-MCP-1, D74): [`McpProject`]'s tools over `rmcp`, on stdio for
//! `tortuga-cli mcp`. Native only.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::Value;

use super::McpProject;
use crate::storage::FsStorage;
use crate::{Client, Error, Result};

/// Stack for the thread a send runs on: the script engine needs more than tokio's
/// default 2 MB (the same as `tortuga-cli run`).
const SEND_STACK: usize = 8 * 1024 * 1024;

const INSTRUCTIONS: &str = "\
Tortuga is an API client whose projects are plain files in git. These tools work on one \
project: list_project shows its folders, requests (by path, relative to the project, \
with '/'), workflows and environments. get_request shows a request as saved; send_request \
sends it and returns the response and test results. Edits (create_request, save_request, \
create_folder, rename_folder, move_item, delete_item) change the project's files, which \
the user reviews in git like any change. Secret values are never shown: they appear as \
{{name}}.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathParams {
    /// A request's or folder's path as `list_project` gives it, e.g. "users/get-user.toml".
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendParams {
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
    /// The folder to add it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub folder: String,
    /// The request, shaped like the "request" object `get_request` returns: name, method
    /// and url are required; params, headers, body, auth, scripts and examples may be given.
    pub request: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveRequestParams {
    /// The request's path as `list_project` gives it.
    pub path: String,
    /// The whole request as `get_request` returned it, with the changes made. A new name
    /// renames the file.
    pub request: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateFolderParams {
    /// The folder to add it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub parent: String,
    /// The new folder's name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameFolderParams {
    /// The folder's path as `list_project` gives it.
    pub path: String,
    /// Its new name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MoveParams {
    /// The request's or folder's path as `list_project` gives it.
    pub path: String,
    /// The folder to move it to; "" (the default) is the project's top level.
    #[serde(default)]
    pub folder: String,
}

/// The MCP server for one project: its tools and the client its requests go out with.
#[derive(Debug, Clone)]
pub struct TortugaMcp {
    project: Arc<McpProject>,
    client: Client,
    tool_router: ToolRouter<Self>,
}

/// Serves `project` on stdin and stdout until the agent closes them (R-MCP-1).
pub async fn serve_stdio(project: McpProject, client: Client) -> Result<()> {
    let service = TortugaMcp::new(project, client)
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

#[tool_router]
impl TortugaMcp {
    pub fn new(project: McpProject, client: Client) -> Self {
        Self {
            project: Arc::new(project),
            client,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List the project's folders and requests (with their paths and methods), workflows and environments, as JSON."
    )]
    fn list_project(&self) -> std::result::Result<String, String> {
        text(self.project.list_project(&FsStorage))
    }

    #[tool(
        description = "Show a request as saved: method, url, params, headers, body, auth, scripts, docs and example responses, as JSON."
    )]
    fn get_request(
        &self,
        Parameters(p): Parameters<PathParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.get_request(&FsStorage, &p.path))
    }

    #[tool(
        description = "List the environments with their servers' base URLs and variables, and the project's variables. Secret variables are listed by name only."
    )]
    fn list_environments(&self) -> std::result::Result<String, String> {
        text(self.project.list_environments(&FsStorage))
    }

    #[tool(
        description = "Send a saved request in an environment and return what was sent, the response (status, time, headers, body up to 100 KB) and the results of its tests. Requests that may change data (not GET, HEAD or OPTIONS) are refused in protected environments."
    )]
    async fn send_request(
        &self,
        Parameters(p): Parameters<SendParams>,
    ) -> std::result::Result<String, String> {
        let prepared = self
            .project
            .prepare_send(&FsStorage, &p.path, p.environment.as_deref(), &p.variables)
            .map_err(|e| e.to_string())?;
        let client = self.client.clone();
        let hidden = self.project.values.clone();
        let (done, result) = tokio::sync::oneshot::channel();
        // Scripts run in the JS engine, which needs a big stack and a runtime of its own.
        std::thread::Builder::new()
            .name("mcp-send".into())
            .stack_size(SEND_STACK)
            .spawn(move || {
                let answer = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(super::send(&client, &prepared, &hidden)),
                    Err(e) => format!("Could not send: {e}"),
                };
                // The receiver only goes away when the agent stopped waiting for the answer.
                let _ = done.send(answer);
            })
            .map_err(|e| format!("Could not send: {e}"))?;
        result
            .await
            .map_err(|_| "The send stopped before it finished.".to_owned())
    }

    #[tool(
        description = "Add a request to a folder. Returns its path. Writes a file to the project."
    )]
    fn create_request(
        &self,
        Parameters(p): Parameters<CreateRequestParams>,
    ) -> std::result::Result<String, String> {
        text(
            self.project
                .create_request(&FsStorage, &p.folder, &p.request),
        )
    }

    #[tool(
        description = "Replace a saved request with a changed version of what get_request returned (also to add, change or remove its example responses). Returns its path, which changes with its name."
    )]
    fn save_request(
        &self,
        Parameters(p): Parameters<SaveRequestParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.save_request(&FsStorage, &p.path, &p.request))
    }

    #[tool(description = "Add a folder. Returns its path.")]
    fn create_folder(
        &self,
        Parameters(p): Parameters<CreateFolderParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.create_folder(&FsStorage, &p.parent, &p.name))
    }

    #[tool(description = "Rename a folder. Returns its new path.")]
    fn rename_folder(
        &self,
        Parameters(p): Parameters<RenameFolderParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.rename_folder(&FsStorage, &p.path, &p.name))
    }

    #[tool(
        description = "Move a request or folder to the end of another folder. Returns its new path."
    )]
    fn move_item(
        &self,
        Parameters(p): Parameters<MoveParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.move_item(&FsStorage, &p.path, &p.folder))
    }

    #[tool(description = "Delete a request, or a folder with everything in it.")]
    fn delete_item(
        &self,
        Parameters(p): Parameters<PathParams>,
    ) -> std::result::Result<String, String> {
        text(self.project.delete_item(&FsStorage, &p.path))
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
