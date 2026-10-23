//! What the sidebar asks for: opening, renaming, moving and deleting requests, folders and
//! projects (R-COL-2, R-COL-3, R-COL-5, R-WS-5).

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use tortuga_core::{Item, Request};

use super::{Modal, TortugaUi};
use crate::components::{
    AddProjectsDialog, ConfirmDialog, CopyToProjectDialog, CreateProjectDialog, DefaultsDialog,
    EnvironmentsDialog, MoveDialog, SidebarAction,
};
use crate::state::ItemKey;

impl TortugaUi {
    pub(super) fn on_sidebar(&mut self, action: SidebarAction, ctx: &egui::Context) {
        match action {
            SidebarAction::Select(key) => self.select(key),
            SidebarAction::Keep(key) => self.open_tab(key, true),
            SidebarAction::NewRequest { root, parent } => {
                let request = Request {
                    name: t!("new-request-name"),
                    ..Request::default()
                };
                if let Some(path) = self
                    .state
                    .run(&root, |c, s| c.new_request(s, &parent, &request))
                {
                    self.open_tab(Some(ItemKey::new(&root, path)), true);
                }
            }
            SidebarAction::NewFolder { root, parent } => {
                if let Some(path) = self.state.run(&root, |c, s| {
                    c.new_folder(s, &parent, &t!("new-folder-name"))
                }) {
                    let key = ItemKey::new(&root, path);
                    let name = self
                        .state
                        .collection(&root)
                        .and_then(|c| c.find(&key.path).map(Item::name))
                        .unwrap_or_default();
                    self.sidebar.start_rename(key, name);
                }
            }
            SidebarAction::Rename { key, name } => self.rename(&key, name),
            SidebarAction::RenameCollection { root, name } => self.rename_project(&root, &name),
            SidebarAction::Duplicate(key) => {
                if let Some(path) = self.state.run(&key.root, |c, s| c.duplicate(s, &key.path))
                    && path.extension().is_some_and(|e| e == "toml")
                {
                    self.open_tab(Some(ItemKey::new(&key.root, path)), true);
                }
            }
            SidebarAction::MoveBy(key, direction) => {
                self.state
                    .run(&key.root, |c, s| c.move_by(s, &key.path, direction));
            }
            SidebarAction::MoveTo(key) => self.modal = Some(Modal::MoveTo(MoveDialog::new(key))),
            SidebarAction::CopyTo(key) => {
                self.modal = Some(Modal::CopyTo(CopyToProjectDialog::new(&self.state, key)));
            }
            SidebarAction::Delete(key) => {
                let dialog = self.confirm_delete(&key);
                self.modal = Some(Modal::ConfirmDelete(key, dialog));
            }
            SidebarAction::ShowError(message) => self.state.notify(message),
            action @ (SidebarAction::EditEnvironments(_)
            | SidebarAction::EditDefaults(_)
            | SidebarAction::EditFolderDefaults(_)
            | SidebarAction::ProjectDescription(_)
            | SidebarAction::FolderDescription(_)) => self.open_project_dialog(action),
            SidebarAction::Place {
                key,
                parent,
                before,
            } => self.place(&key, &parent, before.as_deref()),
            action @ (SidebarAction::NewWorkflow(_)
            | SidebarAction::SelectWorkflow(_)
            | SidebarAction::RunWorkflow(_)
            | SidebarAction::RenameWorkflow(_)
            | SidebarAction::DeleteWorkflow(_)
            | SidebarAction::RuntimeVariables(_)) => self.on_workflow_sidebar(action, ctx),
            SidebarAction::Reload(root) => self.on_disk_change(&root),
            SidebarAction::OpenInFileManager(root) => {
                self.notify_failure(self.state.host.open_in_file_manager(&root));
            }
            SidebarAction::OpenInTerminal(root) => {
                self.notify_failure(self.state.host.open_in_terminal(&root));
            }
            SidebarAction::ShowCookies(root) => self.show_cookies(root),
            action @ (SidebarAction::StartMock(_)
            | SidebarAction::StopMock(_)
            | SidebarAction::ShowMock(_)) => self.on_mock_sidebar(action),
            SidebarAction::Close(root) => {
                self.forget(&root, Path::new(""));
                self.state.close_collection(&root);
            }
            SidebarAction::NewProject => self.new_project(),
            SidebarAction::AddProjects => self.add_projects(),
            SidebarAction::Import(kind) => self.import_file(kind),
            SidebarAction::ImportEnvironments(root, kind) => self.import_environments(root, kind),
            SidebarAction::LocateMissing(path) => self.locate_missing(&path),
            SidebarAction::RemoveMissing(path) => self.state.remove_missing(&path),
        }
    }

    /// The environments and defaults dialogs of a project or folder, and their
    /// Description tab (R-COL-22).
    fn open_project_dialog(&mut self, action: SidebarAction) {
        self.modal = Some(match action {
            SidebarAction::EditEnvironments(root) => {
                Modal::Environments(EnvironmentsDialog::new(&self.state, root))
            }
            SidebarAction::EditDefaults(root) => {
                Modal::Defaults(DefaultsDialog::new(&self.state, root))
            }
            SidebarAction::ProjectDescription(root) => {
                Modal::Defaults(DefaultsDialog::new(&self.state, root).on_description())
            }
            SidebarAction::EditFolderDefaults(key) => {
                Modal::Defaults(DefaultsDialog::for_folder(&self.state, key.root, key.path))
            }
            SidebarAction::FolderDescription(key) => Modal::Defaults(
                DefaultsDialog::for_folder(&self.state, key.root, key.path).on_description(),
            ),
            _ => return,
        });
    }

    /// Copies a request or folder into a folder of a project (R-COL-38) and opens a
    /// copied request, so it shows where it went.
    pub(super) fn copy_to_project(&mut self, item: &ItemKey, to: &ItemKey) {
        // The target is re-read after the copy, so the source can't stay borrowed.
        let Some(source) = self.state.collection(&item.root).cloned() else {
            return;
        };
        if let Some(path) = self.state.run(&to.root, |c, s| {
            c.copy_from(s, &source, &item.path, &to.path)
        }) && path.extension().is_some_and(|e| e == "toml")
        {
            self.open_tab(Some(ItemKey::new(&to.root, path)), true);
        }
    }

    /// Shows what went wrong, if anything did.
    fn notify_failure(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.state.notify(e);
        }
    }

    pub(super) fn rename_project(&mut self, root: &Path, name: &str) {
        self.state.run(root, |c, s| {
            let manifest = tortuga_core::Manifest {
                name: name.to_owned(),
                ..c.manifest.clone()
            };
            c.save_config(s, &manifest, &c.environments)
        });
    }

    /// A dropped tree row (R-COL-12): edits and selection follow it if it moved folder.
    pub(super) fn place(&mut self, key: &ItemKey, parent: &Path, before: Option<&Path>) {
        if let Some(new_path) = self
            .state
            .run(&key.root, |c, s| c.place(s, &key.path, parent, before))
        {
            self.remap(&key.root, &key.path, &new_path);
        }
    }

    pub(super) fn rename(&mut self, key: &ItemKey, name: String) {
        let is_folder = self
            .state
            .collection(&key.root)
            .is_some_and(|c| matches!(c.find(&key.path), Some(Item::Folder(_))));
        let new_path = if is_folder {
            self.state
                .run(&key.root, |c, s| c.rename_folder(s, &key.path, &name))
        } else {
            let Some(saved) = self.state.saved_request(key) else {
                return;
            };
            let renamed = Request {
                name: name.clone(),
                ..saved
            };
            self.state
                .run(&key.root, |c, s| c.save_request(s, &key.path, &renamed))
        };
        let Some(new_path) = new_path else {
            return;
        };
        if !is_folder {
            if let Some(d) = self.drafts.get_mut(key) {
                d.name.clone_from(&name);
            }
            if self.selection.as_ref() == Some(key) {
                self.current.name = name;
            }
        }
        self.remap(&key.root, &key.path, &new_path);
    }

    pub(super) fn confirm_delete(&self, key: &ItemKey) -> ConfirmDialog {
        let item = self
            .state
            .collection(&key.root)
            .and_then(|c| c.find(&key.path));
        let name = item.map(Item::name).unwrap_or_default();
        let message = if matches!(item, Some(Item::Folder(_))) {
            t!("delete-folder-message", name = name)
        } else {
            t!("delete-message", name = name)
        };
        ConfirmDialog {
            title: t!("delete-title"),
            message,
            confirm_label: t!("delete"),
        }
    }

    /// R-WS-7: pick (or make) a folder, then name the project there.
    pub(super) fn new_project(&mut self) {
        let Some(dir) = self.state.host.pick_folder(&t!("new-project-pick-folder")) else {
            return;
        };
        self.create_project_in(dir);
    }

    pub(super) fn create_project_in(&mut self, dir: PathBuf) {
        let manifest = dir.join(tortuga_core::collection::MANIFEST_FILE);
        if self.state.storage().exists(&manifest) {
            // Already a project: just add it.
            if let Err(e) = self.state.open_collection(&dir) {
                self.state.notify(e.to_string());
            }
            return;
        }
        let repository = self.state.host.repository_of(&dir);
        let can_init = self.state.host.can_init_git();
        self.modal = Some(Modal::CreateProject(CreateProjectDialog::new(
            dir, repository, can_init,
        )));
    }

    /// R-WS-3: pick a folder or repository and list the projects in it.
    pub(super) fn add_projects(&mut self) {
        let Some(dir) = self.state.host.pick_folder(&t!("add-projects-pick-folder")) else {
            return;
        };
        let (tx, rx) = channel();
        let wake = self.state.waker();
        self.state.host.find_projects(
            &dir,
            Box::new(move |result| {
                let _ = tx.send(result);
                wake();
            }),
        );
        self.modal = Some(Modal::AddProjects(AddProjectsDialog::new(dir, rx)));
    }

    /// R-WS-5: the user points at the folder where a missing project is now.
    pub(super) fn locate_missing(&mut self, old: &Path) {
        let name = old
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(dir) = self
            .state
            .host
            .pick_folder(&t!("locate-project", name = name.as_str()))
        else {
            return;
        };
        if !self
            .state
            .storage()
            .exists(&dir.join(tortuga_core::collection::MANIFEST_FILE))
        {
            self.state
                .notify(t!("not-a-project", path = dir.display().to_string()));
            return;
        }
        if let Err(e) = self.state.locate_missing(old, &dir) {
            self.state.notify(e.to_string());
        }
    }
}
