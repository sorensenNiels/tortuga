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
            Modal::NewWorkflow(..)
            | Modal::RenameWorkflow(..)
            | Modal::DeleteWorkflow(..)
            | Modal::RuntimeVars(_) => self.show_workflow_modal(ctx, &mut modal),
            Modal::Cookies(d) => self.show_cookies_dialog(ctx, d),
            Modal::Import(open) => self.show_import_dialog(ctx, open),
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

    /// Saves the environments dialog. Secrets first: if the credential store fails,
    /// nothing changes on disk and the dialog stays open with the error. Returns success.
    /// The settings dialog, with "Add…" for CA certificates (R-HTTP-16). Returns whether
    /// it stays open.
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
        match result {
            DialogResult::Done(settings) => match self.state.apply_settings(&settings) {
                Ok(()) => {
                    self.trim_loaded_history();
                    false
                }
                Err(e) => {
                    d.error = Some(e);
                    true
                }
            },
            result => result == DialogResult::Open,
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
                        auth: d.auth,
                    });
                true
            }
            DefaultsTarget::Folder(root, folder) => {
                let defaults = tortuga_core::format::FolderDefaults {
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

    pub(super) fn save_environments(
        &mut self,
        d: &mut EnvironmentsDialog,
        saved: crate::components::SavedConfig,
    ) -> bool {
        if let Err(e) = self.state.save_secrets(&d.root, saved.secrets) {
            d.error = Some(e);
            return false;
        }
        self.state
            .run(&d.root, |c, s| {
                c.save_config(s, &saved.manifest, &saved.environments)
            })
            .is_some()
    }
}
