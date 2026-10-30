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
            SidebarAction::ClearScratch => self.clear_scratch(),
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
            SidebarAction::Sort(key) => {
                self.state.run(&key.root, |c, s| c.sort(s, &key.path));
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
            | SidebarAction::RunRequests(..)
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

    /// R-VSC-3: opens the project that holds `path` (a project folder or a file in one),
    /// adding it to the workspace, and a request file in a tab. Used when the app is
    /// started with a path, e.g. from the VS Code extension.
    pub fn open_path(&mut self, path: &Path) {
        let manifest = tortuga_core::collection::MANIFEST_FILE;
        let Some(root) = path
            .ancestors()
            .find(|dir| self.state.storage().exists(&dir.join(manifest)))
            .map(Path::to_path_buf)
        else {
            self.state
                .notify(t!("not-a-project", path = path.display().to_string()));
            return;
        };
        if let Err(e) = self.state.open_collection(&root) {
            self.state.notify(e.to_string());
            return;
        }
        let Ok(relative) = path.strip_prefix(&root) else {
            return;
        };
        let is_request = self
            .state
            .collection(&root)
            .is_some_and(|c| c.request(relative).is_some());
        if is_request {
            self.open_tab(Some(ItemKey::new(&root, relative)), true);
        }
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tortuga_core::storage::FsStorage;
    use tortuga_core::{Collection, HttpMethod, Request};

    use crate::TortugaUi;
    use crate::state::ItemKey;
    use crate::test_host::TestHost;

    /// R-COL-48: the 🗙 on the scratch row empties the scratch request and its response,
    /// shown or not, and is only there when there is something to clear.
    #[test]
    fn the_scratch_rows_cross_clears_the_scratch_request() {
        use egui_kittest::kittest::Queryable;

        for shown in [true, false] {
            let (mut ui, root, key) = super::super::tests::setup(&format!("scratch-x-{shown}"));
            ui.select(None);
            ui.current.url = "https://example.test/ü😀".into();
            ui.current.body = tortuga_core::RequestBody::Json("x".repeat(10_000));
            ui.sent.insert(
                None,
                super::super::Sent {
                    filter: "$.id".into(),
                    ..Default::default()
                },
            );
            if !shown {
                ui.select(Some(key.clone()));
            }
            {
                let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
                h.run();
                // The sidebar's is the leftmost; tabs have their own 🗙.
                let before = h.query_all_by_label("🗙").count();
                h.query_all_by_label("🗙")
                    .min_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
                    .expect("a 🗙 on the scratch row")
                    .click();
                h.run();
                assert_eq!(
                    h.query_all_by_label("🗙").count(),
                    before - 1,
                    "nothing left to clear"
                );
            }
            assert_eq!(*ui.scratch_request(), Request::default());
            // The shown request gets an empty entry again on the next frame.
            assert!(ui.sent.get(&None).is_none_or(|s| s.filter.is_empty()));
            if !shown {
                assert_eq!(ui.selection, Some(key), "the shown request stays");
                assert_ne!(ui.current, Request::default());
            }
            std::fs::remove_dir_all(root).expect("clean up");
        }
    }

    /// R-VSC-3: starting the app with a request file adds its project and opens it.
    #[test]
    fn a_request_file_opens_its_project_and_the_request() {
        let root = std::env::temp_dir().join(format!("tortuga-open-ü😀-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let collection = Collection::create(&FsStorage, &root, "Shop").expect("create");
        let folder = collection
            .new_folder(&FsStorage, Path::new(""), "Users")
            .expect("folder");
        let mut request = Request::new(HttpMethod::Get, "https://a.test/users");
        request.name = "List users".into();
        let path = collection
            .new_request(&FsStorage, &folder, &request)
            .expect("request");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));

        ui.open_path(&root.join(&path));

        assert!(ui.state.collection(&root).is_some());
        assert_eq!(ui.selection, Some(ItemKey::new(&root, path)));
        // A folder (or the project itself) only opens the project.
        ui.open_path(&root.join(&folder));
        assert_eq!(ui.state.collections.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_outside_any_project_says_so() {
        let dir = std::env::temp_dir().join(format!("tortuga-open-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));

        ui.open_path(&dir);

        let notice = ui.state.notice.clone().unwrap_or_default();
        assert!(notice.contains("not a Tortuga project"), "{notice}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
