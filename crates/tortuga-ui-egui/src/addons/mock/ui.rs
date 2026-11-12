//! The mock server add-on's place in the UI (R-MOCK-2, R-MOCK-4, R-ADD-5): starting,
//! stopping and showing a project's mock server, its palette commands, and the "Mock"
//! environment's name in the environment picker.

use std::path::{Path, PathBuf};

use super::Mocks;
use super::dialog::{MockAction, MockDialog, MockView};
use crate::components::DialogResult;
use crate::state::{AppState, root_key};

/// What a project menu item or a palette command asks of a mock server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Start(PathBuf),
    Stop(PathBuf),
    /// Show the project's mock dialog.
    Show(PathBuf),
    /// Send requests to the running mocks (D45).
    UseEnvironment,
}

/// What the environment picker calls the mocks' environment while one runs (D45).
pub(crate) fn environment_label(ports: &[u16]) -> Option<String> {
    match ports {
        [] => None,
        [port] => Some(t!("mock-label-port", port = *port)),
        more => Some(t!("mock-label-servers", count = more.len())),
    }
}

/// The palette's "Mock" environment entry while a mock runs, and whether it is chosen.
pub(crate) fn environment_entry(mocks: &Mocks) -> Option<(String, bool, Command)> {
    (!mocks.ports().is_empty()).then(|| {
        (
            t!("cmd-environment-mock"),
            mocks.in_use(),
            Command::UseEnvironment,
        )
    })
}

/// The palette's entries for the project at `root`.
pub(crate) fn project_palette(mocks: &Mocks, root: &Path) -> Vec<(String, Command)> {
    let root = root.to_owned();
    if mocks.port(&root).is_some() {
        vec![
            (t!("cmd-mock-server"), Command::Show(root.clone())),
            (t!("cmd-stop-mock"), Command::Stop(root)),
        ]
    } else {
        vec![(t!("cmd-start-mock"), Command::Start(root))]
    }
}

/// Does what was asked. Returns whether to show the dialog.
pub(crate) fn run(state: &mut AppState, command: Command) -> bool {
    if state.addons.mock.is_none() {
        return false;
    }
    match command {
        Command::Start(root) => start(state, &root),
        Command::Stop(root) => stop(state, &root),
        Command::Show(root) => return open_dialog(state, root),
        Command::UseEnvironment => state.use_mock_environment(),
    }
    false
}

fn start(state: &mut AppState, root: &Path) {
    match state.start_mock(root) {
        Ok(port) => {
            let name = project_name(state, root);
            state.inform(t!("mock-started", name = name.as_str(), port = port));
        }
        Err(e) => state.notify(t!("mock-start-failed", error = e)),
    }
}

/// Stops the project's mock server, and says so (R-COL-28).
fn stop(state: &mut AppState, root: &Path) {
    state.stop_mock(root);
    let name = project_name(state, root);
    state.inform(t!("mock-stopped", name = name.as_str()));
}

fn project_name(state: &AppState, root: &Path) -> String {
    state
        .collection(root)
        .map_or_else(|| root.display().to_string(), |c| c.name().to_owned())
}

/// Opens the project's mock dialog. Returns whether there is one to show.
fn open_dialog(state: &mut AppState, root: PathBuf) -> bool {
    let Some(project) = state.collection(&root).map(|c| c.name().to_owned()) else {
        return false;
    };
    let port = port_for(state, &root);
    let Some(mocks) = &mut state.addons.mock else {
        return false;
    };
    mocks.dialog = Some(MockDialog::new(root, &project, port));
    true
}

/// The port the project's mock runs or starts on.
fn port_for(state: &AppState, root: &Path) -> u16 {
    state.mock_port(root).unwrap_or_else(|| {
        state
            .workspace()
            .mock_ports
            .get(&root_key(root))
            .copied()
            .unwrap_or(tortuga_core::mock::DEFAULT_PORT)
    })
}

/// Shows the mock dialog. Returns whether it stays open.
pub(crate) fn dialog(ctx: &egui::Context, state: &mut AppState) -> bool {
    let Some(mut d) = state.addons.mock.as_mut().and_then(|m| m.dialog.take()) else {
        return false;
    };
    let root = d.root.clone();
    if state.collection(&root).is_none() {
        return false;
    }
    let Some(mocks) = &state.addons.mock else {
        return false;
    };
    let view = MockView {
        running: mocks.port(&root),
        port: port_for(state, &root),
        hits: mocks.hits(&root),
        in_use: mocks.in_use(),
        offset_minutes: state.host.utc_offset_minutes(),
    };
    let (result, action) = d.show(ctx, view);
    match action {
        Some(MockAction::Start) => start(state, &root),
        Some(MockAction::Stop) => stop(state, &root),
        Some(MockAction::SetPort(port)) => {
            if let Err(e) = state.set_mock_port(&root, port) {
                state.notify(t!("mock-start-failed", error = e));
            }
        }
        Some(MockAction::ClearLog) => {
            if let Some(mocks) = &mut state.addons.mock {
                mocks.clear_hits(&root);
            }
        }
        Some(MockAction::UseMockEnvironment) => state.use_mock_environment(),
        None => {}
    }
    let open = result == DialogResult::Open;
    if open && let Some(mocks) = &mut state.addons.mock {
        mocks.dialog = Some(d);
    }
    open
}
