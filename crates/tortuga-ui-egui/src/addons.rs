//! Add-ons (D78, R-ADD-1 to R-ADD-3): parts of the app not everyone needs, off until they
//! are turned on in Settings → Add-ons.
//!
//! An add-on that is off is not loaded: its state in [`AddOns`] is `None`, and none of its
//! menu items, commands, top-bar items or dialogs exist (R-ADD-2). The rest of the UI
//! reaches add-ons only through the functions in this file, which dispatch over
//! [`AddOn`]; each add-on's code lives in its own module beside it. Adding an add-on means
//! a variant of [`AddOn`] in `tortuga-core`, a module here and an arm in each `match`.
//!
//! The exception is Workflows (R-ADD-4): its code stays in `shell/workflows.rs` and
//! `components/workflow_view.rs`, woven into tabs, auto-save and the sidebar; only its
//! switch is here, and the shell asks [`AppState::addon_on`].

pub(crate) mod mcp;
pub(crate) mod mock;

use egui::{Context, Ui};
pub use tortuga_core::settings::AddOn;

use crate::host::Host;
use crate::state::AppState;

/// The add-ons that are loaded, each `None` while it is off.
#[derive(Debug, Default)]
pub struct AddOns {
    /// The MCP server for AI agents (R-ADD-3).
    pub(crate) mcp: Option<mcp::McpServer>,
    /// The projects' mock servers (R-ADD-5).
    pub(crate) mock: Option<mock::Mocks>,
}

/// What an add-on's menu item or palette command asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Mcp(mcp::Command),
    Mock(mock::Command),
}

/// A command palette entry of an add-on.
pub(crate) struct PaletteEntry {
    pub label: String,
    pub keywords: &'static str,
    pub command: Command,
    /// Marks the entry that is chosen now, e.g. the "Mock" environment.
    pub current: bool,
}

/// What Settings → Add-ons calls an add-on.
pub(crate) fn name(addon: AddOn) -> String {
    match addon {
        AddOn::McpServer => t!("addon-mcp-server"),
        AddOn::Workflows => t!("addon-workflows"),
        AddOn::MockServer => t!("addon-mock-server"),
    }
}

/// One line on what an add-on does, under its name in Settings → Add-ons.
pub(crate) fn about(addon: AddOn) -> String {
    match addon {
        AddOn::McpServer => t!("addon-mcp-server-about"),
        AddOn::Workflows => t!("addon-workflows-about"),
        AddOn::MockServer => t!("addon-mock-server-about"),
    }
}

/// Whether an add-on can work in this build; one that can't is not listed (R-ADD-1).
fn available(addon: AddOn, host: &dyn Host) -> bool {
    match addon {
        AddOn::McpServer | AddOn::MockServer => host.can_run_servers(),
        AddOn::Workflows => true,
    }
}

/// The add-ons Settings → Add-ons lists, in order.
pub(crate) fn listed(host: &dyn Host) -> Vec<AddOn> {
    AddOn::ALL
        .into_iter()
        .filter(|a| available(*a, host))
        .collect()
}

impl AppState {
    /// Whether `addon` is turned on and can work here.
    pub(crate) fn addon_on(&self, addon: AddOn) -> bool {
        self.settings.addons.enabled(addon) && available(addon, self.host.as_ref())
    }

    /// Loads the add-ons that are on and unloads those turned off, stopping what they
    /// run (R-ADD-1). Called at start-up and when the settings are saved.
    pub(crate) fn load_addons(&mut self) {
        for addon in AddOn::ALL {
            let on = self.addon_on(addon);
            match addon {
                AddOn::McpServer => {
                    if on {
                        self.addons.mcp.get_or_insert_with(mcp::McpServer::default);
                    } else if let Some(server) = self.addons.mcp.take()
                        && server.running().is_some()
                    {
                        self.inform(t!("mcp-stopped"));
                    }
                }
                // Its state is the shell's, which closes the workflow view and stops
                // runs when it is turned off (`TortugaUi::close_workflows_when_off`).
                AddOn::Workflows => {}
                AddOn::MockServer => {
                    if on {
                        self.addons.mock.get_or_insert_with(mock::Mocks::default);
                    } else if let Some(mocks) = self.addons.mock.take()
                        && !mocks.ports().is_empty()
                    {
                        self.inform(t!("mock-addon-stopped"));
                    }
                }
            }
        }
    }

    /// Tells the add-ons that the workspace changed: a project was opened, closed or
    /// reloaded, or the environment or secrets changed.
    pub(crate) fn refresh_addons(&self) {
        self.refresh_mcp();
    }

    /// Takes in what the add-ons' background work sent; once a frame.
    pub(crate) fn poll_addons(&mut self) {
        if let Some(server) = &mut self.addons.mcp {
            server.take_actions();
        }
        if let Some(mocks) = &mut self.addons.mock {
            mocks.take_hits();
        }
    }
}

/// The top-bar items of the add-ons that are on. Returns the add-on whose dialog to open.
pub(crate) fn top_bar(ui: &mut Ui, state: &AppState) -> Option<AddOn> {
    let server = state.addons.mcp.as_ref()?;
    mcp::ui::indicator(ui, server).then_some(AddOn::McpServer)
}

/// The workspace menu's items of the add-ons that are on.
pub(crate) fn workspace_menu(ui: &mut Ui, state: &AppState) -> Option<Command> {
    let server = state.addons.mcp.as_ref()?;
    mcp::ui::menu_item(ui, server).map(Command::Mcp)
}

/// The command palette's entries of the add-ons that are on.
pub(crate) fn palette(state: &AppState) -> Vec<PaletteEntry> {
    let mut entries = Vec::new();
    if let Some((label, current, command)) = state
        .addons
        .mock
        .as_ref()
        .and_then(mock::ui::environment_entry)
    {
        entries.push(PaletteEntry {
            label,
            keywords: "switch mock server",
            command: Command::Mock(command),
            current,
        });
    }
    if let Some(server) = &state.addons.mcp {
        entries.extend(
            mcp::ui::palette(server)
                .into_iter()
                .map(|(label, command)| PaletteEntry {
                    label,
                    keywords: "mcp ai agent claude cursor copilot",
                    command: Command::Mcp(command),
                    current: false,
                }),
        );
    }
    entries
}

/// The command palette's entries of the add-ons that are on, for the project at `root`.
pub(crate) fn project_palette(state: &AppState, root: &std::path::Path) -> Vec<PaletteEntry> {
    let Some(mocks) = &state.addons.mock else {
        return Vec::new();
    };
    mock::ui::project_palette(mocks, root)
        .into_iter()
        .map(|(label, command)| PaletteEntry {
            label,
            keywords: "mock examples",
            command: Command::Mock(command),
            current: false,
        })
        .collect()
}

/// Does what a menu item or command asked. Returns the add-on whose dialog to open.
pub(crate) fn run(state: &mut AppState, command: Command) -> Option<AddOn> {
    match command {
        Command::Mcp(command) => mcp::ui::run(state, command).then_some(AddOn::McpServer),
        Command::Mock(command) => mock::ui::run(state, command).then_some(AddOn::MockServer),
    }
}

/// Shows an add-on's dialog. Returns whether it stays open; it closes when the add-on
/// is off.
pub(crate) fn dialog(ctx: &Context, state: &mut AppState, addon: AddOn) -> bool {
    match addon {
        AddOn::McpServer => state.addons.mcp.is_some() && mcp::ui::dialog(ctx, state),
        AddOn::MockServer => mock::ui::dialog(ctx, state),
        AddOn::Workflows => false,
    }
}

#[cfg(test)]
mod tests;
