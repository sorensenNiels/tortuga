//! The import dialog for OpenAPI descriptions (R-COL-15), Postman collections
//! (R-COL-16, with environment files, R-COL-20) and `.http` files (R-COL-17): what the
//! file becomes, where the new project goes, and what could not be imported as written.
//! Also the dialog for Postman environments imported into a project (R-COL-20).

use std::path::{Path, PathBuf};

use egui::{Context, RichText, Ui};
use tortuga_core::NewItem;
use tortuga_core::import::{Import, ImportKind};
use tortuga_core::postman::EnvironmentImport;

use super::{DialogResult, dialog};
use crate::format::count;
use crate::theme;

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
    /// Postman environment files for the new project (R-COL-20).
    AddEnvironments,
}

/// Reviews an import before the project is written.
#[derive(Debug, Clone)]
pub struct ImportDialog {
    pub import: Import,
    /// The description file it came from.
    pub source: PathBuf,
    /// Where the project will be created.
    pub dir: PathBuf,
}

impl ImportDialog {
    /// Proposes a folder named after the API next to the description file.
    pub fn new(import: Import, source: PathBuf) -> Self {
        let name = tortuga_core::slug::slugify(&import.manifest.name);
        let dir = source
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(if name.is_empty() { "api".into() } else { name });
        Self {
            import,
            source,
            dir,
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
        };
        let result = dialog(ctx, "import", title, 560.0, true, |ui| {
            let file = self
                .source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            theme::hint(
                ui,
                format!(
                    "From {file} ({}). Tortuga creates a new project from it; the file itself is \
                     not changed.",
                    self.import.kind.label()
                ),
            );
            ui.add_space(8.0);
            self.summary(ui);
            if self.import.kind == ImportKind::Postman
                && target.can_pick_folders
                && ui
                    .button("Add environment files…")
                    .on_hover_text(
                        "Postman environments exported as JSON become this project's \
                         environments",
                    )
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

/// Reviews Postman environments before they are imported into a project (R-COL-20).
#[derive(Debug, Clone)]
pub struct EnvironmentImportDialog {
    pub root: PathBuf,
    pub project: String,
    /// The files' names.
    pub sources: Vec<String>,
    pub preview: EnvironmentImport,
}

impl EnvironmentImportDialog {
    /// Done means "import".
    pub fn show(&self, ctx: &Context) -> DialogResult<()> {
        dialog(
            ctx,
            "import-environments",
            "Import Postman environments",
            480.0,
            true,
            |ui| {
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
            },
        )
    }
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
        let dialog = ImportDialog::new(import, PathBuf::from("/repo/openapi.yaml"));
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
        let dialog = ImportDialog::new(import, PathBuf::from("/repo/shop.json"));
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

    /// R-COL-20: a Postman collection import offers environment files; other imports
    /// don't.
    #[test]
    fn import_dialog_adds_postman_environments() {
        let collection = r#"{"info": {"name": "Shop"},
            "item": [{"name": "Ping", "request": "https://shop.test/ping"}]}"#;
        let target = ImportTarget {
            can_pick_folders: true,
            ..ImportTarget::default()
        };
        for (text, file, offered) in [
            (collection, "shop.json", true),
            (SPEC, "openapi.json", false),
        ] {
            let import =
                tortuga_core::import::parse(text, file, ImportKind::Postman).expect("parse");
            let dialog = ImportDialog::new(import, PathBuf::from("/repo").join(file));
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
        let preview = tortuga_core::postman::import_environments(
            &tortuga_core::Manifest::new("Shop"),
            &[existing],
            &[env, default],
        );
        let dialog = EnvironmentImportDialog {
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
        let dialog = ImportDialog::new(import, PathBuf::from("/repo/pokemon.http"));
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
}
