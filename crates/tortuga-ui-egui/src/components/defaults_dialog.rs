use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{ComboBox, Context, TextEdit, Ui};
use tortuga_core::format::FolderDefaults;
use tortuga_core::settings::{Feature, Inherited, Level, RequestSettings, ResolvedSettings};
use tortuga_core::{Auth, Item, KeyValue};

use super::auth_editor::{auth_fields, kind_picker};
use super::request_editor::parse_timeout;
use super::{DialogResult, VarHelp, description_editor, dialog, key_value_table};
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Headers,
    Auth,
    Variables,
    Requests,
    /// A project's or folder's description (R-COL-22).
    Description,
}

/// Whose defaults the dialog edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultsTarget {
    /// The project at this root (R-COL-4, R-SET-1).
    Project(PathBuf),
    /// A folder (path in the project) of the project at the root (R-COL-11).
    Folder(PathBuf, PathBuf),
    /// The active workspace (R-SET-1, R-SET-2).
    Workspace,
}

/// What the dialog saves. `variables` is only used for folders, `settings` only for
/// projects and the workspace, `description` for projects and folders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditedDefaults {
    pub description: String,
    pub headers: Vec<KeyValue>,
    /// `None` = inherit; the workspace has nothing to inherit, so `None` = no auth there.
    pub auth: Option<Auth>,
    pub variables: BTreeMap<String, String>,
    pub settings: RequestSettings,
}

/// Headers and auth every request in a project inherits (R-COL-4, R-HTTP-5) and how
/// they are sent (R-SET-1); the headers, auth and variables of a folder (R-COL-11); or
/// the headers, auth and settings every project of the workspace inherits (R-SET-2).
#[derive(Debug, Clone)]
pub struct DefaultsDialog {
    pub target: DefaultsTarget,
    headers: Vec<KeyValue>,
    auth: Option<Auth>,
    variables: Vec<KeyValue>,
    settings: RequestSettings,
    timeout_text: String,
    description: String,
    editing_description: bool,
    tab: Tab,
}

impl DefaultsDialog {
    fn with(target: DefaultsTarget, headers: Vec<KeyValue>, auth: Option<Auth>) -> Self {
        Self {
            target,
            headers,
            auth,
            variables: Vec::new(),
            settings: RequestSettings::default(),
            timeout_text: String::new(),
            description: String::new(),
            editing_description: false,
            tab: Tab::Headers,
        }
    }

    #[must_use]
    fn with_settings(mut self, settings: RequestSettings) -> Self {
        self.timeout_text = settings
            .timeout_ms
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        self.settings = settings;
        self
    }

    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let (headers, auth, settings, description) = state
            .collection(&root)
            .map(|c| {
                (
                    c.manifest.headers.clone(),
                    c.manifest.auth.clone(),
                    c.manifest.settings,
                    c.manifest.description.clone(),
                )
            })
            .unwrap_or_default();
        let mut d =
            Self::with(DefaultsTarget::Project(root), headers, auth).with_settings(settings);
        d.description = description;
        d
    }

    /// The defaults of the folder at `folder` in the project at `root` (R-COL-11).
    pub fn for_folder(state: &AppState, root: PathBuf, folder: PathBuf) -> Self {
        let defaults = match state.collection(&root).and_then(|c| c.find(&folder)) {
            Some(Item::Folder(f)) => f.defaults.clone(),
            _ => FolderDefaults::default(),
        };
        let mut d = Self::with(
            DefaultsTarget::Folder(root, folder),
            defaults.headers,
            defaults.auth,
        );
        d.variables = defaults
            .variables
            .into_iter()
            .map(|(name, value)| KeyValue::new(name, value))
            .collect();
        d.description = defaults.description;
        d
    }

    /// The active workspace's defaults (R-SET-1, R-SET-2).
    pub fn for_workspace(state: &AppState) -> Self {
        let d = &state.workspace().defaults;
        let auth = Some(d.auth.clone()).filter(|a| !a.is_none());
        Self::with(DefaultsTarget::Workspace, d.headers.clone(), auth).with_settings(d.settings)
    }

    /// Opens on the Auth tab (from a request's "Edit … defaults…").
    #[must_use]
    pub fn on_auth(mut self) -> Self {
        self.tab = Tab::Auth;
        self
    }

    /// Opens on the Variables tab (a folder's, from a Ctrl+click on a variable, R-VAR-9).
    #[must_use]
    pub fn on_variables(mut self) -> Self {
        self.tab = Tab::Variables;
        self
    }

    /// Opens on the Description tab (from "Description…" in a menu, R-COL-22).
    #[must_use]
    pub fn on_description(mut self) -> Self {
        self.tab = Tab::Description;
        self
    }

    /// Done with the new defaults.
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<EditedDefaults> {
        let Some((title, read_only, root, inherited)) = self.heading(state) else {
            return DialogResult::Close;
        };
        let ws = state.workspace();
        let folder = matches!(self.target, DefaultsTarget::Folder(..));
        let workspace = self.target == DefaultsTarget::Workspace;
        // Each project defines its own variables, so a workspace's can't be checked.
        let vars = VarHelp {
            undefined: if workspace {
                Vec::new()
            } else {
                undefined_names(state, root.as_deref(), &self.headers, self.auth.as_ref())
            },
            ..VarHelp::default()
        };
        dialog(ctx, "collection-defaults", &title, 640.0, false, |ui| {
            theme::hint(ui, explanation(&self.target));
            ui.add_space(8.0);
            // Descriptions that are off still show one that was written (R-SET-13).
            let description = !workspace
                && (state.settings.features.enabled(Feature::Docs)
                    || !self.description.trim().is_empty());
            self.tabs(ui, folder, inherited.is_some(), description);
            let file = match self.target {
                DefaultsTarget::Folder(..) => "folder file (_folder.toml)",
                DefaultsTarget::Project(_) => "project file (tortuga.toml)",
                DefaultsTarget::Workspace if ws.file.is_some() => "workspace file",
                DefaultsTarget::Workspace => "workspace (on this machine)",
            };
            match self.tab {
                Tab::Headers => {
                    key_value_table(ui, "default-headers", &mut self.headers, "Header", &vars);
                }
                Tab::Auth => auth_tab(ui, &mut self.auth, &vars, file, folder, workspace),
                Tab::Variables => {
                    key_value_table(
                        ui,
                        "default-variables",
                        &mut self.variables,
                        "Variable",
                        &VarHelp::default(),
                    );
                    theme::hint(
                        ui,
                        "Defaults for {{variables}} in this folder's requests. A value from the \
                         active environment, or a secret value, wins over these.",
                    );
                }
                Tab::Description => {
                    description_editor(
                        ui,
                        "defaults",
                        &mut self.description,
                        &mut self.editing_description,
                        read_only,
                        380.0,
                        None,
                    );
                }
                Tab::Requests => {
                    if let Some(inherited) = &inherited {
                        requests_tab(
                            ui,
                            &mut self.settings,
                            &mut self.timeout_text,
                            inherited,
                            workspace,
                        );
                    }
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
                DialogResult::Done(EditedDefaults {
                    description: self.description.clone(),
                    headers: self.headers.clone(),
                    auth: self.auth.clone(),
                    variables: self
                        .variables
                        .iter()
                        .filter(|v| v.is_active())
                        .map(|v| (v.name.trim().to_owned(), v.value.clone()))
                        .collect(),
                    settings: self.settings,
                })
            } else if cancel {
                DialogResult::Close
            } else {
                DialogResult::Open
            }
        })
    }

    /// The title, whether it is read-only, the project's root (for undefined variables)
    /// and the request settings a project or the workspace inherits; `None` when the
    /// project or folder is gone.
    fn heading(
        &self,
        state: &AppState,
    ) -> Option<(String, bool, Option<PathBuf>, Option<ResolvedSettings>)> {
        let ws = state.workspace();
        let app = &state.settings.client;
        Some(match &self.target {
            DefaultsTarget::Workspace => (
                format!("Workspace defaults — {}", ws.name),
                false,
                None,
                Some(ResolvedSettings::resolve(&[], app)),
            ),
            DefaultsTarget::Project(root) => {
                let c = state.collection(root)?;
                (
                    format!("Project defaults — {}", c.name()),
                    c.read_only,
                    Some(root.clone()),
                    Some(ResolvedSettings::resolve(
                        &[(Level::Workspace, ws.defaults.settings)],
                        app,
                    )),
                )
            }
            DefaultsTarget::Folder(root, folder) => {
                let c = state.collection(root)?;
                let item = c.find(folder)?;
                (
                    format!("Folder defaults — {}", item.name()),
                    c.read_only,
                    Some(root.clone()),
                    None,
                )
            }
        })
    }

    fn tabs(&mut self, ui: &mut Ui, folder: bool, settings: bool, description: bool) {
        let header_count = self.headers.iter().filter(|h| h.is_active()).count();
        let variable_count = self.variables.iter().filter(|v| v.is_active()).count();
        let count = |n: usize| (n > 0).then(|| n.to_string());
        let mut tabs = vec![
            (Tab::Headers, "Headers", count(header_count)),
            (
                Tab::Auth,
                "Auth",
                self.auth.is_some().then(|| "⏺".to_owned()),
            ),
        ];
        if folder {
            tabs.push((Tab::Variables, "Variables", count(variable_count)));
        }
        if settings {
            let overrides = [
                self.settings.timeout_ms.is_some(),
                self.settings.follow_redirects.is_some(),
                self.settings.cookies.is_some(),
            ]
            .into_iter()
            .filter(|set| *set)
            .count();
            tabs.push((Tab::Requests, "Requests", count(overrides)));
        }
        if description {
            let written = !self.description.trim().is_empty();
            tabs.push((
                Tab::Description,
                "Description",
                written.then(|| "⏺".to_owned()),
            ));
        }
        theme::tabs(ui, &mut self.tab, &tabs);
    }
}

/// What the defaults of `target` are for.
fn explanation(target: &DefaultsTarget) -> &'static str {
    match target {
        DefaultsTarget::Folder(..) => {
            "Every request in this folder and its subfolders inherits these, before the \
             project's defaults. A request (or a subfolder) can replace a header by using the \
             same name, and can choose its own auth."
        }
        DefaultsTarget::Project(_) => {
            "Every request in this project inherits these, over the workspace's. A request can \
             replace a header by adding one with the same name (or switch it off), and can \
             choose its own auth."
        }
        DefaultsTarget::Workspace => {
            "Every project in this workspace inherits these, below its own: a project, folder or \
             request header with the same name replaces one of these, and their own auth wins. \
             The scratch request doesn't use them."
        }
    }
}

/// How the project's or workspace's requests are sent (R-SET-1): each setting is
/// inherited until it is set here.
fn requests_tab(
    ui: &mut Ui,
    settings: &mut RequestSettings,
    timeout_text: &mut String,
    inherited: &ResolvedSettings,
    workspace: bool,
) {
    egui::Grid::new("request-settings")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            let label = ui.label("Timeout (ms)");
            let t = inherited.timeout_ms;
            let edit = TextEdit::singleline(timeout_text)
                .id_salt("settings-timeout")
                .hint_text(format!("{} (from {})", t.value, t.from.label()))
                .desired_width(200.0);
            if ui.add(edit).labelled_by(label.id).changed() {
                settings.timeout_ms = parse_timeout(timeout_text);
            }
            ui.end_row();
            let label = ui.label("Follow redirects");
            inherit_picker(
                ui,
                (label.id, "settings-redirects"),
                &mut settings.follow_redirects,
                inherited.follow_redirects,
            );
            ui.end_row();
            let label = ui.label("Send and store cookies");
            inherit_picker(
                ui,
                (label.id, "settings-cookies"),
                &mut settings.cookies,
                inherited.cookies,
            );
            ui.end_row();
        });
    ui.add_space(6.0);
    theme::hint(
        ui,
        if workspace {
            "Empty or “Inherit” uses the app's settings. Projects can set their own, and a \
             request can still set its own timeout and turn cookies off."
        } else {
            "Empty or “Inherit” uses the workspace's value, else the app's settings. A request \
             can still set its own timeout and turn cookies off."
        },
    );
}

/// "Inherit: on (from …)", "On" or "Off". `id` is the row's label and the picker's id.
fn inherit_picker(
    ui: &mut Ui,
    id: (egui::Id, &str),
    value: &mut Option<bool>,
    inherited: Inherited<bool>,
) {
    let on_off = |b: bool| if b { "on" } else { "off" };
    let inherit = format!(
        "Inherit: {} (from {})",
        on_off(inherited.value),
        inherited.from.label()
    );
    let shown = match value {
        None => inherit.clone(),
        Some(true) => "On".to_owned(),
        Some(false) => "Off".to_owned(),
    };
    ComboBox::from_id_salt(id.1)
        .selected_text(shown)
        .width(260.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, inherit);
            ui.selectable_value(value, Some(true), "On");
            ui.selectable_value(value, Some(false), "Off");
        })
        .response
        .labelled_by(id.0);
}

fn auth_tab(
    ui: &mut Ui,
    auth: &mut Option<Auth>,
    vars: &VarHelp,
    file: &str,
    folder: bool,
    workspace: bool,
) {
    // A folder or project can inherit, or say "no auth" to stop what it would inherit
    // (R-HTTP-5, R-COL-11, R-SET-2). The workspace is the top: nothing to inherit.
    kind_picker(ui, auth, (!workspace).then_some("Inherit"));
    if workspace && *auth == Some(Auth::None) {
        *auth = None;
    }
    ui.add_space(6.0);
    let hint = match (auth.as_mut(), folder, workspace) {
        (None, true, _) => {
            "Requests in this folder use the auth of a parent folder, or of the project."
        }
        (None, false, false) => "Requests in this project use the workspace's auth, if it has one.",
        (None, false, true) => "Requests send no authentication unless their project sets some.",
        (Some(Auth::None), true, _) => {
            "Requests in this folder send no authentication, whatever a parent folder or the \
             project sets. A request can still choose its own."
        }
        (Some(Auth::None), false, _) => {
            "Requests in this project send no authentication, even when the workspace sets \
             some. A folder or request can still choose its own."
        }
        (Some(a), ..) => {
            auth_fields(ui, "default-auth", a, vars, file);
            return;
        }
    };
    theme::hint(ui, hint);
}

/// Variables used in the defaults that the active environment does not define (R-VAR-5).
fn undefined_names(
    state: &AppState,
    root: Option<&std::path::Path>,
    headers: &[KeyValue],
    auth: Option<&Auth>,
) -> Vec<String> {
    let ctx = state.context(root);
    let mut texts: Vec<&str> = auth.map(Auth::fields).unwrap_or_default();
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
        assert_eq!(saved.auth, None);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-22: a project's and a folder's description are edited on their Description
    /// tab, which the workspace's defaults don't have.
    #[test]
    fn writing_a_description() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().to_path_buf();
        let col = Collection::create(&FsStorage, &root, "C").expect("create");
        col.new_folder(&FsStorage, std::path::Path::new(""), "Admin")
            .expect("folder");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let project = DefaultsDialog::new(&state, root.clone()).on_description();
        let folder =
            DefaultsDialog::for_folder(&state, root.clone(), "admin".into()).on_description();
        for mut dialog in [project, folder] {
            let mut result = None;
            {
                let mut h = crate::test_host::harness(|ui| {
                    if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                        result = Some(r);
                    }
                });
                h.run();
                h.get_by_label_contains("No description yet");
                h.get_by_label("Edit").click();
                h.run();
                let field = h.get_by_role(egui::accesskit::Role::MultilineTextInput);
                field.focus();
                field.type_text("# Admin ü😀");
                h.run();
                h.get_by_label("Save").click();
                h.run();
            }
            assert_eq!(result.expect("saved").description, "# Admin ü😀");
        }

        let mut workspace = DefaultsDialog::for_workspace(&state);
        let mut h = crate::test_host::harness(|ui| {
            let _ = workspace.show(ui.ctx(), &state);
        });
        h.run();
        assert!(h.query_by_label("Description").is_none());
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
            Some(Auth::Bearer {
                token: "{{token}}".into()
            })
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-1: the Requests tab shows inherited values and saves overrides only.
    /// R-HTTP-5, R-SET-2: a project can choose "No auth" to stop the workspace's auth;
    /// the workspace, with nothing above it, has no "Inherit".
    #[test]
    fn project_auth_can_be_none_and_the_workspace_has_no_inherit() {
        let root =
            std::env::temp_dir().join(format!("tortuga-defaults-none-{}", std::process::id()));
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
            h.get_by_label("Inherit");
            h.get_by_label("No auth").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        assert_eq!(result.expect("saved").auth, Some(Auth::None));

        let mut workspace = DefaultsDialog::for_workspace(&state).on_auth();
        let mut h = crate::test_host::harness(|ui| {
            workspace.show(ui.ctx(), &state);
        });
        h.run();
        h.get_by_label("No auth");
        assert!(h.query_by_label("Inherit").is_none());
        drop(h);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn project_request_settings() {
        let root =
            std::env::temp_dir().join(format!("tortuga-defaults-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let mut state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        state.save_workspace_defaults(tortuga_core::workspace::WorkspaceDefaults {
            settings: RequestSettings {
                timeout_ms: Some(9_000),
                ..RequestSettings::default()
            },
            ..Default::default()
        });
        let mut dialog = DefaultsDialog::new(&state, root.clone());
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Requests").click();
            h.run();
            h.get_by_label("Timeout (ms)");
            // Redirects, then cookies: both inherit from the app.
            h.get_all_by_role(egui::accesskit::Role::ComboBox)
                .next()
                .expect("redirects picker")
                .click();
            h.run();
            h.get_by_label("Off").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(
            saved.settings,
            RequestSettings {
                follow_redirects: Some(false),
                ..RequestSettings::default()
            },
            "the timeout is still inherited"
        );
        assert_eq!(
            dialog.timeout_text, "",
            "the timeout field is empty with the workspace's value as hint"
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
