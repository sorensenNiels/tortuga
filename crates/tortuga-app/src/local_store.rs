//! Data kept on this machine only, never in a collection (R-LOC-1):
//! global settings (config dir) and workspace state (data dir). Desktop only.

use std::path::PathBuf;

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
