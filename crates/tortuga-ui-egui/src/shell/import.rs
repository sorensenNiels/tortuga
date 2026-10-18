//! Importing an OpenAPI description (R-COL-15), a Postman collection (R-COL-16) or a
//! `.http` file (R-COL-17) as a new project.

use std::path::PathBuf;

use tortuga_core::collection::MANIFEST_FILE;
use tortuga_core::import::{self, ImportKind};

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
    /// Picks a file to import, reads it and shows what the import will make. A file of
    /// the other kind is imported as what it is.
    pub(super) fn import_file(&mut self, kind: ImportKind) {
        let title = match kind {
            ImportKind::OpenApi => "Choose an OpenAPI description (JSON or YAML)",
            ImportKind::Postman => "Choose a Postman collection (exported as JSON)",
            ImportKind::Http => "Choose a .http file (REST Client or JetBrains HTTP Client)",
        };
        let Some(path) = self.state.host.pick_any_file(title) else {
            return;
        };
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let read = self
            .state
            .storage()
            .read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                import::parse(&text, &file_name, kind).map_err(|e| tortuga_core::error::message(&e))
            });
        match read {
            Ok(import) => {
                let mut open = OpenImport {
                    dialog: ImportDialog::new(import, path),
                    taken: false,
                    repository: None,
                };
                self.check_import_folder(&mut open);
                self.modal = Some(Modal::Import(Box::new(open)));
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

    /// Writes the project and adds it to the workspace. Secret values found in the file
    /// go to this machine's secret store (D35); with secret variables still without a
    /// value, the environments dialog opens so they can be filled in.
    fn finish_import(&mut self, dialog: &ImportDialog) {
        let dir = &dialog.dir;
        let found = &dialog.import.secret_values;
        if let Err(e) = import::create_project(self.state.storage(), dir, &dialog.import) {
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
        if !found.is_empty() {
            let mut values = self.state.secrets.get(dir).cloned().unwrap_or_default();
            values.values.extend(
                found
                    .iter()
                    .map(|(name, value)| ((None, name.clone()), value.clone())),
            );
            if let Err(e) = self.state.save_secrets(dir, values) {
                self.state
                    .notify(format!("Could not keep the secret values: {e}"));
            }
        }
        let missing = dialog
            .import
            .manifest
            .secrets
            .iter()
            .any(|name| !found.contains_key(name));
        if missing {
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
                import::parse(spec, "openapi.json", ImportKind::OpenApi).expect("parse"),
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

    fn postman(dir: &std::path::Path, password: &str) -> OpenImport {
        let collection = format!(
            r#"{{"info": {{"name": "Shop"}},
            "auth": {{"type": "basic", "basic": [{{"key": "username", "value": "ann"}},
                                               {{"key": "password", "value": "{password}"}}]}},
            "item": [{{"name": "Ping", "request": "https://shop.test/ping"}}]}}"#
        );
        let import = import::parse(&collection, "shop.json", ImportKind::Postman).expect("parse");
        OpenImport {
            dialog: ImportDialog::new(import, dir.join("shop.postman_collection.json")),
            taken: false,
            repository: None,
        }
    }

    /// R-COL-16, D35: credentials found in a Postman collection are kept in this
    /// machine's secret store; the environments dialog only opens for secrets that
    /// still have no value.
    #[test]
    fn postman_import_keeps_found_secrets_locally() {
        let dir = std::env::temp_dir().join(format!("tortuga-ui-postman-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.finish_import(&postman(&dir.join("a"), "hunter2").dialog);
        let root = dir.join("a").join("shop");
        assert_eq!(ui.state.collection(&root).expect("added").name(), "Shop");
        assert!(ui.modal.is_none(), "every secret has a value");
        let stored = &ui.state.secrets[&root].values;
        assert_eq!(stored[&(None, "username".to_owned())], "ann");
        assert_eq!(stored[&(None, "password".to_owned())], "hunter2");
        let manifest = std::fs::read_to_string(root.join(MANIFEST_FILE)).expect("manifest");
        assert!(!manifest.contains("hunter2"));

        // An empty password in the collection: fill it in under Environments.
        ui.finish_import(&postman(&dir.join("b"), "").dialog);
        assert!(matches!(ui.modal, Some(Modal::Environments(_))));
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
