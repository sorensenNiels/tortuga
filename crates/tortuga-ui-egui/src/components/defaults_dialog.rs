use std::path::PathBuf;

use egui::{Context, Ui};
use tortuga_core::format::FolderDefaults;
use tortuga_core::{Auth, Item, KeyValue};

use super::auth_editor::{auth_fields, kind_picker};
use super::{DialogResult, dialog, key_value_table};
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Headers,
    Auth,
    Variables,
}

/// Headers and auth every request in a project inherits (R-COL-4, R-HTTP-5), or the
/// headers, auth and variables of a folder (R-COL-11).
#[derive(Debug, Clone)]
pub struct DefaultsDialog {
    pub root: PathBuf,
    /// The folder whose defaults these are; `None`: the project's.
    pub folder: Option<PathBuf>,
    headers: Vec<KeyValue>,
    auth: Auth,
    variables: Vec<KeyValue>,
    tab: Tab,
}

impl DefaultsDialog {
    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let (headers, auth) = state
            .collection(&root)
            .map(|c| (c.manifest.headers.clone(), c.manifest.auth.clone()))
            .unwrap_or_default();
        Self {
            root,
            folder: None,
            headers,
            auth,
            variables: Vec::new(),
            tab: Tab::Headers,
        }
    }

    /// The defaults of the folder at `folder` in the project at `root` (R-COL-11).
    pub fn for_folder(state: &AppState, root: PathBuf, folder: PathBuf) -> Self {
        let defaults = match state.collection(&root).and_then(|c| c.find(&folder)) {
            Some(Item::Folder(f)) => f.defaults.clone(),
            _ => FolderDefaults::default(),
        };
        Self {
            root,
            folder: Some(folder),
            headers: defaults.headers,
            auth: defaults.auth,
            variables: defaults
                .variables
                .into_iter()
                .map(|(name, value)| KeyValue::new(name, value))
                .collect(),
            tab: Tab::Headers,
        }
    }

    /// Opens on the Auth tab (from a request's "Edit … defaults…").
    #[must_use]
    pub fn on_auth(mut self) -> Self {
        self.tab = Tab::Auth;
        self
    }

    /// Done with the new defaults (`variables` stays empty for a project: its variables
    /// are edited with its environments).
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<FolderDefaults> {
        let Some(c) = state.collection(&self.root) else {
            return DialogResult::Close;
        };
        let folder_name = match self.folder.as_ref().map(|f| c.find(f)) {
            None => None,
            Some(Some(item)) => Some(item.name()),
            Some(None) => return DialogResult::Close,
        };
        let title = match &folder_name {
            Some(name) => format!("Folder defaults — {name}"),
            None => format!("Project defaults — {}", c.name()),
        };
        let read_only = c.read_only;
        let undefined = undefined_names(state, &self.root, &self.headers, &self.auth);
        dialog(ctx, "collection-defaults", &title, 640.0, false, |ui| {
            theme::hint(
                ui,
                if folder_name.is_some() {
                    "Every request in this folder and its subfolders inherits these, before the \
                     project's defaults. A request (or a subfolder) can replace a header by using \
                     the same name, and can choose its own auth."
                } else {
                    "Every request in this project inherits these. A request can replace a header by \
                     adding one with the same name (or switch it off), and can choose its own auth."
                },
            );
            ui.add_space(8.0);
            self.tabs(ui, folder_name.is_some());
            let file = if folder_name.is_some() {
                "folder file (_folder.toml)"
            } else {
                "project file (tortuga.toml)"
            };
            match self.tab {
                Tab::Headers => {
                    key_value_table(
                        ui,
                        "default-headers",
                        &mut self.headers,
                        "Header",
                        &undefined,
                    );
                }
                Tab::Auth => auth_tab(ui, &mut self.auth, &undefined, file, folder_name.is_some()),
                Tab::Variables => {
                    key_value_table(
                        ui,
                        "default-variables",
                        &mut self.variables,
                        "Variable",
                        &[],
                    );
                    theme::hint(
                        ui,
                        "Defaults for {{variables}} in this folder's requests. A value from the \
                         active environment, or a secret value, wins over these.",
                    );
                }
            }
            ui.add_space(16.0);
            let (cancel, save) = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui
                        .add_enabled(!read_only, theme::primary_button(ui, "Save"))
                        .clicked();
                    (ui.button("Cancel").clicked(), save)
                })
                .inner;
            if save {
                DialogResult::Done(FolderDefaults {
                    headers: self.headers.clone(),
                    auth: self.auth.clone(),
                    variables: self
                        .variables
                        .iter()
                        .filter(|v| v.is_active())
                        .map(|v| (v.name.trim().to_owned(), v.value.clone()))
                        .collect(),
                })
            } else if cancel {
                DialogResult::Close
            } else {
                DialogResult::Open
            }
        })
    }

    fn tabs(&mut self, ui: &mut Ui, folder: bool) {
        let header_count = self.headers.iter().filter(|h| h.is_active()).count();
        let variable_count = self.variables.iter().filter(|v| v.is_active()).count();
        let count = |n: usize| (n > 0).then(|| n.to_string());
        let mut tabs = vec![
            (Tab::Headers, "Headers", count(header_count)),
            (
                Tab::Auth,
                "Auth",
                (!self.auth.is_none()).then(|| "⏺".to_owned()),
            ),
        ];
        if folder {
            tabs.push((Tab::Variables, "Variables", count(variable_count)));
        }
        theme::tabs(ui, &mut self.tab, &tabs);
    }
}

fn auth_tab(ui: &mut Ui, auth: &mut Auth, undefined: &[String], file: &str, folder: bool) {
    // A folder without auth passes on what it inherits (R-COL-11).
    kind_picker(ui, auth, if folder { "Inherit" } else { "No auth" });
    ui.add_space(6.0);
    if folder && auth.is_none() {
        theme::hint(
            ui,
            "Requests in this folder use the auth of a parent folder, or of the project.",
        );
    } else {
        auth_fields(ui, "default-auth", auth, undefined, file);
    }
}

/// Variables used in the defaults that the active environment does not define (R-VAR-5).
fn undefined_names(
    state: &AppState,
    root: &std::path::Path,
    headers: &[KeyValue],
    auth: &Auth,
) -> Vec<String> {
    let ctx = state.context(Some(root));
    let mut texts: Vec<&str> = auth.fields();
    for h in headers.iter().filter(|h| h.enabled) {
        texts.extend([h.name.as_str(), h.value.as_str()]);
    }
    let mut names: Vec<String> = texts
        .into_iter()
        .flat_map(tortuga_core::vars::names)
        .filter(|n| !ctx.is_defined(n))
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::Collection;
    use tortuga_core::storage::FsStorage;

    use super::*;
    use crate::test_host::TestHost;

    #[test]
    fn typing_a_default_header_and_saving() {
        let root = std::env::temp_dir().join(format!("tortuga-defaults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut dialog = DefaultsDialog::new(&state, root.clone());
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("header name field");
            field.focus();
            field.type_text("X-Team");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(saved.headers, [KeyValue::new("X-Team", "")]);
        assert_eq!(saved.auth, Auth::None);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn choosing_bearer_and_typing_a_token() {
        let root =
            std::env::temp_dir().join(format!("tortuga-defaults-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut dialog = DefaultsDialog::new(&state, root.clone()).on_auth();
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Bearer token").click();
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("token field");
            field.click();
            h.run();
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("token field")
                .type_text("{{token}}");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(
            saved.auth,
            Auth::Bearer {
                token: "{{token}}".into()
            }
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-11: a folder's dialog also has variables.
    #[test]
    fn folder_defaults_have_variables() {
        let root =
            std::env::temp_dir().join(format!("tortuga-defaults-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let col = Collection::create(&FsStorage, &root, "C").expect("create");
        let folder = col
            .new_folder(&FsStorage, std::path::Path::new(""), "Admin")
            .expect("folder");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut dialog = DefaultsDialog::for_folder(&state, root.clone(), folder);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Folder defaults — Admin");
            h.get_by_label("Variables").click();
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("variable name field");
            field.focus();
            field.type_text("tenant");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(
            saved.variables,
            [("tenant".to_owned(), String::new())].into()
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
