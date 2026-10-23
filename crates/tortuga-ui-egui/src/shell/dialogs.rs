//! The open dialog, and saving what it returns.

use tortuga_core::Request;

use super::{Modal, TortugaUi};
use crate::components::{
    AddProjects, DialogResult, EnvironmentsDialog, SettingsDialog, WorkspaceVarsAction,
};
use crate::state::{AppState, ItemKey};

impl TortugaUi {
    pub(super) fn show_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.modal.take() else {
            return;
        };
        let keep = match &mut modal {
            Modal::Settings(d) => self.show_settings(ctx, d),
            Modal::Environments(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(saved) => !self.save_environments(d, saved),
                result => result == DialogResult::Open,
            },
            Modal::Defaults(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(defaults) => !self.save_defaults(&d.target.clone(), defaults),
                result => result == DialogResult::Open,
            },
            Modal::ConfirmDelete(key, d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    if self
                        .state
                        .run(&key.root, |c, s| c.delete(s, &key.path))
                        .is_some()
                    {
                        self.forget(&key.root, &key.path);
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::MoveTo(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(dest) => {
                    let key = d.item.clone();
                    if let Some(new_path) = self
                        .state
                        .run(&key.root, |c, s| c.move_to(s, &key.path, &dest))
                    {
                        self.remap(&key.root, &key.path, &new_path);
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::SaveToCollection(d) => match d.show(ctx, &self.state) {
                DialogResult::Done((folder, name)) => {
                    let request = Request {
                        name,
                        ..self.current.clone()
                    };
                    if let Some(path) = self.state.run(&folder.root, |c, s| {
                        c.new_request(s, &folder.path, &request)
                    }) {
                        // The scratch request starts over; the saved copy takes its tab.
                        self.current = Request::default();
                        self.tabs.close(&None);
                        self.open_tab(Some(ItemKey::new(&folder.root, path)), true);
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::CopyCurl(d) => match d.show(ctx) {
                DialogResult::Done(text) => {
                    ctx.copy_text(text);
                    self.copied_at = Some(ctx.input(|i| i.time));
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::Palette(palette) => self.show_palette(ctx, palette),
            Modal::Shortcuts(d) => d.show(ctx) == DialogResult::Open,
            Modal::Changes(d) => d.show(ctx) == DialogResult::Open,
            Modal::Fanout => self.show_fanout(ctx),
            Modal::Protected(guarded, d) => self.show_protected(ctx, guarded, d),
            Modal::NewWorkflow(..)
            | Modal::RenameWorkflow(..)
            | Modal::DeleteWorkflow(..)
            | Modal::RuntimeVars(_) => self.show_workflow_modal(ctx, &mut modal),
            Modal::Cookies(d) => self.show_cookies_dialog(ctx, d),
            Modal::Mock(d) => self.show_mock_dialog(ctx, d),
            Modal::Import(open) => self.show_import_dialog(ctx, open),
            Modal::EnvironmentImport(open) => self.show_environment_import(ctx, open),
            Modal::ClearHistory(target, d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    self.clear_history(target);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::CreateProject(_)
            | Modal::AddProjects(_)
            | Modal::NewWorkspace(_)
            | Modal::RenameWorkspace(_)
            | Modal::RemoveWorkspace(_)
            | Modal::WorkspaceVars(_) => self.show_workspace_modal(ctx, &mut modal),
        };
        // A dialog may have opened another one ("Create a project there…"); never overwrite it.
        if keep && self.modal.is_none() {
            self.modal = Some(modal);
        }
    }

    /// Dialogs about projects and workspaces (R-WS-1, 3, 5, 7). Returns whether it stays open.
    pub(super) fn show_workspace_modal(&mut self, ctx: &egui::Context, modal: &mut Modal) -> bool {
        match modal {
            Modal::CreateProject(d) => match d.show(ctx) {
                DialogResult::Done((name, init_git)) => {
                    if init_git && let Err(e) = self.state.host.init_git(&d.dir) {
                        self.state.notify(e);
                    }
                    if let Err(e) = self.state.create_collection(&d.dir, &name) {
                        self.state.notify(e.to_string());
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::AddProjects(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(AddProjects::Add(paths)) => {
                    for path in paths {
                        if let Err(e) = self.state.open_collection(&path) {
                            self.state.notify(e.to_string());
                        }
                    }
                    false
                }
                DialogResult::Done(AddProjects::CreateHere(dir)) => {
                    self.create_project_in(dir);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::NewWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    self.switch_workspace(|state| state.new_workspace(&name));
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RenameWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    self.state.rename_workspace(&name);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RemoveWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    self.switch_workspace(AppState::delete_workspace);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::WorkspaceVars(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(WorkspaceVarsAction::ClearCaptured) => {
                    self.state.clear_workspace_runtime();
                    true
                }
                DialogResult::Done(WorkspaceVarsAction::Save(vars, secrets)) => {
                    match self.state.save_workspace_variables(vars, &secrets) {
                        Ok(()) => false,
                        Err(e) => {
                            d.error = Some(e);
                            true
                        }
                    }
                }
                result => result == DialogResult::Open,
            },
            _ => false,
        }
    }

    /// The settings dialog, with "Add…" for CA certificates (R-HTTP-16) and "Choose…"
    /// for the code font (R-SET-10). Returns whether it stays open.
    pub(super) fn show_settings(&mut self, ctx: &egui::Context, d: &mut SettingsDialog) -> bool {
        let result = d.show(ctx, self.state.host.can_pick_folders());
        if std::mem::take(&mut d.add_ca_requested)
            && let Some(path) = self
                .state
                .host
                .pick_any_file("Choose a CA certificate (PEM)")
        {
            d.add_ca(path);
        }
        if std::mem::take(&mut d.choose_font_requested)
            && let Some(path) = self
                .state
                .host
                .pick_any_file("Choose a font for code (.ttf, .otf)")
        {
            d.set_code_font(path);
        }
        match result {
            DialogResult::Done(settings) => match self.new_code_font_problem(&settings) {
                Some(e) => {
                    d.error = Some(e);
                    true
                }
                None => self.save_settings(&settings, d),
            },
            result => result == DialogResult::Open,
        }
    }

    /// Asks before something goes to a protected environment, and does it on a yes
    /// (R-SRV-9). Returns whether the question stays open.
    fn show_protected(
        &mut self,
        ctx: &egui::Context,
        guarded: &super::Guarded,
        d: &crate::components::ConfirmDialog,
    ) -> bool {
        match d.show(ctx) {
            DialogResult::Done(()) => {
                self.go_ahead(ctx, guarded.clone());
                false
            }
            result => result == DialogResult::Open,
        }
    }

    /// Does what waited for a yes before going to a protected environment (R-SRV-9).
    fn go_ahead(&mut self, ctx: &egui::Context, guarded: super::Guarded) {
        match guarded {
            super::Guarded::Send => self.send_now(ctx),
            super::Guarded::SendEverywhere => self.start_fanout(ctx),
            super::Guarded::RunWorkflow(key) => self.start_workflow(&key, ctx),
        }
    }

    /// The answers of every environment (R-SRV-10); closing the dialog drops them and
    /// stops what is still on its way. Returns whether it stays open.
    fn show_fanout(&mut self, ctx: &egui::Context) -> bool {
        let open = self.fanout.as_ref().is_some_and(|f| {
            crate::components::fanout_dialog(ctx, &f.title, &f.rows) == DialogResult::Open
        });
        if !open {
            self.fanout = None;
        }
        open
    }

    /// Opens where the variable `name` of request `key` gets its value (a Ctrl+click on
    /// a `{{name}}`, R-VAR-9); an undefined one, where it can be defined.
    pub(super) fn open_variable(&mut self, key: Option<&ItemKey>, name: &str) {
        use tortuga_core::vars::Source;
        let root = key.map(|k| k.root.clone());
        match (self.state.variable_source(key, name), root) {
            (Some(Source::Runtime | Source::WorkspaceRuntime), root) => {
                self.show_runtime_vars(root);
            }
            (Some(Source::Workspace), _) => {
                self.modal = Some(Modal::WorkspaceVars(
                    crate::components::WorkspaceVarsDialog::new(&self.state),
                ));
            }
            (Some(Source::Dynamic), _) => self.state.inform(format!(
                "{{{{{name}}}}} gets a new value every time it is sent."
            )),
            (Some(Source::WorkspaceEnvironment(env)), _) => {
                self.modal = Some(Modal::Environments(
                    EnvironmentsDialog::for_workspace(&self.state).on_environment(&env),
                ));
            }
            (Some(Source::Folder { path, .. }), Some(root)) => {
                self.modal = Some(Modal::Defaults(
                    crate::components::DefaultsDialog::for_folder(&self.state, root, path)
                        .on_variables(),
                ));
            }
            (_, Some(root)) => {
                self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                    &self.state,
                    root,
                )));
            }
            (_, None) => self.state.inform(
                "The scratch request has no project: save it in one to define its variables.",
            ),
        }
    }

    /// Why a newly chosen code font can't be used, if it can't (R-SET-10).
    fn new_code_font_problem(&self, settings: &tortuga_core::AppSettings) -> Option<String> {
        let font = settings.appearance.code_font.as_deref()?;
        if self.state.settings.appearance.code_font.as_deref() == Some(font) {
            return None;
        }
        super::appearance::read_font(self.state.host.as_ref(), font)
            .err()
            .map(|e| format!("The code font can't be used: {e}"))
    }

    /// Applies and saves the settings. Returns whether the dialog stays open.
    fn save_settings(
        &mut self,
        settings: &tortuga_core::AppSettings,
        d: &mut SettingsDialog,
    ) -> bool {
        match self.state.apply_settings(settings) {
            Ok(()) => {
                self.trim_loaded_history();
                false
            }
            Err(e) => {
                d.error = Some(e);
                true
            }
        }
    }

    /// Saves the defaults dialog: workspace (R-SET-1, R-SET-2), project (R-COL-4,
    /// R-SET-1) or folder (R-COL-11) defaults. Returns success.
    pub(super) fn save_defaults(
        &mut self,
        target: &crate::components::DefaultsTarget,
        d: crate::components::EditedDefaults,
    ) -> bool {
        use crate::components::DefaultsTarget;
        match target {
            DefaultsTarget::Workspace => {
                self.state
                    .save_workspace_defaults(tortuga_core::workspace::WorkspaceDefaults {
                        settings: d.settings,
                        headers: d.headers,
                        auth: d.auth.unwrap_or_default(),
                    });
                true
            }
            DefaultsTarget::Folder(root, folder) => {
                let defaults = tortuga_core::format::FolderDefaults {
                    description: d.description,
                    headers: d.headers,
                    auth: d.auth,
                    variables: d.variables,
                };
                self.state
                    .run(root, |c, s| c.save_folder_defaults(s, folder, &defaults))
                    .is_some()
            }
            DefaultsTarget::Project(root) => self
                .state
                .run(root, |c, s| {
                    let manifest = tortuga_core::Manifest {
                        description: d.description.clone(),
                        headers: d.headers.clone(),
                        auth: d.auth.clone(),
                        settings: d.settings,
                        ..c.manifest.clone()
                    };
                    c.save_config(s, &manifest, &c.environments)
                })
                .is_some(),
        }
    }

    /// Saves the environments dialog (R-WS-9). Returns success; on failure the dialog
    /// stays open with the reason.
    pub(super) fn save_environments(
        &mut self,
        d: &mut EnvironmentsDialog,
        saved: crate::components::SavedEnvironments,
    ) -> bool {
        match self
            .state
            .save_environments(saved.list, &saved.secrets, saved.projects)
        {
            Ok(()) => true,
            Err(e) => {
                d.error = Some(e);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-SET-10: a newly chosen code font that isn't a font keeps the dialog open with
    /// the reason; one already in use isn't read again.
    #[test]
    fn a_code_font_that_is_not_a_font_is_refused() {
        let dir = std::env::temp_dir().join(format!("tortuga-font-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let page = dir.join("not a font ü😀.ttf");
        std::fs::write(&page, "<html>").expect("file");
        let ui = TortugaUi::new(Box::new(crate::test_host::TestHost::default()));
        let mut settings = ui.state.settings.clone();
        assert_eq!(ui.new_code_font_problem(&settings), None, "bundled font");
        settings.appearance.code_font = Some(page.clone());
        let problem = ui.new_code_font_problem(&settings).expect("refused");
        assert!(
            problem.contains("not a TrueType or OpenType font"),
            "{problem}"
        );

        let mut ui = ui;
        ui.state
            .update_settings(|s| s.appearance.code_font = Some(page.clone()));
        assert_eq!(ui.new_code_font_problem(&settings), None, "unchanged");
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// The settings dialog fits the shown tab: after a tall tab, a short one leaves Save
    /// right under its content (the button row used to keep the tallest height).
    #[test]
    fn the_settings_dialog_shrinks_to_a_short_tab() {
        use egui_kittest::kittest::Queryable;

        let mut ui = TortugaUi::new(Box::new(crate::test_host::TestHost::default()));
        let mut settings = ui.state.settings.clone();
        settings.client.proxy = tortuga_core::ProxyMode::Manual;
        ui.modal = Some(super::Modal::Settings(SettingsDialog::new(&settings)));
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run_steps(6);
        h.get_by_label("Editing").click();
        h.run_steps(6);
        let last = h.get_by_label("Save automatically").rect().bottom();
        let save = h.get_by_label("Save").rect().top();
        assert!(
            save - last < 120.0,
            "Save at {save}, content ends at {last}"
        );
    }

    /// The title bar's ⚙ opens Settings; it is named "Settings" for screen readers.
    #[test]
    fn the_settings_icon_opens_settings() {
        use egui_kittest::kittest::Queryable;

        let mut ui = TortugaUi::new(Box::new(crate::test_host::TestHost::default()));
        {
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 800.0))
                .build_ui(|egui_ui| ui.ui(egui_ui));
            h.run();
            assert!(
                h.query_by_label("⚙  Settings").is_none(),
                "no text any more"
            );
            h.get_by_label("Settings").click();
            h.run();
        }
        assert!(matches!(ui.modal, Some(super::Modal::Settings(_))));
    }
}
