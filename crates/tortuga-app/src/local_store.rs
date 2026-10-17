//! Data kept on this machine only, never in a collection (R-LOC-1):
//! global settings (config dir) and workspace state (data dir). Desktop only.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tortuga_core::ClientSettings;
use tortuga_ui_egui::LocalState;

fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("dk", "SoftDesign", "Tortuga")
}

fn settings_path() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("settings.toml"))
}

fn local_path() -> Option<PathBuf> {
    dirs().map(|d| d.data_dir().join("workspace.toml"))
}

/// Fallback file for secret values when there is no OS credential store (R-LOC-2).
pub fn secrets_path() -> Option<PathBuf> {
    dirs().map(|d| d.data_dir().join("secrets.toml"))
}

fn load<T: DeserializeOwned + Default>(path: Option<PathBuf>) -> Result<T, String> {
    let Some(path) = path else {
        return Ok(T::default());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|e| format!("Could not read {}: {e}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

fn save<T: Serialize>(path: Option<PathBuf>, value: &T) -> Result<(), String> {
    let path = path.ok_or("No configuration directory available")?;
    let text = toml::to_string(value).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// File for the cookie jar of the collection at `root` (R-CK-3): named by a hash of
/// the path, so any path works as a file name.
fn cookies_path(root: &Path) -> Option<PathBuf> {
    let key = fnv1a(root.to_string_lossy().as_bytes());
    dirs().map(|d| {
        d.data_dir()
            .join("cookies")
            .join(format!("{key:016x}.json"))
    })
}

/// Stable 64-bit FNV-1a (std's hasher may change between Rust versions).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The saved cookie jar of a collection, if any.
pub fn load_cookies(root: &Path) -> Result<Option<String>, String> {
    let Some(path) = cookies_path(root) else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

/// Cookies can hold session tokens, so the file is private to the user like secrets.
pub fn save_cookies(root: &Path, json: &str) -> Result<(), String> {
    let path = cookies_path(root).ok_or("No app-data directory available")?;
    write_private(&path, json).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `text` to `path` through a temporary file, so a failed write never
/// leaves half a file. On Unix the file is created with mode 0600; on Windows
/// the per-user app-data folder is already private to the user.
pub fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

pub fn load_settings() -> Result<ClientSettings, String> {
    load(settings_path())
}

pub fn save_settings(settings: &ClientSettings) -> Result<(), String> {
    save(settings_path(), settings)
}

pub fn load_local() -> Result<LocalState, String> {
    load(local_path())
}

pub fn save_local(state: &LocalState) -> Result<(), String> {
    save(local_path(), state)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn cookie_files_are_named_by_a_stable_hash() {
        // Known FNV-1a test vector: the empty input.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a(b"/repo/a"), fnv1a(b"/repo/b"));
    }

    #[test]
    fn workspace_state_round_trips_and_old_files_load() {
        let state = LocalState {
            open_collections: vec![PathBuf::from("/repo/api")],
            active_environments: BTreeMap::from([("/repo/api".into(), "dev".into())]),
        };
        let text = toml::to_string(&state).expect("serialize");
        assert_eq!(toml::from_str::<LocalState>(&text).expect("parse"), state);
        assert_eq!(
            toml::from_str::<LocalState>("").expect("empty file"),
            LocalState::default()
        );
    }
}
