//! The MCP server add-on (R-ADD-3): the app's MCP server (R-MCP-5, D72, D75), started and
//! stopped for the whole workspace, serving its projects with the environment and secret
//! values chosen in the app, with a log of what agents did.

mod dialog;
pub(crate) mod ui;

pub(crate) use ui::Command;

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, PoisonError, RwLock};

use tortuga_core::mcp::{self, McpAction, McpProject, SharedProjects};
use tortuga_core::workspace::WorkspaceFile;

use crate::host::McpHandle;
use crate::state::AppState;

/// Actions kept in the log; older ones are dropped.
const MAX_ACTIONS: usize = 500;

/// The app's MCP server, while it runs, and what agents did through it.
#[derive(Debug)]
pub struct McpServer {
    running: Option<Running>,
    /// Oldest first; kept after a stop until the next start.
    log: Vec<McpAction>,
    tx: Sender<McpAction>,
    rx: Receiver<McpAction>,
}

#[derive(Debug)]
struct Running {
    handle: McpHandle,
    projects: SharedProjects,
}

impl Default for McpServer {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            running: None,
            log: Vec::new(),
            tx,
            rx,
        }
    }
}

impl McpServer {
    /// The running server's address and token.
    pub fn running(&self) -> Option<&McpHandle> {
        self.running.as_ref().map(|r| &r.handle)
    }

    /// What agents did, oldest first.
    pub fn log(&self) -> &[McpAction] {
        &self.log
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    /// Takes in what agents did since the last call.
    pub fn take_actions(&mut self) {
        while let Ok(action) = self.rx.try_recv() {
            if self.log.len() == MAX_ACTIONS {
                self.log.remove(0);
            }
            self.log.push(action);
        }
    }
}

impl AppState {
    /// Starts the MCP server for the workspace (R-MCP-5) on port 4040, else a free one.
    /// Returns its address.
    pub fn start_mcp(&mut self) -> Result<String, String> {
        let Some(server) = &self.addons.mcp else {
            return Err(t!("mcp-addon-off"));
        };
        if let Some(running) = server.running() {
            return Ok(running.url.clone());
        }
        let tx = server.tx.clone();
        let projects = Arc::new(RwLock::new(self.mcp_projects()));
        let mut problem = String::new();
        for port in [mcp::DEFAULT_PORT, 0] {
            let tx = tx.clone();
            let wake = self.waker();
            let on_action = Arc::new(move |action| {
                // The receiver only goes away with the app.
                let _ = tx.send(action);
                wake();
            });
            match self
                .host
                .start_mcp(port, Arc::clone(&projects), self.client.clone(), on_action)
            {
                Ok(handle) => {
                    let url = handle.url.clone();
                    if let Some(server) = &mut self.addons.mcp {
                        server.log.clear();
                        server.running = Some(Running { handle, projects });
                    }
                    return Ok(url);
                }
                Err(e) => problem = e,
            }
        }
        Err(problem)
    }

    /// Stops the MCP server; agents lose their connection.
    pub fn stop_mcp(&mut self) {
        if let Some(server) = &mut self.addons.mcp {
            server.running = None;
        }
    }

    /// Gives the running MCP server the workspace's projects as they are now: after a
    /// project was opened, closed or reloaded, or the environment or secrets changed.
    pub(super) fn refresh_mcp(&self) {
        let Some(running) = self.addons.mcp.as_ref().and_then(|s| s.running.as_ref()) else {
            return;
        };
        *running
            .projects
            .write()
            .unwrap_or_else(PoisonError::into_inner) = self.mcp_projects();
    }

    /// The open projects as the MCP server serves them: with the workspace's shared values
    /// (its secret ones filled in), the chosen environment as the default, and this
    /// machine's secret values for every environment, which a send uses and the agent
    /// never sees (P6).
    pub(crate) fn mcp_projects(&self) -> Vec<McpProject> {
        let ws = self.workspace();
        let mut workspace = WorkspaceFile {
            name: ws.name.clone(),
            projects: Vec::new(),
            variables: ws.variables.clone(),
            defaults: ws.defaults.clone(),
            environments: ws.environments.clone(),
        };
        workspace.variables.values = self.workspace_values();
        let environment = self.chosen_environment_name();
        let names = self.environment_names();
        self.collections
            .iter()
            .map(|c| {
                let own = self.secrets.get(&c.root);
                let for_env =
                    |id: Option<&str>| own.map(|s| s.for_environment(id)).unwrap_or_default();
                let mut secrets = BTreeMap::from([(String::new(), for_env(None))]);
                for name in &names {
                    // The workspace's secret values, the project's own over them.
                    let mut values = self
                        .workspace_environment(name)
                        .and_then(|e| self.workspace_env_secrets.get(&e.id))
                        .cloned()
                        .unwrap_or_default();
                    let own_id = c
                        .environments
                        .iter()
                        .find(|e| e.is_named(name))
                        .map(|e| e.id.as_str());
                    values.extend(for_env(own_id));
                    secrets.insert(name.clone(), values);
                }
                McpProject {
                    name: c.name().to_owned(),
                    root: c.root.clone(),
                    workspace: workspace.clone(),
                    environment: environment.clone(),
                    values: BTreeMap::new(),
                    secrets,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
