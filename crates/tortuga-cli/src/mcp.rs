//! `tortuga-cli mcp`: serves the project to an AI agent over the Model Context Protocol on
//! stdio (R-MCP-1). Stdout carries only the protocol; errors go to stderr.

use tortuga_core::Collection;
use tortuga_core::mcp::McpProject;
use tortuga_core::storage::FsStorage;
use tortuga_core::workspace::{self, WorkspaceFile};

use crate::Args;

/// Serves `args.project` until the agent closes stdin.
pub fn serve(args: &Args) -> Result<(), String> {
    let workspace = match &args.workspace {
        None => WorkspaceFile::default(),
        Some(path) => crate::read_workspace(path)?,
    };
    // Say what is wrong now, on stderr, rather than in every tool call.
    let collection = Collection::open(&FsStorage, &args.project).map_err(|e| e.to_string())?;
    if let Some(name) = &args.env {
        workspace::environment_named(&collection, &workspace, name).map_err(|e| e.to_string())?;
    }
    let project = McpProject {
        root: args.project.clone(),
        workspace,
        environment: args.env.clone(),
        values: args.vars.clone(),
    };
    let client = crate::client(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime
        .block_on(tortuga_core::mcp::server::serve_stdio(project, client))
        .map_err(|e| e.to_string())
}
