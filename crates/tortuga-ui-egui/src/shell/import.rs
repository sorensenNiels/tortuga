//! Importing an OpenAPI description as a new project (R-COL-15).

use std::path::PathBuf;

use tortuga_core::collection::MANIFEST_FILE;
use tortuga_core::openapi;

use super::{Modal, TortugaUi};
use crate::components::{DialogResult, EnvironmentsDialog, ImportDialog, ImportTarget};

/// The import dialog with what the shell found out about its folder (checked when
/// the folder changes, not every frame).
#[derive(Debug)]
pub(super) struct OpenImport {
    dialog: ImportDialog,
    taken: bool,
    repository: Option<PathBuf>,
}

impl TortugaUi {
    /// Picks a description file, reads it and shows what the import will make.
    pub(super) fn import_openapi(&mut self) {
        let Some(path) = self
            .state
            .host
            .pick_any_file("Choose an OpenAPI description (JSON or YAML)")
        else {
            return;
        };
        let read = self
            .state
            .storage()
            .read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| openapi::parse(&text).map_err(|e| tortuga_core::error::message(&e)));
        match read {
            Ok(import) => {
                let mut open = OpenImport {
                    dialog: ImportDialog::new(import, path),
                    taken: false,
                    repository: None,
                };
                self.check_import_folder(&mut open);
                self.modal = Some(Modal::ImportOpenApi(Box::new(open)));
            }
            Err(e) => self
                .state
                .notify(format!("Could not import {}: {e}", path.display())),
        }
    }

    fn check_import_folder(&self, open: &mut OpenImport) {
        let dir = &open.dialog.dir;
        open.taken = self.state.storage().exists(&dir.join(MANIFEST_FILE));
        // The folder may not exist yet: look from the nearest one that does.
        let existing = dir
            .ancestors()
            .find(|d| self.state.storage().exists(d))
            .unwrap_or(dir);
        open.repository = self.state.host.repository_of(existing);
    }

    /// Shows the import dialog. Returns whether it stays open.
    pub(super) fn show_import_dialog(
        &mut self,
        ctx: &egui::Context,
        open: &mut OpenImport,
    ) -> bool {
        let target = ImportTarget {
            taken: open.taken,
            repository: open.repository.as_deref(),
            can_pick_folders: self.state.host.can_pick_folders(),
        };
        let (result, choose) = open.dialog.show(ctx, target);
        if choose
            && let Some(dir) = self
                .state
                .host
                .pick_folder("Choose or create a folder for the project")
        {
            open.dialog.dir = dir;
            self.check_import_folder(open);
        }
        match result {
            DialogResult::Done(()) => {
                self.finish_import(&open.dialog);
                false
            }
            result => result == DialogResult::Open,
        }
    }

    /// Writes the project and adds it to the workspace. With secret variables, the
    /// environments dialog opens so their values can be filled in.
    fn finish_import(&mut self, dialog: &ImportDialog) {
        let dir = &dialog.dir;
        if let Err(e) = openapi::create_project(self.state.storage(), dir, &dialog.import) {
            self.state.notify(format!(
                "Could not import: {}",
                tortuga_core::error::message(&e)
            ));
            return;
        }
        if let Err(e) = self.state.open_collection(dir) {
            self.state.notify(e.to_string());
            return;
        }
        if !dialog.import.manifest.secrets.is_empty() {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                dir.clone(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;

    /// R-COL-15: importing writes the project, adds it to the workspace and opens the
    /// environments dialog for its secret values; a folder that is a project is refused.
    #[test]
    fn import_adds_the_project_and_asks_for_secrets() {
        let dir = std::env::temp_dir().join(format!("tortuga-ui-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let spec = r#"{"openapi": "3.0.0", "info": {"title": "Pets"},
            "security": [{"b": []}],
            "components": {"securitySchemes": {"b": {"type": "http", "scheme": "bearer"}}},
            "paths": {"/pets": {"get": {"summary": "List pets"}}}}"#;
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        let mut open = OpenImport {
            dialog: ImportDialog::new(
                openapi::parse(spec).expect("parse"),
                dir.join("openapi.json"),
            ),
            taken: false,
            repository: None,
        };
        ui.check_import_folder(&mut open);
        assert!(!open.taken);
        ui.finish_import(&open.dialog);
        let root = dir.join("pets");
        let project = ui.state.collection(&root).expect("in the workspace");
        assert_eq!(project.name(), "Pets");
        assert_eq!(project.items.len(), 1);
        assert!(matches!(ui.modal, Some(Modal::Environments(_))));

        ui.modal = None;
        ui.check_import_folder(&mut open);
        assert!(open.taken, "now it is a project");
        ui.finish_import(&open.dialog);
        assert!(
            ui.state
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("already"))
        );
        assert!(ui.modal.is_none());
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
