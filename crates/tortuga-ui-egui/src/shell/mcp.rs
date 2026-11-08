//! Starting, stopping and showing the app's MCP server for AI agents (R-MCP-5).

use egui::{RichText, Ui};

use super::{Modal, TortugaUi};
use crate::components::{DialogResult, McpDialogAction, McpView, mcp_dialog};
use crate::theme;

impl TortugaUi {
    pub(super) fn start_mcp(&mut self) {
        match self.state.start_mcp() {
            Ok(url) => self.state.inform(t!("mcp-started", url = url)),
            Err(e) => self.state.notify(t!("mcp-start-failed", error = e)),
        }
    }

    pub(super) fn stop_mcp(&mut self) {
        self.state.stop_mcp();
        self.state.inform(t!("mcp-stopped"));
    }

    pub(super) fn show_mcp(&mut self) {
        self.modal = Some(Modal::Mcp);
    }

    /// "MCP" in the top bar while the server runs; it opens the dialog.
    pub(super) fn mcp_indicator(&mut self, ui: &mut Ui) {
        let Some(running) = self.state.mcp.running() else {
            return;
        };
        let p = theme::palette(ui);
        let label = RichText::new("⏺ MCP").color(p.ok);
        if ui
            .add(theme::ghost_button(label))
            .on_hover_text(t!("mcp-indicator-hint", url = running.url.as_str()))
            .clicked()
        {
            self.show_mcp();
        }
    }

    /// Shows the MCP dialog. Returns whether it stays open.
    pub(super) fn show_mcp_dialog(&mut self, ctx: &egui::Context) -> bool {
        let view = McpView {
            url: self.state.mcp.running().map(|r| r.url.as_str()),
            log: self.state.mcp.log(),
            offset_minutes: self.state.host.utc_offset_minutes(),
        };
        let (result, action) = mcp_dialog(ctx, view);
        match action {
            Some(McpDialogAction::Start) => self.start_mcp(),
            Some(McpDialogAction::Stop) => self.stop_mcp(),
            Some(McpDialogAction::Copy(agent)) => {
                if let Some(running) = self.state.mcp.running() {
                    ctx.copy_text(agent.config(&running.url, &running.token));
                    self.state.inform(t!("mcp-copied", agent = agent.name()));
                }
            }
            Some(McpDialogAction::ClearLog) => self.state.mcp.clear_log(),
            None => {}
        }
        result == DialogResult::Open
    }
}
