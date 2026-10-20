//! Importing an OpenAPI description (R-COL-15), a Postman collection (R-COL-16) or a
//! `.http` file (R-COL-17) as a new project, and Postman environments into a new or an
//! existing project (R-COL-20).

use std::path::{Path, PathBuf};

use tortuga_core::collection::MANIFEST_FILE;
use tortuga_core::import::{self, Import, ImportKind};
use tortuga_core::postman::{self, PostmanEnvironment};
use tortuga_core::settings::ImporterSettings;

use super::{Modal, TortugaUi};
use crate::components::{
    DialogResult, EnvironmentImportDialog, EnvironmentsDialog, ImportAction, ImportDialog,
    ImportTarget,
};

const PICK_ENVIRONMENTS: &str = "Choose Postman environments (exported as JSON)";

/// The import dialog with what the shell found out about its folder (checked when
/// the folder changes, not every frame).
#[derive(Debug)]
pub(super) struct OpenImport {
    dialog: ImportDialog,
    /// The file as read, before environment files were added (R-COL-20).
    read: Import,
    environments: Vec<PostmanEnvironment>,
    taken: bool,
    repository: Option<PathBuf>,
}

impl OpenImport {
    fn new(import: Import, source: PathBuf) -> Self {
        Self {
            dialog: ImportDialog::new(import.clone(), source),
            read: import,
            environments: Vec::new(),
            taken: false,
            repository: None,
        }
    }
}

/// The dialog for Postman environments imported into a project, with the files it
/// read: the import is worked out again from the project as it is when confirmed.
#[derive(Debug)]
pub(super) struct OpenEnvironmentImport {
    dialog: EnvironmentImportDialog,
    files: Vec<PostmanEnvironment>,
}

/// Reads Postman environment files (R-COL-20).
fn read_environments(
    storage: &dyn tortuga_core::storage::Storage,
    paths: &[PathBuf],
) -> Result<Vec<PostmanEnvironment>, String> {
    paths
        .iter()
        .map(|path| {
            storage
                .read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    postman::parse_environment(&text).map_err(|e| tortuga_core::error::message(&e))
                })
                .map_err(|e| format!("Could not import {}: {e}", path.display()))
        })
        .collect()
}

fn file_names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// `import`, unless it was recognised as a kind whose importer is off (R-COL-18).
fn allowed(importers: &ImporterSettings, import: Import) -> Result<Import, String> {
    if importers.enabled(import.kind) {
        Ok(import)
    } else {
        Err(format!(
            "it is a {}, and importing {} is turned off in Settings",
            import.kind.label(),
            import.kind.importer_name()
        ))
    }
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
            })
            .and_then(|import| allowed(&self.state.settings.importers, import));
        match read {
            Ok(import) => {
                let mut open = OpenImport::new(import, path);
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
        let (result, action) = open.dialog.show(ctx, target);
        match action {
            Some(ImportAction::ChooseFolder) => {
                if let Some(dir) = self
                    .state
                    .host
                    .pick_folder("Choose or create a folder for the project")
                {
                    open.dialog.dir = dir;
                    self.check_import_folder(open);
                }
            }
            Some(ImportAction::AddEnvironments) => {
                let paths = self.state.host.pick_any_files(PICK_ENVIRONMENTS);
                self.add_environment_files(open, &paths);
            }
            None => {}
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
        let import = &dialog.import;
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
        let found = import
            .secret_values
            .iter()
            .map(|(name, value)| ((None, name.clone()), value.clone()))
            .chain(
                import
                    .environment_secret_values
                    .iter()
                    .map(|((env, name), value)| ((Some(env.clone()), name.clone()), value.clone())),
            );
        self.keep_secrets(dir, found);
        let missing = import
            .manifest
            .secrets
            .iter()
            .any(|name| !import.has_secret_value(name));
        if missing {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                dir.clone(),
            )));
        }
    }
}

impl TortugaUi {
    /// Keeps secret values on this machine (D35); they replace stored ones.
    fn keep_secrets(
        &mut self,
        root: &Path,
        found: impl Iterator<Item = ((Option<String>, String), String)>,
    ) {
        let mut found = found.peekable();
        if found.peek().is_none() {
            return;
        }
        let mut values = self.state.secrets.get(root).cloned().unwrap_or_default();
        values.values.extend(found);
        if let Err(e) = self.state.save_secrets(root, values) {
            self.state
                .notify(format!("Could not keep the secret values: {e}"));
        }
    }

    /// Adds Postman environment files to a collection import (R-COL-20): they
    /// replace its "Default" environment.
    fn add_environment_files(&mut self, open: &mut OpenImport, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        match read_environments(self.state.storage(), paths) {
            Ok(files) => {
                open.environments.extend(files);
                open.dialog.import = open.read.with_environments(&open.environments);
            }
            Err(e) => self.state.notify(e),
        }
    }

    /// Picks Postman environment files and shows what importing them into the project
    /// at `root` changes (R-COL-20).
    pub(super) fn import_environments(&mut self, root: PathBuf) {
        let paths = self.state.host.pick_any_files(PICK_ENVIRONMENTS);
        if !paths.is_empty() {
            self.review_environments(root, &paths);
        }
    }

    fn review_environments(&mut self, root: PathBuf, paths: &[PathBuf]) {
        let files = match read_environments(self.state.storage(), paths) {
            Ok(files) => files,
            Err(e) => {
                self.state.notify(e);
                return;
            }
        };
        let Some(c) = self.state.collection(&root) else {
            return;
        };
        let preview = postman::import_environments(&c.manifest, &c.environments, &files);
        let dialog = EnvironmentImportDialog {
            project: c.name().to_owned(),
            root,
            sources: file_names(paths),
            preview,
        };
        self.modal = Some(Modal::EnvironmentImport(Box::new(OpenEnvironmentImport {
            dialog,
            files,
        })));
    }

    /// Shows the dialog for environments imported into a project. Returns whether it
    /// stays open.
    pub(super) fn show_environment_import(
        &mut self,
        ctx: &egui::Context,
        open: &OpenEnvironmentImport,
    ) -> bool {
        match open.dialog.show(ctx) {
            DialogResult::Done(()) => {
                self.finish_environment_import(open);
                false
            }
            result => result == DialogResult::Open,
        }
    }

    /// Writes the environments and keeps their secret values on this machine; with a
    /// secret still without a value somewhere, the environments dialog opens.
    fn finish_environment_import(&mut self, open: &OpenEnvironmentImport) {
        let root = &open.dialog.root;
        let Some(c) = self.state.collection(root) else {
            return;
        };
        // What is on disk now, which may have changed while the dialog was open.
        let result = postman::import_environments(&c.manifest, &c.environments, &open.files);
        let saved = self.state.run(root, |c, s| {
            c.save_config(s, &result.manifest, &result.environments)
        });
        if saved.is_none() {
            return;
        }
        let stored = self
            .state
            .secrets
            .get(root)
            .map(|s| s.values.clone())
            .unwrap_or_default();
        let complete = result.secrets_complete(&stored);
        // A project variable that became secret keeps a stored default (D35).
        let found = result
            .secret_values
            .into_iter()
            .filter(|(key, _)| key.0.is_some() || !stored.contains_key(key));
        self.keep_secrets(root, found);
        if !complete {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                root.clone(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;

    /// R-COL-18: a file recognised as a kind whose importer is off is refused.
    #[test]
    fn files_of_a_turned_off_importer_are_refused() {
        let text = r#"{"info": {"name": "Shop"}, "item": []}"#;
        // Picked with "Import OpenAPI…", recognised as a Postman collection.
        let import = import::parse(text, "shop.json", ImportKind::OpenApi).expect("parse");
        assert_eq!(import.kind, ImportKind::Postman);
        let mut importers = ImporterSettings::default();
        assert!(allowed(&importers, import.clone()).is_ok());
        importers.set_enabled(ImportKind::Postman, false);
        let err = allowed(&importers, import).expect_err("off");
        assert!(err.contains("Postman collections is turned off"), "{err}");
    }

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
        let mut open = OpenImport::new(
            import::parse(spec, "openapi.json", ImportKind::OpenApi).expect("parse"),
            dir.join("openapi.json"),
        );
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
        OpenImport::new(import, dir.join("shop.postman_collection.json"))
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

    fn environment_file(dir: &Path, name: &str, base: &str, token: &str) -> PathBuf {
        let path = dir.join(format!("{name}.postman_environment.json"));
        let text = format!(
            r#"{{"name": "{name}", "_postman_variable_scope": "environment",
               "values": [{{"key": "baseUrl", "value": "{base}", "enabled": true}},
                          {{"key": "token", "value": "{token}", "type": "secret"}}]}}"#
        );
        std::fs::write(&path, text).expect("write");
        path
    }

    fn shop(dir: &Path) -> OpenImport {
        let collection = r#"{"info": {"name": "Shop"},
            "item": [{"name": "List", "request": {"method": "GET", "url": "{{baseUrl}}/orders"}}]}"#;
        let import = import::parse(collection, "shop.json", ImportKind::Postman).expect("parse");
        OpenImport::new(import, dir.join("shop.postman_collection.json"))
    }

    /// R-COL-20: environment files added to a collection import replace "Default";
    /// base URLs go to the server, secret values to the secret store per environment.
    #[test]
    fn environment_files_join_a_collection_import() {
        let dir =
            std::env::temp_dir().join(format!("tortuga-ui-postman-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let staging = environment_file(&dir, "Staging", "https://staging.test", "st-ü😀");
        let prod = environment_file(&dir, "Production", "https://shop.test", "pr-1");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        let mut open = shop(&dir);
        ui.add_environment_files(&mut open, &[staging]);
        ui.add_environment_files(&mut open, &[prod]);
        let names: Vec<_> = open
            .dialog
            .import
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["Staging", "Production"]);

        let broken = dir.join("broken.json");
        std::fs::write(&broken, "{").expect("write");
        ui.add_environment_files(&mut open, &[broken]);
        assert!(
            ui.state
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("not valid JSON"))
        );
        assert_eq!(open.dialog.import.environments.len(), 2, "unchanged");

        ui.finish_import(&open.dialog);
        assert!(ui.modal.is_none(), "every secret has a value");
        let root = dir.join("shop");
        let project = ui.state.collection(&root).expect("added");
        let staging = project.environment("staging").expect("staging");
        assert_eq!(staging.servers["baseurl"], "https://staging.test");
        let stored = &ui.state.secrets[&root].values;
        assert_eq!(
            stored[&(Some("staging".to_owned()), "token".to_owned())],
            "st-ü😀"
        );
        assert_eq!(
            stored[&(Some("production".to_owned()), "token".to_owned())],
            "pr-1"
        );
        for file in ["staging.toml", "production.toml"] {
            let text = std::fs::read_to_string(root.join("environments").join(file)).expect("env");
            assert!(!text.contains("st-") && !text.contains("pr-1"), "{text}");
        }
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-COL-20, D49: environments imported into an existing project add new ones and
    /// update the one with the same name; a secret without a value opens Environments.
    #[test]
    fn environments_import_into_an_existing_project() {
        let dir =
            std::env::temp_dir().join(format!("tortuga-ui-postman-envs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.finish_import(&shop(&dir).dialog);
        let root = dir.join("shop");
        ui.modal = None;

        let default = environment_file(&dir, "Default", "https://dev.test", "dev-token");
        let staging = environment_file(&dir, "Staging", "https://staging.test", "");
        ui.review_environments(root.clone(), &[default, staging]);
        let Some(Modal::EnvironmentImport(open)) = ui.modal.take() else {
            panic!("the dialog opens");
        };
        assert_eq!(open.dialog.preview.updated, ["Default"]);
        assert_eq!(open.dialog.preview.added, ["Staging"]);
        assert_eq!(open.dialog.project, "Shop");
        ui.finish_environment_import(&open);

        let project = ui.state.collection(&root).expect("project");
        let names: Vec<_> = project
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["Default", "Staging"]);
        assert_eq!(
            project.environments[0].servers["baseurl"],
            "https://dev.test"
        );
        assert_eq!(project.manifest.secrets, ["token"]);
        let stored = &ui.state.secrets[&root].values;
        assert_eq!(
            stored[&(Some("default".to_owned()), "token".to_owned())],
            "dev-token"
        );
        assert!(
            matches!(ui.modal, Some(Modal::Environments(_))),
            "Staging's token is empty"
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
