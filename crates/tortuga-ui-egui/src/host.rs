//! What the UI needs from the platform it runs on.
//!
//! The UI itself never touches the file system, native dialogs or an async runtime
//! directly; the launcher (`tortuga-app`) provides them through [`Host`], so the same
//! UI runs on desktop and (later) in the browser.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tortuga_core::ClientSettings;
use tortuga_core::storage::Storage;

/// Which collections are open (R-COL-5) and the active environment of each (R-SRV-5).
/// Stored on this machine only, never in a collection (R-LOC-1).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalState {
    pub open_collections: Vec<PathBuf>,
    /// Collection root (as text) → environment id.
    pub active_environments: BTreeMap<String, String>,
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

    /// Runs `task` in the background. The UI cancels it by dropping it (it is abortable).
    fn spawn(&self, task: BoxTask);

    /// Global settings (R-HTTP-8); missing settings give the defaults.
    fn load_settings(&self) -> Result<ClientSettings, String>;
    fn save_settings(&self, settings: &ClientSettings) -> Result<(), String>;

    /// Workspace state (R-LOC-1); missing state gives the default.
    fn load_local(&self) -> Result<LocalState, String>;
    fn save_local(&self, state: &LocalState) -> Result<(), String>;

    /// Whether [`Host::pick_folder`] works, i.e. collections can be opened.
    fn can_pick_folders(&self) -> bool {
        false
    }

    /// Asks for a folder. `None` if the user cancelled. May block (native dialogs are modal).
    fn pick_folder(&self, _title: &str) -> Option<PathBuf> {
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
