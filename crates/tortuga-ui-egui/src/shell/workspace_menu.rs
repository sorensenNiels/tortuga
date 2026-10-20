//! The workspace menu and the environment picker in the top bar (R-WS-1..8, R-VAR-2).

use egui::{ComboBox, RichText, Ui};

use super::{Modal, TortugaUi, WorkspaceAction};
use crate::components::{ConfirmDialog, DefaultsDialog, EnvironmentsDialog, NameDialog};
use crate::state::AppState;
use crate::theme;

impl TortugaUi {
    /// The workspace switcher and what can be done with workspaces (R-WS-1, R-WS-5).
    pub(super) fn workspace_menu(&self, ui: &mut Ui) -> Option<WorkspaceAction> {
        let local = &self.state.local;
        let current = self.state.workspace();
        let button =
            egui::Button::new((theme::semibold(&current.name), "  ⏷")).frame_when_inactive(false);
        let (response, inner) =
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                let mut action = None;
                theme::section_label(ui, "Workspaces");
                for (i, ws) in local.workspaces.iter().enumerate() {
                    let mut button = ui.add(egui::Button::selectable(
                        i == local.active_workspace,
                        &ws.name,
                    ));
                    if let Some(file) = &ws.file {
                        button = button.on_hover_text(format!("Saved in {}", file.display()));
                    }
                    if button.clicked() {
                        action = Some(WorkspaceAction::Switch(i));
                    }
                }
                ui.separator();
                if ui.button("New workspace…").clicked() {
                    action = Some(WorkspaceAction::New);
                }
                if ui.button("Rename…").clicked() {
                    action = Some(WorkspaceAction::Rename);
                }
                if ui
                    .button("Variables…")
                    .on_hover_text("Variables every project in this workspace can use")
                    .clicked()
                {
                    action = Some(WorkspaceAction::Variables);
                }
                if ui
                    .button("Defaults…")
                    .on_hover_text(
                        "Headers, auth, timeout, redirects and cookies every project in this \
                         workspace inherits",
                    )
                    .clicked()
                {
                    action = Some(WorkspaceAction::Defaults);
                }
                if self.state.host.can_save_files() {
                    let hover = match &current.file {
                        Some(file) => format!(
                            "Kept in sync with {}. Choose another file to move it.",
                            file.display()
                        ),
                        None => {
                            "Write the workspace to a file you can commit, so teammates can open it"
                                .into()
                        }
                    };
                    if ui.button("Save as file…").on_hover_text(hover).clicked() {
                        action = Some(WorkspaceAction::SaveAsFile);
                    }
                    if ui
                        .button("Open workspace file…")
                        .on_hover_text("Open a workspace a teammate saved as a file")
                        .clicked()
                    {
                        action = Some(WorkspaceAction::OpenFile);
                    }
                }
                let danger = theme::palette(ui).fail;
                if ui
                    .add_enabled(
                        local.workspaces.len() > 1,
                        egui::Button::new(RichText::new("Remove workspace…").color(danger)),
                    )
                    .on_disabled_hover_text("There is always at least one workspace")
                    .clicked()
                {
                    action = Some(WorkspaceAction::Remove);
                }
                action
            });
        response.on_hover_text("Switch workspace");
        inner.and_then(|i| i.inner)
    }

    pub(super) fn on_workspace(&mut self, action: WorkspaceAction) {
        match action {
            WorkspaceAction::Switch(index) => self.switch_workspace(|state| {
                state.switch_workspace(index);
            }),
            WorkspaceAction::New => {
                self.modal = Some(Modal::NewWorkspace(NameDialog::new(
                    "New workspace",
                    "Create",
                    "",
                )));
            }
            WorkspaceAction::Rename => {
                let name = self.state.workspace().name.clone();
                self.modal = Some(Modal::RenameWorkspace(NameDialog::new(
                    "Rename workspace",
                    "Rename",
                    &name,
                )));
            }
            WorkspaceAction::Variables => {
                self.modal = Some(Modal::WorkspaceVars(
                    crate::components::WorkspaceVarsDialog::new(&self.state),
                ));
            }
            WorkspaceAction::Defaults => {
                self.modal = Some(Modal::Defaults(DefaultsDialog::for_workspace(&self.state)));
            }
            WorkspaceAction::SaveAsFile => {
                let name = tortuga_core::workspace::file_name(&self.state.workspace().name);
                if let Some(file) = self.state.host.pick_save_path("Save workspace as", &name)
                    && let Err(e) = self.state.save_workspace_as(&file)
                {
                    self.state.notify(e);
                }
            }
            WorkspaceAction::OpenFile => {
                if let Some(file) = self.state.host.pick_file(
                    "Open a workspace file",
                    tortuga_core::workspace::WORKSPACE_FILE_SUFFIX,
                ) {
                    let mut result = Ok(());
                    self.switch_workspace(|state| result = state.open_workspace_file(&file));
                    if let Err(e) = result {
                        self.state.notify(e);
                    }
                }
            }
            WorkspaceAction::Remove => {
                let name = self.state.workspace().name.clone();
                self.modal = Some(Modal::RemoveWorkspace(ConfirmDialog {
                    title: "Remove workspace?".into(),
                    message: format!(
                        "“{name}” is removed from this machine. Its projects and files stay where they are."
                    ),
                    confirm_label: "Remove".into(),
                }));
            }
        }
    }

    /// Runs `change` (which may switch workspace), keeping unsaved edits of the old
    /// workspace for this session, then selects the new workspace's remembered request.
    pub(super) fn switch_workspace(&mut self, change: impl FnOnce(&mut AppState)) {
        self.keep_if_edited();
        self.state.remember_tabs(self.tabs.to_saved());
        self.select_and_remember(None, false);
        change(&mut self.state);
        self.check_restored_drafts();
        self.restore_tabs();
        let remembered = self.state.remembered_selection();
        self.tabs.open(&remembered, false);
        self.select_and_remember(remembered, false);
    }

    /// One environment for the whole workspace (R-WS-6, R-SRV-5): every project with an
    /// environment of that name uses it.
    pub(super) fn environment_picker(&mut self, ui: &mut Ui) {
        let names = self.state.environment_names();
        // The project whose environments ✏ edits: the selected request's, else the first.
        let root = self
            .selection
            .as_ref()
            .map(|k| k.root.clone())
            .or_else(|| self.state.collections.first().map(|c| c.root.clone()));
        let Some(root) = root else {
            return;
        };
        let p = theme::palette(ui);
        ui.label(RichText::new("Environment").color(p.muted));
        let edit = if names.is_empty() {
            ui.button("Add environments…").clicked()
        } else {
            // A name no project has any more (renamed or removed) is not shown.
            let chosen = self.state.workspace().environment.clone();
            let shown = chosen.filter(|n| names.contains(n)).or_else(|| {
                let c = self.state.collection(&root)?;
                let id = self.state.active_environment(&root)?;
                c.environment(&id).map(|e| e.name.clone())
            });
            let mut picked = None;
            ComboBox::from_id_salt("environment")
                .selected_text((
                    RichText::new("⏺").size(9.0).color(p.ok),
                    " ",
                    shown.clone().unwrap_or_default(),
                ))
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui
                            .selectable_label(shown.as_ref() == Some(name), name)
                            .clicked()
                        {
                            picked = Some(name.clone());
                        }
                    }
                })
                .response
                .on_hover_text("Switches every project in the workspace that has an environment with this name");
            if let Some(name) = picked {
                self.state.set_workspace_environment(&name);
            }
            ui.add(theme::ghost_button("✏"))
                .on_hover_text("Edit environments, servers and variables of this project")
                .clicked()
        };
        if edit {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                root,
            )));
        }
    }
}
