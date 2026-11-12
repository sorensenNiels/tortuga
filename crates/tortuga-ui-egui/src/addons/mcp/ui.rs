//! The MCP server add-on's place in the UI (R-MCP-5, R-ADD-3): the workspace menu item, the
//! palette commands, "MCP" in the top bar, and the dialog.

use egui::{RichText, Ui};

use super::McpServer;
use super::dialog::{McpDialogAction, McpView, mcp_dialog};
use crate::components::DialogResult;
use crate::state::AppState;
use crate::theme;

/// What the MCP server's menu item or a palette command asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    /// Start the server if it is stopped, and show its dialog.
    Open,
    Stop,
}

/// "MCP" in the top bar while the server runs. Returns whether it was clicked.
pub(crate) fn indicator(ui: &mut Ui, server: &McpServer) -> bool {
    let Some(running) = server.running() else {
        return false;
    };
    let label = RichText::new("⏺ MCP").color(theme::palette(ui).ok);
    ui.add(theme::ghost_button(label))
        .on_hover_text(t!("mcp-indicator-hint", url = running.url.as_str()))
        .clicked()
}

/// The workspace menu's item.
pub(crate) fn menu_item(ui: &mut Ui, server: &McpServer) -> Option<Command> {
    let label = if server.running().is_some() {
        t!("mcp-menu-running")
    } else {
        t!("mcp-menu-start")
    };
    ui.button(label)
        .on_hover_text(t!("mcp-menu-hint"))
        .clicked()
        .then_some(Command::Open)
}

/// The command palette's entries.
pub(crate) fn palette(server: &McpServer) -> Vec<(String, Command)> {
    if server.running().is_some() {
        vec![
            (t!("cmd-mcp-server"), Command::Open),
            (t!("cmd-stop-mcp"), Command::Stop),
        ]
    } else {
        vec![(t!("cmd-start-mcp"), Command::Open)]
    }
}

/// Does what was asked. Returns whether to show the dialog.
pub(crate) fn run(state: &mut AppState, command: Command) -> bool {
    match command {
        Command::Open => {
            let Some(server) = &state.addons.mcp else {
                return false;
            };
            if server.running().is_none() {
                start(state);
            }
            true
        }
        Command::Stop => {
            stop(state);
            false
        }
    }
}

fn start(state: &mut AppState) {
    match state.start_mcp() {
        Ok(url) => state.inform(t!("mcp-started", url = url)),
        Err(e) => state.notify(t!("mcp-start-failed", error = e)),
    }
}

fn stop(state: &mut AppState) {
    state.stop_mcp();
    state.inform(t!("mcp-stopped"));
}

/// Shows the MCP dialog. Returns whether it stays open.
pub(crate) fn dialog(ctx: &egui::Context, state: &mut AppState) -> bool {
    let Some(server) = &state.addons.mcp else {
        return false;
    };
    let view = McpView {
        url: server.running().map(|r| r.url.as_str()),
        log: server.log(),
        offset_minutes: state.host.utc_offset_minutes(),
    };
    let (result, action) = mcp_dialog(ctx, view);
    match action {
        Some(McpDialogAction::Start) => start(state),
        Some(McpDialogAction::Stop) => stop(state),
        Some(McpDialogAction::Copy(agent)) => {
            if let Some(running) = state.addons.mcp.as_ref().and_then(McpServer::running) {
                ctx.copy_text(agent.config(&running.url, &running.token));
                let agent = agent.name();
                state.inform(t!("mcp-copied", agent = agent));
            }
        }
        Some(McpDialogAction::ClearLog) => {
            if let Some(server) = &mut state.addons.mcp {
                server.clear_log();
            }
        }
        None => {}
    }
    result == DialogResult::Open
}
