//! Importing an OpenAPI description (R-COL-15), a Postman collection (R-COL-16),
//! `.http` files (R-COL-17, several at once R-COL-29), a Bruno collection (R-COL-31) or
//! an Insomnia export (R-COL-32) as a new project, and Postman (R-COL-20), HTTP Client (R-COL-29) or Bruno
//! (R-COL-31) environments into a new or an existing project.

use std::path::{Path, PathBuf};

use tortuga_core::collection::MANIFEST_FILE;
use tortuga_core::import::{self, Import, ImportKind, ImportedEnvironment};
use tortuga_core::settings::ImporterSettings;
use tortuga_core::storage::Storage;
use tortuga_core::{bruno, http_file, postman};

use super::{Modal, TortugaUi};
use crate::components::{
    DialogResult, EnvironmentImportDialog, EnvironmentsDialog, ImportAction, ImportDialog,
    ImportTarget, file_names, importer_name, kind_label,
};

/// The JetBrains HTTP Client's environment files, found next to `.http` files and
/// added to their import (R-COL-29).
const HTTP_ENVIRONMENT_FILES: [&str; 2] = ["http-client.env.json", "http-client.private.env.json"];

/// The file picker's title and extensions for environment files of `kind`.
fn pick_environments(kind: ImportKind) -> (String, &'static [&'static str]) {
    match kind {
        ImportKind::Http | ImportKind::OpenApi | ImportKind::Insomnia => (
            t!("import-pick-http-environments"),
            &["json", "code-workspace"],
        ),
        ImportKind::Postman => (t!("import-pick-postman-environments"), &["json"]),
        ImportKind::Bruno => (
            t!("import-pick-bruno-environments"),
            &["bru", "yml", "yaml"],
        ),
    }
}

/// The import dialog with what the shell found out about its folder (checked when
/// the folder changes, not every frame).
#[derive(Debug)]
pub(super) struct OpenImport {
    dialog: ImportDialog,
    /// The file as read, before environment files were added (R-COL-20, R-COL-29).
    read: Import,
    environments: Vec<ImportedEnvironment>,
    taken: bool,
    repository: Option<PathBuf>,
}

impl OpenImport {
    fn new(import: Import, sources: Vec<PathBuf>) -> Self {
        Self {
            dialog: ImportDialog::new(import.clone(), sources),
            read: import,
            environments: Vec::new(),
            taken: false,
            repository: None,
        }
    }
}

/// The dialog for environments imported into a project, with the files it read: the
/// import is worked out again from the project as it is when confirmed.
#[derive(Debug)]
pub(super) struct OpenEnvironmentImport {
    dialog: EnvironmentImportDialog,
    files: Vec<ImportedEnvironment>,
}

/// Reads environment files of `kind`: Postman's (R-COL-20), one environment each, or
/// the HTTP clients' (R-COL-29), with an environment per name in them.
fn read_environments(
    storage: &dyn Storage,
    kind: ImportKind,
    paths: &[PathBuf],
) -> Result<Vec<ImportedEnvironment>, String> {
    let mut out = Vec::new();
    for path in paths {
        let read = storage
            .read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                let parsed = match kind {
                    ImportKind::Postman => postman::parse_environment(&text).map(|e| vec![e]),
                    ImportKind::Http | ImportKind::OpenApi | ImportKind::Insomnia => {
                        http_file::parse_environments(&text, &file_name(path))
                    }
                    ImportKind::Bruno => {
                        bruno::parse_environment_file(&text, &file_name(path)).map(|e| vec![e])
                    }
                };
                parsed.map_err(|e| tortuga_core::error::message(&e))
            })
            .map_err(|e| {
                t!(
                    "import-failed-file",
                    file = path.display().to_string(),
                    error = e
                )
            })?;
        out.extend(read);
    }
    Ok(out)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Picked with `.http` files: an environment file rather than requests (R-COL-29).
fn is_environment_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    http_file::is_environment_file_name(&file_name(path))
        || matches!(extension.as_deref(), Some("json" | "code-workspace"))
}

/// Reads one file of any kind, several `.http` files as one project named after their
/// folder (R-COL-29), or a Bruno collection's folder (R-COL-31).
fn read_import(
    storage: &dyn Storage,
    paths: &[PathBuf],
    kind: ImportKind,
) -> Result<Import, String> {
    if let [dir] = paths
        && (kind == ImportKind::Bruno || bruno::is_collection(storage, dir))
    {
        return bruno::read(storage, dir).map_err(|e| tortuga_core::error::message(&e));
    }
    let texts = paths
        .iter()
        .map(|path| {
            storage
                .read_to_string(path)
                .map_err(|e| format!("{}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parsed = if let ([path], [text]) = (paths, texts.as_slice()) {
        import::parse(text, &file_name(path), kind)
    } else {
        let names: Vec<String> = paths.iter().map(|p| file_name(p)).collect();
        let files: Vec<(&str, &str)> = texts
            .iter()
            .zip(&names)
            .map(|(t, n)| (t.as_str(), n.as_str()))
            .collect();
        let dir = paths
            .first()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new(""));
        http_file::parse_files(&files, &http_file::folder_project_name(dir))
    };
    parsed.map_err(|e| tortuga_core::error::message(&e))
}

/// A Bruno collection's folder for its picked `bruno.json` or `opencollection.yml`
/// (R-COL-31, R-COL-34), else `path`.
fn collection_folder(path: &Path) -> PathBuf {
    match path.parent() {
        Some(dir) if bruno::is_collection_file(&file_name(path)) => dir.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

/// The HTTP Client environment files next to `files` that `picked` doesn't have yet.
fn environment_files_next_to(
    storage: &dyn Storage,
    files: &[PathBuf],
    picked: &[PathBuf],
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in files.iter().filter_map(|f| f.parent()) {
        for name in HTTP_ENVIRONMENT_FILES {
            let path = dir.join(name);
            if !picked.contains(&path) && !out.contains(&path) && storage.exists(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// `import`, unless it was recognised as a kind whose importer is off (R-COL-18).
fn allowed(importers: &ImporterSettings, import: Import) -> Result<Import, String> {
    if importers.enabled(import.kind) {
        Ok(import)
    } else {
        Err(t!(
            "import-turned-off",
            kind = kind_label(import.kind),
            importer = importer_name(import.kind)
        ))
    }
}

impl TortugaUi {
    /// Picks a file to import (several for `.http` files, R-COL-29), reads it and shows
    /// what the import will make. A file of the other kind is imported as what it is.
    pub(super) fn import_file(&mut self, kind: ImportKind) {
        let paths: Vec<PathBuf> = match kind {
            ImportKind::OpenApi => self
                .state
                .host
                .pick_any_file(&t!("import-pick-openapi"))
                .into_iter()
                .collect(),
            ImportKind::Postman => self
                .state
                .host
                .pick_any_file(&t!("import-pick-postman"))
                .into_iter()
                .collect(),
            ImportKind::Http => self.state.host.pick_any_files(
                &t!("import-pick-http"),
                &["http", "rest", "json", "code-workspace"],
            ),
            ImportKind::Insomnia => self
                .state
                .host
                .pick_any_file(&t!("import-pick-insomnia"))
                .into_iter()
                .collect(),
            ImportKind::Bruno => self
                .state
                .host
                .pick_folder(&t!("import-pick-bruno"))
                .into_iter()
                .collect(),
        };
        if !paths.is_empty() {
            self.import_paths(kind, &paths);
        }
    }

    /// Reads the picked files and shows the import dialog. With several, environment
    /// files among them, and those next to the `.http` files, become the project's
    /// environments (R-COL-29).
    fn import_paths(&mut self, kind: ImportKind, paths: &[PathBuf]) {
        let (mut environments, files): (Vec<PathBuf>, Vec<PathBuf>) = if paths.len() > 1 {
            paths.iter().cloned().partition(|p| is_environment_path(p))
        } else {
            (
                Vec::new(),
                paths.iter().map(|p| collection_folder(p)).collect(),
            )
        };
        if files.is_empty() {
            self.state.notify(
                "Could not import: choose the .http files together with their environment \
                 files, or import environments into a project from its menu.",
            );
            return;
        }
        let read = read_import(self.state.storage(), &files, kind)
            .and_then(|import| allowed(&self.state.settings.importers, import));
        let import = match read {
            Ok(import) => import,
            Err(e) => {
                let what = match files.as_slice() {
                    [one] => one.display().to_string(),
                    _ => file_names(&files).join(", "),
                };
                self.state
                    .notify(t!("import-failed-file", file = what, error = e));
                return;
            }
        };
        let is_http = import.kind == ImportKind::Http;
        let mut open = OpenImport::new(import, files);
        self.check_import_folder(&mut open);
        if is_http {
            let found = environment_files_next_to(
                self.state.storage(),
                &open.dialog.sources,
                &environments,
            );
            environments.extend(found);
            self.add_environment_files(&mut open, &environments);
        }
        self.modal = Some(Modal::Import(Box::new(open)));
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
                    .pick_folder(&t!("import-pick-project-folder"))
                {
                    open.dialog.dir = dir;
                    self.check_import_folder(open);
                }
            }
            Some(ImportAction::AddEnvironments) => {
                let (title, extensions) = pick_environments(open.read.kind);
                let paths = self.state.host.pick_any_files(&title, extensions);
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
            self.state.notify(t!(
                "import-failed",
                error = tortuga_core::error::message(&e)
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
        let name = self
            .state
            .collection(dir)
            .map_or_else(String::new, |c| c.name().to_owned());
        self.state.inform(t!("import-done", name = name.as_str()));
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
            self.state.notify(t!("import-secrets-not-kept", error = e));
        }
    }

    /// Adds environment files to a Postman collection (R-COL-20) or `.http` import
    /// (R-COL-29): they replace its "Default" environment.
    fn add_environment_files(&mut self, open: &mut OpenImport, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        match read_environments(self.state.storage(), open.read.kind, paths) {
            Ok(files) => {
                open.environments.extend(files);
                open.dialog.environment_files.extend(file_names(paths));
                open.dialog.import = open.read.with_environments(&open.environments);
            }
            Err(e) => self.state.notify(e),
        }
    }

    /// Picks environment files of `kind` and shows what importing them into the project
    /// at `root` changes (R-COL-20, R-COL-29).
    pub(super) fn import_environments(&mut self, root: PathBuf, kind: ImportKind) {
        let (title, extensions) = pick_environments(kind);
        let paths = self.state.host.pick_any_files(&title, extensions);
        if !paths.is_empty() {
            self.review_environments(root, kind, &paths);
        }
    }

    fn review_environments(&mut self, root: PathBuf, kind: ImportKind, paths: &[PathBuf]) {
        let files = match read_environments(self.state.storage(), kind, paths) {
            Ok(files) => files,
            Err(e) => {
                self.state.notify(e);
                return;
            }
        };
        let Some(c) = self.state.collection(&root) else {
            return;
        };
        let preview = import::import_environments(&c.manifest, &c.environments, &files);
        let dialog = EnvironmentImportDialog {
            kind,
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
        let result = import::import_environments(&c.manifest, &c.environments, &open.files);
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
            vec![dir.join("openapi.json")],
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
        OpenImport::new(import, vec![dir.join("shop.postman_collection.json")])
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
        OpenImport::new(import, vec![dir.join("shop.postman_collection.json")])
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
        ui.review_environments(root.clone(), ImportKind::Postman, &[default, staging]);
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

    /// R-COL-31: a Bruno collection's folder (or its picked `bruno.json`) is imported
    /// next to it, never into it; its environments come along, and Bruno's secrets
    /// are asked for afterwards. A folder that isn't one is refused by name.
    #[test]
    fn bruno_collections_import_from_their_folder() {
        let base = tempfile::tempdir().expect("dir");
        let dir = base.path().join("shop-api");
        std::fs::create_dir_all(dir.join("environments")).expect("dir");
        let config = write(&dir, "bruno.json", r#"{"name": "Shop API"}"#);
        write(
            &dir,
            "list.bru",
            "meta {\n  name: List ü😀\n}\n\nget {\n  url: {{baseUrl}}/users\n  auth: bearer\n}\n\nauth:bearer {\n  token: {{token}}\n}\n",
        );
        write(
            &dir.join("environments"),
            "Local.bru",
            "vars {\n  baseUrl: http://localhost:3000\n}\nvars:secret [\n  token\n]\n",
        );

        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Http, &[config]);
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        assert_eq!(open.dialog.import.kind, ImportKind::Bruno);
        assert_eq!(open.dialog.sources, std::slice::from_ref(&dir));
        // The slug is the collection's own folder name, so the project goes beside it.
        assert_eq!(open.dialog.dir, base.path().join("shop-api-tortuga"));

        ui.import_paths(ImportKind::Bruno, std::slice::from_ref(&dir));
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        ui.finish_import(&open.dialog);
        let root = base.path().join("shop-api-tortuga");
        let project = ui.state.collection(&root).expect("added");
        assert_eq!(project.environments[0].name, "Local");
        assert_eq!(project.manifest.secrets, ["token"]);
        assert!(
            matches!(ui.modal, Some(Modal::Environments(_))),
            "Bruno keeps token's value"
        );
        assert!(
            !dir.join(MANIFEST_FILE).exists(),
            "the collection is not changed"
        );

        let empty = base.path().join("empty");
        std::fs::create_dir_all(&empty).expect("dir");
        ui.modal = None;
        ui.import_paths(ImportKind::Bruno, &[empty]);
        assert!(ui.modal.is_none());
        let notice = format!("{:?}", ui.state.notice);
        assert!(notice.contains("has no bruno.json"), "{notice}");
    }

    /// R-COL-34: a Bruno collection in YAML, picked as its `opencollection.yml`, is
    /// imported from its folder with its environments and examples.
    #[test]
    fn yaml_bruno_collections_import_from_their_folder() {
        let base = tempfile::tempdir().expect("dir");
        let dir = base.path().join("shop");
        std::fs::create_dir_all(dir.join("environments")).expect("dir");
        let config = write(
            &dir,
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: Shop ü😀\n",
        );
        write(
            &dir,
            "list.yml",
            "info:\n  name: List\n  type: http\nhttp:\n  method: GET\n  url: \"{{baseUrl}}/users\"\nexamples:\n  - name: Two\n    response:\n      status: 200\n      body:\n        type: json\n        data: \"[1, 2]\"\n",
        );
        write(
            &dir.join("environments"),
            "Local.yml",
            "name: Local\nvariables:\n  - name: baseUrl\n    value: http://localhost:3000\n",
        );
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Postman, &[config]);
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        assert_eq!(open.dialog.import.kind, ImportKind::Bruno);
        assert_eq!(open.dialog.sources, std::slice::from_ref(&dir));
        ui.finish_import(&open.dialog);
        let project = ui.state.collection(&open.dialog.dir).expect("added");
        assert_eq!(project.name(), "Shop ü😀");
        assert_eq!(project.environments[0].name, "Local");
    }

    /// R-COL-32: an Insomnia export becomes a project beside it, with its
    /// sub-environments and their values; a private environment's go to the secret store.
    #[test]
    fn insomnia_exports_import_with_their_environments() {
        let base = tempfile::tempdir().expect("dir");
        let file = write(
            base.path(),
            "Insomnia_2026-10-21.yaml",
            "type: collection.insomnia.rest/5.0\nname: Shop ü😀\ncollection:\n  - url: \"{{ _.base_url }}/users\"\n    name: List users\n    method: GET\n    authentication:\n      type: bearer\n      token: \"{{ _.token }}\"\nenvironments:\n  name: Base Environment\n  data:\n    base_url: https://shop.test\n  subEnvironments:\n    - name: Prod\n      isPrivate: true\n      data:\n        token: t-prod\n",
        );
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Insomnia, &[file]);
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        assert_eq!(open.dialog.import.kind, ImportKind::Insomnia);
        ui.finish_import(&open.dialog);
        let root = open.dialog.dir.clone();
        assert_eq!(root.parent(), Some(base.path()));
        let project = ui.state.collection(&root).expect("added");
        assert_eq!(project.environments[0].name, "Prod");
        assert_eq!(project.manifest.secrets, ["token"]);
        let stored = &ui.state.secrets[&root].values;
        assert_eq!(
            stored[&(Some(project.environments[0].id.clone()), "token".to_owned())],
            "t-prod"
        );
    }

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).expect("write");
        path
    }

    /// R-COL-29: several `.http` files are one project named after their folder, a
    /// folder per file; the HTTP Client environment files next to them become its
    /// environments, the private file's values going to the secret store.
    #[test]
    fn several_http_files_import_with_the_environment_files_next_to_them() {
        let base = tempfile::tempdir().expect("dir");
        let dir = base.path().join("shop-api");
        std::fs::create_dir_all(&dir).expect("dir");
        let orders = write(
            &dir,
            "orders.http",
            "### Orders\nGET {{host}}/orders\nAuthorization: Bearer {{token}}\n",
        );
        let users = write(&dir, "users.http", "### Users\nGET {{host}}/users\n");
        write(
            &dir,
            "http-client.env.json",
            r#"{"dev": {"host": "http://localhost:3000"}, "prod": {"host": "https://shop.test"}}"#,
        );
        write(
            &dir,
            "http-client.private.env.json",
            r#"{"dev": {"token": "t-dev-ü😀"}}"#,
        );
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Http, &[orders, users]);
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        assert_eq!(
            open.dialog.environment_files,
            ["http-client.env.json", "http-client.private.env.json"]
        );
        let import = &open.dialog.import;
        assert_eq!(import.manifest.name, "Shop api");
        let names: Vec<_> = import
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["dev", "prod"]);

        ui.finish_import(&open.dialog);
        let root = dir.join("shop-api");
        let project = ui.state.collection(&root).expect("added");
        assert_eq!(project.items.len(), 2, "a folder per file");
        assert_eq!(
            project.environment("dev").expect("dev").servers["host"],
            "http://localhost:3000"
        );
        let stored = &ui.state.secrets[&root].values;
        assert_eq!(
            stored[&(Some("dev".to_owned()), "token".to_owned())],
            "t-dev-ü😀"
        );
        assert!(
            matches!(ui.modal, Some(Modal::Environments(_))),
            "prod has no token"
        );
        let env = std::fs::read_to_string(root.join("environments").join("dev.toml")).expect("dev");
        assert!(!env.contains("t-dev"), "{env}");
    }

    /// R-COL-29: environment files picked without `.http` files are refused with a hint.
    #[test]
    fn environment_files_alone_are_not_an_import() {
        let dir = tempfile::tempdir().expect("dir");
        let env = write(dir.path(), "http-client.env.json", r#"{"dev": {}}"#);
        let private = write(dir.path(), "http-client.private.env.json", r#"{"dev": {}}"#);
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Http, &[env.clone(), private]);
        assert!(ui.modal.is_none());
        assert!(
            ui.state
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("together with their environment files"))
        );
        ui.state.notice = None;
        ui.import_paths(ImportKind::Http, &[env]);
        assert!(
            ui.state
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("environment file of the HTTP clients"))
        );
    }

    /// R-COL-29, D49: REST Client environments from a VS Code settings file go into an
    /// existing project: the one with the same name is updated, the others added.
    #[test]
    fn http_environments_import_into_an_existing_project() {
        let dir = tempfile::tempdir().expect("dir");
        let http = write(dir.path(), "shop.http", "### Ping\nGET {{baseUrl}}/ping\n");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.import_paths(ImportKind::Http, &[http]);
        let Some(Modal::Import(open)) = ui.modal.take() else {
            panic!("the dialog opens");
        };
        ui.finish_import(&open.dialog);
        let root = dir.path().join("shop");
        let settings = write(
            dir.path(),
            "settings.json",
            r#"{
              // REST Client
              "rest-client.environmentVariables": {
                "Default": {"baseUrl": "http://localhost:8080"},
                "staging": {"baseUrl": "https://staging.test", "home": "{{$processEnv HOME}}"},
              }
            }"#,
        );
        ui.review_environments(root.clone(), ImportKind::Http, &[settings]);
        let Some(Modal::EnvironmentImport(open)) = ui.modal.take() else {
            panic!("the dialog opens: {:?}", ui.state.notice);
        };
        assert_eq!(open.dialog.kind, ImportKind::Http);
        assert_eq!(open.dialog.preview.updated, ["Default"]);
        assert_eq!(open.dialog.preview.added, ["staging"]);
        assert_eq!(open.dialog.preview.warnings.len(), 1);
        ui.finish_environment_import(&open);
        let project = ui.state.collection(&root).expect("project");
        let urls: Vec<_> = project
            .environments
            .iter()
            .map(|e| e.servers["baseurl"].as_str())
            .collect();
        assert_eq!(urls, ["http://localhost:8080", "https://staging.test"]);
    }
}
