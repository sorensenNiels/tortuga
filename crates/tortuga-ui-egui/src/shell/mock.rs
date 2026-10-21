//! Starting, stopping and showing a project's mock server (R-MOCK-2, R-MOCK-4).

use std::path::{Path, PathBuf};

use super::{Modal, TortugaUi};
use crate::components::{DialogResult, MockAction, MockDialog, MockView, SidebarAction};
use crate::state::root_key;

impl TortugaUi {
    /// The project menu's mock server items.
    pub(super) fn on_mock_sidebar(&mut self, action: SidebarAction) {
        match action {
            SidebarAction::StartMock(root) => self.start_mock(&root),
            SidebarAction::StopMock(root) => self.stop_mock(&root),
            SidebarAction::ShowMock(root) => self.show_mock(root),
            // Only the mock actions are sent here.
            _ => {}
        }
    }

    pub(super) fn start_mock(&mut self, root: &Path) {
        match self.state.start_mock(root) {
            Ok(port) => {
                let name = self.project_name(root);
                self.state
                    .inform(format!("The mock server of “{name}” runs on port {port}."));
            }
            Err(e) => self
                .state
                .notify(format!("Could not start the mock server: {e}")),
        }
    }

    /// Stops the project's mock server, and says so (R-COL-28).
    pub(super) fn stop_mock(&mut self, root: &Path) {
        self.state.stop_mock(root);
        let name = self.project_name(root);
        self.state
            .inform(format!("The mock server of “{name}” stopped."));
    }

    fn project_name(&self, root: &Path) -> String {
        self.state
            .collection(root)
            .map_or_else(|| root.display().to_string(), |c| c.name().to_owned())
    }

    pub(super) fn show_mock(&mut self, root: PathBuf) {
        let Some(project) = self.state.collection(&root).map(|c| c.name().to_owned()) else {
            return;
        };
        let port = self.mock_port(&root);
        self.modal = Some(Modal::Mock(MockDialog::new(root, &project, port)));
    }

    /// The port the project's mock runs or starts on.
    fn mock_port(&self, root: &Path) -> u16 {
        self.state.mocks.port(root).unwrap_or_else(|| {
            self.state
                .workspace()
                .mock_ports
                .get(&root_key(root))
                .copied()
                .unwrap_or(tortuga_core::mock::DEFAULT_PORT)
        })
    }

    /// Shows the mock dialog. Returns whether it stays open.
    pub(super) fn show_mock_dialog(&mut self, ctx: &egui::Context, d: &mut MockDialog) -> bool {
        let root = d.root.clone();
        if self.state.collection(&root).is_none() {
            return false;
        }
        let view = MockView {
            running: self.state.mocks.port(&root),
            port: self.mock_port(&root),
            hits: self.state.mocks.hits(&root),
            in_use: self.state.mocks.in_use(),
            offset_minutes: self.state.host.utc_offset_minutes(),
        };
        let (result, action) = d.show(ctx, view);
        match action {
            Some(MockAction::Start) => self.start_mock(&root),
            Some(MockAction::Stop) => self.stop_mock(&root),
            Some(MockAction::SetPort(port)) => {
                if let Err(e) = self.state.set_mock_port(&root, port) {
                    self.state
                        .notify(format!("Could not start the mock server: {e}"));
                }
            }
            Some(MockAction::ClearLog) => self.state.mocks.clear_hits(&root),
            Some(MockAction::UseMockEnvironment) => self.state.use_mock_environment(),
            None => {}
        }
        result == DialogResult::Open
    }
}
