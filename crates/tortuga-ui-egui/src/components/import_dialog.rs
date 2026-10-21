//! The import dialog for OpenAPI descriptions (R-COL-15), Postman collections
//! (R-COL-16, with environment files, R-COL-20) and `.http` files (R-COL-17, several
//! with environment files, R-COL-29): what the files become, where the new project
//! goes, and what could not be imported as written. Also the dialog for environments
//! imported into a project (R-COL-20, R-COL-29).

use std::path::{Path, PathBuf};

use egui::{Context, RichText, Ui};
use tortuga_core::NewItem;
use tortuga_core::import::EnvironmentImport;
use tortuga_core::import::{Import, ImportKind};

use super::{DialogResult, dialog};
use crate::format::count;
use crate::theme;

/// The importers whose environment files can be imported (R-COL-20, R-COL-29).
pub const ENVIRONMENT_KINDS: [ImportKind; 3] =
    [ImportKind::Postman, ImportKind::Http, ImportKind::Bruno];

/// The menu entry that imports environments of `kind` into a project.
pub fn environments_entry(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Postman => "Import Postman environments…",
        ImportKind::Http => "Import HTTP environments…",
        ImportKind::Bruno => "Import Bruno environments…",
        // Their environments come with them only.
        ImportKind::OpenApi | ImportKind::Insomnia => "Import environments…",
    }
}

/// What "Add environment files…" takes for an import of `kind`.
fn environments_hover(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Postman => {
            "Postman environments exported as JSON become this project's environments"
        }
        ImportKind::Http | ImportKind::OpenApi | ImportKind::Insomnia => {
            "http-client.env.json, http-client.private.env.json or a VS Code settings file with \
             REST Client environments: they become this project's environments"
        }
        ImportKind::Bruno => {
            "Bruno environments (.bru files) from another collection become this project's \
             environments too"
        }
    }
}

/// What the shell must tell the dialog about the folder it would write to.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportTarget<'a> {
    /// The folder already holds a project (D34: an import never writes into one).
    pub taken: bool,
    /// The git repository the folder is in, if any.
    pub repository: Option<&'a Path>,
    pub can_pick_folders: bool,
}

/// What the import dialog asks the shell for, besides importing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAction {
    /// "Choose…": another folder for the project.
    ChooseFolder,
    /// Environment files for the new project (R-COL-20, R-COL-29).
    AddEnvironments,
}

/// Reviews an import before the project is written.
#[derive(Debug, Clone)]
pub struct ImportDialog {
    pub import: Import,
    /// The files it came from: one, or several `.http` files (R-COL-29).
    pub sources: Vec<PathBuf>,
    /// Names of the environment files added to it (R-COL-20, R-COL-29).
    pub environment_files: Vec<String>,
    /// Where the project will be created.
    pub dir: PathBuf,
}

impl ImportDialog {
    /// Proposes a folder named after the API next to the (first) file.
    pub fn new(import: Import, sources: Vec<PathBuf>) -> Self {
        let name = tortuga_core::slug::slugify(&import.manifest.name);
        let name = if name.is_empty() { "api".into() } else { name };
        let source = sources.first().map(PathBuf::as_path);
        let parent = source
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""));
        // Never into the imported folder itself (a Bruno collection, R-COL-31).
        let dir = Some(parent.join(&name))
            .filter(|d| Some(d.as_path()) != source)
            .unwrap_or_else(|| parent.join(format!("{name}-tortuga")));
        Self {
            import,
            sources,
            environment_files: Vec::new(),
            dir,
        }
    }

    /// What is imported from, and that it stays as it is.
    fn source_line(&self) -> String {
        let files = file_names(&self.sources).join(", ");
        if self.import.kind == ImportKind::Bruno {
            format!(
                "From the folder {files} (Bruno collection). Tortuga creates a new project from \
                 it; the collection itself is not changed."
            )
        } else if self.sources.len() == 1 {
            format!(
                "From {files} ({}). Tortuga creates a new project from it; the file itself is \
                 not changed.",
                self.import.kind.label()
            )
        } else {
            format!(
                "From {files}. Tortuga creates a new project from them, with a folder per file; \
                 the files themselves are not changed."
            )
        }
    }

    /// Done means "import"; the action is what else was clicked.
    pub fn show(
        &self,
        ctx: &Context,
        target: ImportTarget<'_>,
    ) -> (DialogResult<()>, Option<ImportAction>) {
        let mut action = None;
        let title = match self.import.kind {
            ImportKind::OpenApi => "Import OpenAPI",
            ImportKind::Postman => "Import Postman collection",
            ImportKind::Http => "Import HTTP file",
            ImportKind::Bruno => "Import Bruno collection",
            ImportKind::Insomnia => "Import Insomnia export",
        };
        let result = dialog(ctx, "import", title, 560.0, true, |ui| {
            theme::hint(ui, self.source_line());
            ui.add_space(8.0);
            self.summary(ui);
            if ENVIRONMENT_KINDS.contains(&self.import.kind)
                && target.can_pick_folders
                && ui
                    .button("Add environment files…")
                    .on_hover_text(environments_hover(self.import.kind))
                    .clicked()
            {
                action = Some(ImportAction::AddEnvironments);
            }

            ui.add_space(12.0);
            theme::section_label(ui, "Project folder");
            ui.horizontal(|ui| {
                if target.can_pick_folders && ui.button("Choose…").clicked() {
                    action = Some(ImportAction::ChooseFolder);
                }
                let full = self.dir.display().to_string();
                ui.add(egui::Label::new(path_end(&full, 60)).truncate())
                    .on_hover_text(full);
            });
            if target.taken {
                theme::error(
                    ui,
                    "That folder already is a Tortuga project: choose another one.",
                );
            } else if let Some(repo) = target.repository {
                theme::hint(
                    ui,
                    format!(
                        "Inside the git repository at {}: commit the project with your code.",
                        repo.display()
                    ),
                );
            }

            if !self.import.warnings.is_empty() {
                ui.add_space(12.0);
                theme::section_label(
                    ui,
                    &format!("Not imported as written ({})", self.import.warnings.len()),
                );
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for warning in &self.import.warnings {
                            theme::callout(ui, warning);
                            ui.add_space(4.0);
                        }
                    });
            }

            ui.add_space(16.0);
            theme::button_row(ui, |ui| {
                let import = ui
                    .add_enabled(!target.taken, theme::primary_button(ui, "Import"))
                    .clicked();
                let cancel = ui.button("Cancel").clicked();
                if import {
                    DialogResult::Done(())
                } else if cancel {
                    DialogResult::Close
                } else {
                    DialogResult::Open
                }
            })
        });
        (result, action)
    }

    fn summary(&self, ui: &mut Ui) {
        let import = &self.import;
        let folders = import
            .items
            .iter()
            .filter(|i| matches!(i, NewItem::Folder { .. }))
            .count();
        let requests = if folders == 0 {
            count(import.requests, "request")
        } else {
            format!(
                "{} in {}",
                count(import.requests, "request"),
                count(folders, "folder")
            )
        };
        let environments = import
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let secrets = if import.manifest.secrets.is_empty() {
            "None".to_owned()
        } else {
            import.manifest.secrets.join(", ")
        };
        let environments = if environments.is_empty() {
            "None".to_owned()
        } else {
            environments
        };
        egui::Grid::new("import-summary")
            .num_columns(2)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                let row = |ui: &mut Ui, label: &str, value: &str| {
                    ui.label(RichText::new(label).color(theme::palette(ui).muted));
                    ui.label(value);
                    ui.end_row();
                };
                row(ui, "Project", &import.manifest.name);
                row(ui, "Requests", &requests);
                row(ui, "Environments", &environments);
                if !self.environment_files.is_empty() {
                    row(ui, "Environment files", &self.environment_files.join(", "));
                }
                row(ui, "Secret variables", &secrets);
            });
        if let Some(hint) = secrets_hint(import) {
            theme::hint(ui, hint);
        }
    }
}

/// Where the secret values come from: found in the file and kept on this computer,
/// or to be filled in after the import.
fn secrets_hint(import: &Import) -> Option<String> {
    let secrets = &import.manifest.secrets;
    let found = secrets
        .iter()
        .filter(|s| import.has_secret_value(s))
        .count();
    let missing = secrets.len() - found;
    let kept = "they are kept on this computer, not in the project files";
    match (found, missing) {
        (0, 0) => None,
        (0, _) => Some(
            "Secret values stay on this computer: fill them in under Environments… after the \
             import."
                .to_owned(),
        ),
        (_, 0) => Some(format!("Values for these were found in the file: {kept}.")),
        (_, missing) => Some(format!(
            "Values for {found} of these were found in the file: {kept}. Fill in the other \
             {missing} under Environments… after the import."
        )),
    }
}

/// Reviews environments before they are imported into a project (R-COL-20, R-COL-29).
#[derive(Debug, Clone)]
pub struct EnvironmentImportDialog {
    /// Whose environment files: Postman's or the HTTP clients'.
    pub kind: ImportKind,
    pub root: PathBuf,
    pub project: String,
    /// The files' names.
    pub sources: Vec<String>,
    pub preview: EnvironmentImport,
}

impl EnvironmentImportDialog {
    /// Done means "import".
    pub fn show(&self, ctx: &Context) -> DialogResult<()> {
        let title = environments_entry(self.kind).trim_end_matches('…');
        dialog(ctx, "import-environments", title, 480.0, true, |ui| {
            theme::hint(
                ui,
                format!(
                    "From {}. Base URLs go to the project's servers; secret values stay on \
                         this computer.",
                    self.sources.join(", ")
                ),
            );
            ui.add_space(8.0);
            let preview = &self.preview;
            let list = |names: &[String]| {
                if names.is_empty() {
                    "None".to_owned()
                } else {
                    names.join(", ")
                }
            };
            let mut secrets: Vec<String> = preview
                .secret_values
                .keys()
                .map(|(_, name)| name.clone())
                .collect();
            secrets.sort();
            secrets.dedup();
            egui::Grid::new("environment-import-summary")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    let row = |ui: &mut Ui, label: &str, value: &str| {
                        ui.label(RichText::new(label).color(theme::palette(ui).muted));
                        ui.label(value);
                        ui.end_row();
                    };
                    row(ui, "Project", &self.project);
                    row(ui, "Adds", &list(&preview.added));
                    row(ui, "Updates", &list(&preview.updated));
                    row(ui, "Secret values", &list(&secrets));
                });
            if !preview.updated.is_empty() {
                theme::hint(
                    ui,
                    "Environments that are updated keep what the files don't set.",
                );
            }
            if !secrets.is_empty() {
                theme::hint(
                    ui,
                    "These were found in the files: they are kept on this computer, not in \
                         the project files.",
                );
            }
            if !preview.warnings.is_empty() {
                ui.add_space(12.0);
                theme::section_label(
                    ui,
                    &format!("Not imported as written ({})", preview.warnings.len()),
                );
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for warning in &preview.warnings {
                            theme::callout(ui, warning);
                            ui.add_space(4.0);
                        }
                    });
            }
            ui.add_space(16.0);
            theme::button_row(ui, |ui| {
                let import = ui.add(theme::primary_button(ui, "Import")).clicked();
                let cancel = ui.button("Cancel").clicked();
                if import {
                    DialogResult::Done(())
                } else if cancel {
                    DialogResult::Close
                } else {
                    DialogResult::Open
                }
            })
        })
    }
}

/// The file names of `paths`.
pub fn file_names(paths: &[PathBuf]) -> Vec<String> {
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

/// The last `max` characters of a path, so the folder's own name stays visible.
fn path_end(path: &str, max: usize) -> String {
    let count = path.chars().count();
    if count <= max {
        return path.to_owned();
    }
    let tail: String = path.chars().skip(count - (max - 1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    const SPEC: &str = r#"{"openapi": "3.0.0", "info": {"title": "Pet Store"},
        "servers": [{"url": "/v1"}],
        "security": [{"bearer": []}],
        "components": {"securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}}},
        "paths": {"/pets": {"get": {"tags": ["pets"], "summary": "List pets"}},
                  "/health": {"get": {}}}}"#;

    /// R-COL-15: the dialog shows what the import makes and where, its warnings, and
    /// can't import into a folder that already is a project.
    #[test]
    fn import_dialog_summarises_and_guards_the_folder() {
        let import = tortuga_core::openapi::parse(SPEC).expect("parse");
        let dialog = ImportDialog::new(import, vec![PathBuf::from("/repo/openapi.yaml")]);
        assert_eq!(dialog.dir, Path::new("/repo/pet-store"));
        let mut results = Vec::new();
        let mut taken = false;
        {
            let mut h = crate::test_host::harness(|ui| {
                let target = ImportTarget {
                    taken,
                    repository: Some(Path::new("/repo")),
                    can_pick_folders: true,
                };
                results.push(dialog.show(ui.ctx(), target));
            });
            h.run();
            h.get_by_label("2 requests in 1 folder");
            h.get_by_label("token");
            // As displayed: `join` writes `/repo\pet-store` on Windows.
            h.get_by_label(&dialog.dir.display().to_string());
            h.get_by_label("Not imported as written (1)".to_uppercase().as_str());
            h.get_by_label("Choose…").click();
            h.run();
            h.get_by_label("Import").click();
            h.run();
        }
        assert!(
            results
                .iter()
                .any(|(_, action)| *action == Some(ImportAction::ChooseFolder))
        );
        assert!(results.iter().any(|(r, _)| *r == DialogResult::Done(())));

        taken = true;
        let mut results = Vec::new();
        {
            let mut h = crate::test_host::harness(|ui| {
                let target = ImportTarget {
                    taken,
                    repository: None,
                    can_pick_folders: true,
                };
                results.push(dialog.show(ui.ctx(), target));
            });
            h.run();
            h.get_by_label("That folder already is a Tortuga project: choose another one.");
            h.get_by_label("Import").click();
            h.run();
        }
        assert!(results.iter().all(|(r, _)| *r == DialogResult::Open));
    }

    /// R-COL-16: a Postman collection gets its own title, and the dialog says which
    /// secret values were found in the file.
    #[test]
    fn import_dialog_for_a_postman_collection() {
        let collection = r#"{"info": {"name": "Shop"},
            "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "abc"}]},
            "item": [{"name": "Ping", "request": "https://shop.test/ping"}]}"#;
        let import = tortuga_core::import::parse(collection, "shop.json", ImportKind::Postman)
            .expect("parse");
        let dialog = ImportDialog::new(import, vec![PathBuf::from("/repo/shop.json")]);
        let mut h = crate::test_host::harness(|ui| {
            let _ = dialog.show(ui.ctx(), ImportTarget::default());
        });
        h.run();
        h.get_by_label("Import Postman collection");
        h.get_by_label("1 request");
        h.get_by_label("Default");
        h.get_by_label(
            "Values for these were found in the file: they are kept on this computer, not in \
             the project files.",
        );
    }

    /// R-COL-20, R-COL-29: Postman collection and `.http` imports offer environment
    /// files; OpenAPI imports don't.
    #[test]
    fn import_dialog_adds_environments() {
        let collection = r#"{"info": {"name": "Shop"},
            "item": [{"name": "Ping", "request": "https://shop.test/ping"}]}"#;
        let target = ImportTarget {
            can_pick_folders: true,
            ..ImportTarget::default()
        };
        for (text, file, offered) in [
            (collection, "shop.json", true),
            ("GET https://shop.test/ping\n", "shop.http", true),
            (SPEC, "openapi.json", false),
        ] {
            let import =
                tortuga_core::import::parse(text, file, ImportKind::Postman).expect("parse");
            let dialog = ImportDialog::new(import, vec![PathBuf::from("/repo").join(file)]);
            let mut actions = Vec::new();
            {
                let mut h = crate::test_host::harness(|ui| {
                    actions.push(dialog.show(ui.ctx(), target).1);
                });
                h.run();
                let button = h.query_by_label("Add environment files…");
                assert_eq!(button.is_some(), offered, "{file}");
                if let Some(button) = button {
                    button.click();
                    h.run();
                }
            }
            assert_eq!(
                actions.contains(&Some(ImportAction::AddEnvironments)),
                offered
            );
        }
    }

    /// R-COL-20: the dialog for a project says what is added, what is updated and
    /// which secret values were found, without showing them.
    #[test]
    fn environment_import_dialog_summarises() {
        let env = tortuga_core::postman::parse_environment(
            r#"{"name": "Staging", "values": [{"key": "token", "value": "s3cret", "type": "secret"}]}"#,
        )
        .expect("parse");
        let existing = tortuga_core::Environment {
            id: "default".into(),
            name: "Default".into(),
            ..tortuga_core::Environment::default()
        };
        let default = tortuga_core::postman::parse_environment(
            r#"{"name": "Default", "values": [{"key": "limit", "value": "5"}]}"#,
        )
        .expect("parse");
        let preview = tortuga_core::import::import_environments(
            &tortuga_core::Manifest::new("Shop"),
            &[existing],
            &[env, default],
        );
        let dialog = EnvironmentImportDialog {
            kind: ImportKind::Postman,
            root: PathBuf::from("/repo/shop"),
            project: "Shop".into(),
            sources: vec!["staging.json".into(), "default.json".into()],
            preview,
        };
        let mut results = Vec::new();
        {
            let mut h = crate::test_host::harness(|ui| results.push(dialog.show(ui.ctx())));
            h.run();
            h.get_by_label("Import Postman environments");
            h.get_by_label("Staging");
            h.get_by_label("Default");
            h.get_by_label("token");
            h.get_by_label("Environments that are updated keep what the files don't set.");
            assert!(h.query_by_label_contains("s3cret").is_none());
            h.get_by_label("Import").click();
            h.run();
        }
        assert!(results.contains(&DialogResult::Done(())));
    }

    /// R-COL-17: a `.http` file is named after the file and gets its own title.
    #[test]
    fn import_dialog_for_an_http_file() {
        let text = "@baseUrl = http://localhost:3000\n\n### One\nGET {{baseUrl}}/a\n\n### Two\nGET {{baseUrl}}/b\n";
        let import =
            tortuga_core::import::parse(text, "pokemon.http", ImportKind::Http).expect("parse");
        let dialog = ImportDialog::new(import, vec![PathBuf::from("/repo/pokemon.http")]);
        assert_eq!(dialog.dir, Path::new("/repo/pokemon"));
        let mut h = crate::test_host::harness(|ui| {
            let _ = dialog.show(ui.ctx(), ImportTarget::default());
        });
        h.run();
        h.get_by_label("Import HTTP file");
        h.get_by_label("Pokemon");
        h.get_by_label("2 requests");
        h.get_by_label("None");
    }

    #[test]
    fn secrets_hint_says_where_values_come_from() {
        let mut import = tortuga_core::openapi::parse(SPEC).expect("parse");
        assert!(secrets_hint(&import).is_some_and(|h| h.contains("fill them in")));
        import.secret_values.insert("token".into(), "abc".into());
        assert!(secrets_hint(&import).is_some_and(|h| h.contains("were found")));
        import.manifest.secrets.push("other".into());
        assert!(secrets_hint(&import).is_some_and(|h| h.contains("Fill in the other 1")));
        import.manifest.secrets.clear();
        assert_eq!(secrets_hint(&import), None);
    }

    #[test]
    fn long_paths_keep_their_end() {
        assert_eq!(path_end("/a/b", 10), "/a/b");
        assert_eq!(
            path_end("/very/long/path/repo/pet-store", 15),
            "…repo/pet-store"
        );
    }

    /// R-COL-29: several `.http` files say they become a folder each, and the dialog
    /// lists the environment files added.
    #[test]
    fn import_dialog_for_several_http_files() {
        let import = tortuga_core::http_file::parse_files(
            &[
                ("GET https://a.test/1\n", "a.http"),
                ("GET https://a.test/2\n", "b.http"),
            ],
            "Shop",
        )
        .expect("parse");
        let mut dialog = ImportDialog::new(
            import,
            vec![PathBuf::from("/repo/a.http"), PathBuf::from("/repo/b.http")],
        );
        dialog.environment_files = vec!["http-client.env.json".into()];
        let mut h = crate::test_host::harness(|ui| {
            let _ = dialog.show(ui.ctx(), ImportTarget::default());
        });
        h.run();
        h.get_by_label(
            "From a.http, b.http. Tortuga creates a new project from them, with a folder per \
             file; the files themselves are not changed.",
        );
        h.get_by_label("2 requests in 2 folders");
        h.get_by_label("Environment files");
        h.get_by_label("http-client.env.json");
    }

    /// R-COL-29: the dialog for HTTP environments has its own title and lists what was
    /// not imported as written.
    #[test]
    fn http_environment_import_dialog_shows_warnings() {
        let envs = tortuga_core::http_file::parse_environments(
            r#"{"dev": {"home": "{{$dotenv HOME}}"}}"#,
            "http-client.env.json",
        )
        .expect("parse");
        let preview = tortuga_core::import::import_environments(
            &tortuga_core::Manifest::new("Shop"),
            &[],
            &envs,
        );
        let dialog = EnvironmentImportDialog {
            kind: ImportKind::Http,
            root: PathBuf::from("/repo/shop"),
            project: "Shop".into(),
            sources: vec!["http-client.env.json".into()],
            preview,
        };
        let mut h = crate::test_host::harness(|ui| {
            let _ = dialog.show(ui.ctx());
        });
        h.run();
        h.get_by_label("Import HTTP environments");
        h.get_by_label("dev");
        h.get_by_label("Not imported as written (1)".to_uppercase().as_str());
    }
}
