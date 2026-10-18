//! What the UI needs from the platform it runs on.
//!
//! The UI itself never touches the file system, native dialogs or an async runtime
//! directly; the launcher (`tortuga-app`) provides them through [`Host`], so the same
//! UI runs on desktop and (later) in the browser.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tortuga_core::git::GitStatus;
use tortuga_core::history;
use tortuga_core::secrets::SecretStore;
use tortuga_core::storage::Storage;
use tortuga_core::workspace::ProjectRef;
use tortuga_core::{ClientSettings, Request};

/// What is remembered on this machine only, never in a project (R-LOC-1): the
/// workspaces (R-WS-1) and which one is active.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalState {
    pub workspaces: Vec<Workspace>,
    /// Index into `workspaces`.
    pub active_workspace: usize,
    /// Before workspaces (M6.5): the open collections. Read once, moved into the first workspace.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open_collections: Vec<PathBuf>,
    /// Before workspaces (M6.5): collection root → environment id. Moved like `open_collections`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub active_environments: BTreeMap<String, String>,
}

impl LocalState {
    /// Name of the workspace that always exists (R-WS-1).
    pub const DEFAULT_WORKSPACE: &str = "My workspace";

    /// Makes sure there is a workspace and that the active index is valid. State saved
    /// before workspaces existed becomes the first workspace.
    pub fn migrate(&mut self) {
        let projects: Vec<ProjectRef> = std::mem::take(&mut self.open_collections)
            .into_iter()
            .map(ProjectRef::new)
            .collect();
        let environments = std::mem::take(&mut self.active_environments);
        if self.workspaces.is_empty() {
            self.workspaces.push(Workspace {
                name: Self::DEFAULT_WORKSPACE.into(),
                projects,
                project_environments: environments,
                ..Workspace::default()
            });
        }
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = 0;
        }
    }
}

/// A named set of projects that are open together (R-WS-1).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub name: String,
    /// The shareable workspace file this workspace is saved in, if any (R-WS-5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// Environment chosen for the whole workspace, by name (R-WS-6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// The request that was selected: (project root, path in the project).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<(PathBuf, PathBuf)>,
    /// The open request tabs (R-COL-13, R-COL-14).
    #[serde(skip_serializing_if = "SavedTabs::is_empty")]
    pub tabs: SavedTabs,
    /// Projects in sidebar order, including ones that are missing on this machine.
    pub projects: Vec<ProjectRef>,
    /// Project root (as text) → environment id, for projects without an environment
    /// named like the workspace's (R-WS-6).
    pub project_environments: BTreeMap<String, String>,
}

/// The open request tabs of a workspace (R-COL-13), in strip order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedTabs {
    pub open: Vec<SavedTab>,
    /// Roots of the projects whose tab group is collapsed (R-COL-14).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub collapsed: Vec<PathBuf>,
}

impl SavedTabs {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.collapsed.is_empty()
    }
}

/// One open tab: a request (project root, path in the project), or the scratch request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedTab {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The preview tab, which the next opened request replaces.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub preview: bool,
}

/// Unsaved request edits, kept across restarts (R-COL-8). Local data only (§6.7): it
/// may hold credentials typed into a request, so the app keeps it private to the user.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedDrafts {
    /// The scratch request, if it is not empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scratch: Option<Request>,
    pub drafts: Vec<SavedDraft>,
}

/// One request with unsaved edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedDraft {
    /// Project root.
    pub root: PathBuf,
    /// Path of the request file in the project.
    pub path: PathBuf,
    /// The edited request.
    pub request: Request,
    /// The saved version the edits started from (`None`: the file was gone), so a
    /// change on disk while the app was closed is noticed (R-GIT-11).
    #[serde(default)]
    pub base: Option<Request>,
    /// A conflict the user had not answered yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<DraftConflict>,
}

/// What happened on disk to a request with unsaved edits (R-GIT-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DraftConflict {
    Changed,
    Deleted,
}

/// Background work started by the UI (sending a request).
#[cfg(not(target_arch = "wasm32"))]
pub type BoxTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
/// Background work started by the UI (sending a request). Browser futures are not `Send`.
#[cfg(target_arch = "wasm32")]
pub type BoxTask = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// Platform services. Optional capabilities have defaults that report "not available".
pub trait Host {
    /// Where collections are read and written.
    fn storage(&self) -> &dyn Storage;

    /// Where secret variable values are kept on this machine (R-VAR-3, R-LOC-2).
    fn secrets(&self) -> &dyn SecretStore;

    /// Set when secrets can't go to the OS credential store, e.g. "kept in a file instead" (R-LOC-2).
    fn secrets_warning(&self) -> Option<&str> {
        None
    }

    /// The saved cookie jar (JSON) of the collection at `root`, `None` if there is none (R-CK-3).
    fn load_cookies(&self, _root: &Path) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Saves the cookie jar of the collection at `root`, outside the collection (R-CK-3, R-LOC-1).
    fn save_cookies(&self, _root: &Path, _json: &str) -> Result<(), String> {
        Ok(())
    }

    /// Starts watching the collection at `root`; `changed` is called (from any thread)
    /// after its files, or its git state, changed (R-GIT-10). Without file watching
    /// this does nothing and changes appear after "Reload from disk".
    fn watch(&self, _root: &Path, _changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        Ok(())
    }

    fn unwatch(&self, _root: &Path) {}

    /// Reads the git status of the collection at `root` in the background and passes it
    /// to `done` (from any thread); `Ok(None)` when it is not in a git repository
    /// (R-GIT-12, R-GIT-14). Without git support, `done` gets `Ok(None)` right away.
    fn git_status(
        &self,
        _root: &Path,
        done: Box<dyn FnOnce(Result<Option<GitStatus>, String>) + Send>,
    ) {
        done(Ok(None));
    }

    /// The top folder of the git repository `dir` is in, if any (R-WS-7).
    fn repository_of(&self, _dir: &Path) -> Option<PathBuf> {
        None
    }

    /// Whether [`Host::init_git`] works.
    fn can_init_git(&self) -> bool {
        false
    }

    /// Makes `dir` a new git repository (R-WS-7).
    fn init_git(&self, _dir: &Path) -> Result<(), String> {
        Err("Not available here.".into())
    }

    /// Looks for projects (folders with `tortuga.toml`) in `dir` and below, skipping
    /// git-ignored folders, in the background; passes them to `done` from any thread (R-WS-3).
    fn find_projects(
        &self,
        _dir: &Path,
        done: Box<dyn FnOnce(Result<Vec<PathBuf>, String>) + Send>,
    ) {
        done(Err("Not available here.".into()));
    }

    /// Whether the collection folder can be opened in the file manager and a terminal (R-GIT-13).
    fn can_open_folders(&self) -> bool {
        false
    }

    fn open_in_file_manager(&self, _path: &Path) -> Result<(), String> {
        Err("Not available here.".into())
    }

    fn open_in_terminal(&self, _path: &Path) -> Result<(), String> {
        Err("Not available here.".into())
    }

    /// The local history of a project (`None`: the scratch request), newest first (R-HIST-1).
    fn history_list(&self, _root: Option<&Path>) -> Result<Vec<history::Summary>, String> {
        Ok(Vec::new())
    }

    /// One history entry, in full.
    fn history_entry(&self, _root: Option<&Path>, _id: &str) -> Result<history::Entry, String> {
        Err("History is not kept here.".into())
    }

    /// Adds an entry to the history. Writing can take a moment, so it may happen in the
    /// background.
    fn add_history(&self, _root: Option<&Path>, _entry: history::Entry) {}

    /// Removes the history of a project (`None`: the scratch request) (R-HIST-3).
    fn clear_history(&self, _root: Option<&Path>) -> Result<(), String> {
        Ok(())
    }

    /// Reads files to upload in multipart bodies (R-HTTP-15); `None` where files can't be read.
    fn file_reader(&self) -> Option<tortuga_core::FileReader> {
        None
    }

    /// Asks for any file (to upload). `None` if cancelled or not possible here.
    fn pick_any_file(&self, _title: &str) -> Option<PathBuf> {
        None
    }

    /// Removes all history, of every project (R-HIST-3).
    fn clear_all_history(&self) -> Result<(), String> {
        Ok(())
    }

    /// Whether history is kept at all (not in the web version yet).
    fn keeps_history(&self) -> bool {
        false
    }

    /// Minutes the local time is ahead of UTC, to show times in history.
    fn utc_offset_minutes(&self) -> i32 {
        0
    }

    /// Runs `task` in the background. The UI cancels it by dropping it (it is abortable).
    fn spawn(&self, task: BoxTask);

    /// Global settings (R-HTTP-8); missing settings give the defaults.
    fn load_settings(&self) -> Result<ClientSettings, String>;
    fn save_settings(&self, settings: &ClientSettings) -> Result<(), String>;

    /// Workspace state (R-LOC-1); missing state gives the default.
    fn load_local(&self) -> Result<LocalState, String>;
    fn save_local(&self, state: &LocalState) -> Result<(), String>;

    /// Unsaved edits from the last session (R-COL-8); missing gives none.
    fn load_drafts(&self) -> Result<SavedDrafts, String> {
        Ok(SavedDrafts::default())
    }

    /// Keeps unsaved edits for the next session, privately (R-COL-8, R-LOC-1).
    fn save_drafts(&self, _drafts: &SavedDrafts) -> Result<(), String> {
        Ok(())
    }

    /// Whether [`Host::pick_folder`] works, i.e. collections can be opened.
    fn can_pick_folders(&self) -> bool {
        false
    }

    /// Asks for a folder. `None` if the user cancelled. May block (native dialogs are modal).
    fn pick_folder(&self, _title: &str) -> Option<PathBuf> {
        None
    }

    /// Asks for an existing file whose name ends with `suffix`. `None` if the user cancelled.
    fn pick_file(&self, _title: &str, _suffix: &str) -> Option<PathBuf> {
        None
    }

    /// Asks where to save a file, without writing it. `None` if the user cancelled.
    fn pick_save_path(&self, _title: &str, _suggested_name: &str) -> Option<PathBuf> {
        None
    }

    /// Whether [`Host::save_file`] works.
    fn can_save_files(&self) -> bool {
        false
    }

    /// Asks where to save `contents` and writes them. `None` if the user cancelled.
    fn save_file(
        &self,
        _suggested_name: &str,
        _contents: &[u8],
    ) -> Option<Result<PathBuf, String>> {
        None
    }
}
