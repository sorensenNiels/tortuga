//! The app's MCP server (R-MCP-5): whether it runs and where, the configuration to paste
//! into an AI agent, start and stop, and the log of what agents did.

use std::time::UNIX_EPOCH;

use egui::{Context, Grid, RichText, ScrollArea, Ui};
use tortuga_core::mcp::{Agent, McpAction};

use super::DialogResult;
use super::modals::{dialog, finish};
use crate::theme;

/// What the user asked of the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpDialogAction {
    Start,
    Stop,
    /// Copy the configuration for this agent.
    Copy(Agent),
    ClearLog,
}

/// What the dialog shows.
#[derive(Debug, Clone, Copy)]
pub struct McpView<'a> {
    /// The address agents connect to; `None` when stopped.
    pub url: Option<&'a str>,
    /// Oldest first.
    pub log: &'a [McpAction],
    pub offset_minutes: i32,
}

/// Shows the MCP server dialog; returns whether it stays open and what was asked.
pub fn mcp_dialog(ctx: &Context, view: McpView<'_>) -> (DialogResult<()>, Option<McpDialogAction>) {
    {
        let mut action = None;
        let result = dialog(ctx, "mcp", &t!("mcp-title"), 820.0, true, |ui| {
            theme::hint(ui, t!("mcp-intro"));
            ui.add_space(8.0);
            action = controls(ui, view.url);
            if view.url.is_some() {
                ui.add_space(10.0);
                theme::section_label(ui, &t!("mcp-connect"));
                ui.horizontal_wrapped(|ui| {
                    for agent in Agent::ALL {
                        if ui
                            .button(t!("mcp-copy-for", agent = agent.name()))
                            .clicked()
                        {
                            action = Some(McpDialogAction::Copy(agent));
                        }
                    }
                });
                theme::hint(ui, t!("mcp-token-hint"));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                theme::section_label(ui, &t!("mcp-log"));
                if !view.log.is_empty() && ui.small_button(t!("clear")).clicked() {
                    action = Some(McpDialogAction::ClearLog);
                }
            });
            log(ui, view.log, view.offset_minutes);
            ui.add_space(12.0);
            let close = theme::button_row(ui, |ui| ui.button(t!("close")).clicked());
            finish::<()>(close, None)
        });
        (result, action)
    }
}

/// Running at an address, or stopped; and Start or Stop.
fn controls(ui: &mut Ui, url: Option<&str>) -> Option<McpDialogAction> {
    let p = theme::palette(ui);
    let mut action = None;
    ui.horizontal(|ui| {
        match url {
            Some(url) => {
                theme::badge(ui, t!("mock-running"), p.ok);
                ui.label(RichText::new(url).monospace());
            }
            None => {
                theme::badge(ui, t!("mock-stopped-badge"), p.muted);
            }
        }
        theme::button_row(ui, |ui| {
            if url.is_some() {
                if ui.add(theme::danger_button(ui, t!("mock-stop"))).clicked() {
                    action = Some(McpDialogAction::Stop);
                }
            } else if ui
                .add(theme::primary_button(ui, t!("mock-start")))
                .clicked()
            {
                action = Some(McpDialogAction::Start);
            }
        });
    });
    action
}

/// What agents did, newest first.
fn log(ui: &mut Ui, log: &[McpAction], offset_minutes: i32) {
    if log.is_empty() {
        theme::hint(ui, t!("mock-nothing-yet"));
        return;
    }
    let p = theme::palette(ui);
    ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        Grid::new("mcp-log")
            .num_columns(4)
            .spacing([14.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                for heading in [
                    t!("mock-time"),
                    t!("mcp-project"),
                    t!("mcp-action"),
                    t!("mcp-outcome"),
                ] {
                    theme::section_label(ui, &heading);
                }
                ui.end_row();
                for action in log.iter().rev() {
                    let ms = action
                        .at
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
                    ui.label(
                        RichText::new(crate::format::clock_seconds(ms, offset_minutes))
                            .color(p.muted),
                    );
                    ui.label(&action.project);
                    let (shown, _) = crate::format::truncate(&action.summary, 60);
                    ui.label(RichText::new(shown).monospace())
                        .on_hover_text(&action.summary);
                    let color = if action.failed { p.fail } else { p.ok };
                    let (outcome, _) = crate::format::truncate(&action.outcome, 80);
                    ui.label(RichText::new(outcome).color(color))
                        .on_hover_text(&action.outcome);
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use egui_kittest::kittest::Queryable;

    use super::*;

    fn action(summary: &str, failed: bool) -> McpAction {
        McpAction {
            at: SystemTime::now(),
            project: "Shop ü😀".into(),
            summary: summary.into(),
            outcome: if failed { "404 Not Found" } else { "200 OK" }.into(),
            failed,
        }
    }

    fn run(url: Option<&'static str>, log: Vec<McpAction>) -> Option<McpDialogAction> {
        let clicked = std::rc::Rc::new(std::cell::Cell::new(None));
        let seen = std::rc::Rc::clone(&clicked);
        let mut harness = crate::test_host::harness(move |ui| {
            let view = McpView {
                url,
                log: &log,
                offset_minutes: 0,
            };
            if let (_, Some(a)) = mcp_dialog(ui.ctx(), view) {
                seen.set(Some(a));
            }
        });
        harness.run();
        let target = if url.is_some() {
            t!("mcp-copy-for", agent = "Cursor")
        } else {
            t!("mock-start")
        };
        harness.get_by_label(&target).click();
        harness.run();
        clicked.get()
    }

    /// R-MCP-5: stopped, the dialog offers Start and no configuration to copy.
    #[test]
    fn stopped_offers_start() {
        assert_eq!(run(None, Vec::new()), Some(McpDialogAction::Start));
    }

    /// R-MCP-5: running, it shows the address, copies a config, and lists the log.
    #[test]
    fn running_shows_the_address_and_copies_a_config() {
        let url = "http://127.0.0.1:4040/mcp";
        let log = vec![
            action("Sent users/get-user", false),
            action("Delete users", true),
        ];
        let mut harness = crate::test_host::harness(move |ui| {
            let view = McpView {
                url: Some(url),
                log: &log,
                offset_minutes: 0,
            };
            mcp_dialog(ui.ctx(), view);
        });
        harness.run();
        harness.get_by_label(url);
        harness.get_by_label("Sent users/get-user");
        harness.get_by_label("404 Not Found");
        assert_eq!(
            run(Some(url), Vec::new()),
            Some(McpDialogAction::Copy(Agent::Cursor))
        );
    }
}
