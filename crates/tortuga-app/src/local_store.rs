//! Data kept on this machine only, never in a collection (R-LOC-1):
//! global settings (config dir) and workspace state (data dir). Desktop only.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tortuga_core::AppSettings;
use tortuga_ui_egui::{LocalState, SavedDrafts};

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

/// Folder with the request history of the project at `root`, or of the scratch
/// request (R-HIST-1). Named by a hash of the path, like the cookie files.
pub fn history_dir(root: Option<&Path>) -> Option<PathBuf> {
    let name = root.map_or_else(
        || "scratch".to_owned(),
        |r| format!("{:016x}", fnv1a(r.to_string_lossy().as_bytes())),
    );
    dirs().map(|d| d.data_dir().join("history").join(name))
}

/// Folder with all history (R-HIST-3).
pub fn all_history_dir() -> Option<PathBuf> {
    dirs().map(|d| d.data_dir().join("history"))
}

/// Files only the current user can read, like [`write_private`]: history may hold
/// credentials typed into a request (R-HIST-1).
#[derive(Debug, Default, Clone, Copy)]
pub struct PrivateStorage;

impl tortuga_core::storage::Storage for PrivateStorage {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str) -> std::io::Result<()> {
        write_private(path, contents)
    }

    fn list(&self, dir: &Path) -> std::io::Result<Vec<tortuga_core::storage::Entry>> {
        tortuga_core::storage::FsStorage.list(dir)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }
}

/// Unsaved request edits (R-COL-8). They may hold credentials typed into a request,
/// so the file is private to the user like cookies and secrets.
fn drafts_path() -> Option<PathBuf> {
    dirs().map(|d| d.data_dir().join("drafts.json"))
}

pub fn load_drafts() -> Result<SavedDrafts, String> {
    let Some(path) = drafts_path() else {
        return Ok(SavedDrafts::default());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("Could not read {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SavedDrafts::default()),
        Err(e) => Err(format!("Could not read {}: {e}", path.display())),
    }
}

pub fn save_drafts(drafts: &SavedDrafts) -> Result<(), String> {
    let path = drafts_path().ok_or("No app-data directory available")?;
    let text = serde_json::to_string(drafts).map_err(|e| e.to_string())?;
    write_private(&path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_settings() -> Result<AppSettings, String> {
    load(settings_path())
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
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

    use super::*;

    #[test]
    fn cookie_files_are_named_by_a_stable_hash() {
        // Known FNV-1a test vector: the empty input.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a(b"/repo/a"), fnv1a(b"/repo/b"));
    }

    /// R-HIST-1: history can hold credentials, so only the user can read it.
    #[cfg(unix)]
    #[test]
    fn history_files_are_private() {
        use std::os::unix::fs::PermissionsExt as _;
        use tortuga_core::history::{Entry, HistoryStore, new_id};
        use tortuga_core::{HttpMethod, Request};

        let dir = std::env::temp_dir().join(format!("tortuga-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = HistoryStore::new(&PrivateStorage, dir.join("p"));
        let request = Request::new(HttpMethod::Get, "https://a.test/");
        let entry = Entry::new(new_id(1, 1), 1, None, &request, None, Err("down".into()));
        store
            .add(&entry, tortuga_core::history::DEFAULT_SIZE)
            .expect("add");
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join("p")), 0o700);
        assert_eq!(mode(&dir.join("p/index.json")), 0o600);
        assert_eq!(
            mode(&dir.join(format!("p/{}.json", entry.summary.id))),
            0o600
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-HTTP-16, R-COL-18: CA certificate paths and importer choices are kept in the
    /// settings; older files still load.
    #[test]
    fn settings_round_trip_with_ca_certificates() {
        let mut settings = AppSettings::default();
        settings.client.ca_certificates = vec![PathBuf::from("/etc/ssl/company-ca.pem")];
        settings
            .importers
            .set_enabled(tortuga_core::import::ImportKind::Http, false);
        let text = toml::to_string(&settings).expect("serialize");
        assert_eq!(
            toml::from_str::<AppSettings>(&text).expect("parse"),
            settings
        );
        let old: AppSettings = toml::from_str(
            "follow_redirects = true\nverify_tls = true\ndefault_timeout_ms = 500\n",
        )
        .expect("old file");
        assert_eq!(old.client.ca_certificates, Vec::<PathBuf>::new());
        assert_eq!(old.client.default_timeout_ms, 500);
    }

    /// R-COL-8, R-FLOW-5: unsaved edits round-trip through the drafts file.
    #[test]
    fn drafts_round_trip() {
        use tortuga_core::workflow::{Step, Workflow};
        use tortuga_core::{HttpMethod, Request, RequestBody};
        use tortuga_ui_egui::{DraftConflict, SavedDraft, SavedWorkflowDraft};

        let base = Workflow {
            name: "Smoke".into(),
            ..Workflow::default()
        };
        let workflow = Workflow {
            steps: vec![Step::new(std::path::Path::new("users/create.toml"))],
            ..base.clone()
        };

        let mut edited = Request::new(HttpMethod::Post, "https://api.test/users?x=1");
        edited.body = RequestBody::Json("{\"a\": 1}".into());
        let drafts = SavedDrafts {
            scratch: Some(Request::new(HttpMethod::Get, "https://scratch.test/")),
            drafts: vec![SavedDraft {
                root: "/repo/api".into(),
                path: "users/create.toml".into(),
                request: edited,
                base: Some(Request::new(HttpMethod::Post, "https://api.test/users")),
                conflict: Some(DraftConflict::Changed),
            }],
            workflows: vec![SavedWorkflowDraft {
                root: "/repo/api".into(),
                id: "smoke".into(),
                workflow,
                base: Some(base),
            }],
        };
        let text = serde_json::to_string(&drafts).expect("serialize");
        assert_eq!(
            serde_json::from_str::<SavedDrafts>(&text).expect("parse"),
            drafts
        );
        assert_eq!(
            serde_json::from_str::<SavedDrafts>("{}").expect("empty"),
            SavedDrafts::default()
        );
    }

    #[test]
    fn workspace_state_round_trips_and_old_files_load() {
        use tortuga_core::workspace::ProjectRef;
        use tortuga_ui_egui::{SavedTab, SavedTabs, Workspace};

        let state = LocalState {
            workspaces: vec![
                Workspace {
                    name: "My workspace".into(),
                    projects: vec![ProjectRef::new("/repo/api")],
                    selection: Some(("/repo/api".into(), "users/list.toml".into())),
                    // R-COL-13, R-COL-14: the scratch tab, a kept tab and the preview tab.
                    tabs: SavedTabs {
                        open: vec![
                            SavedTab::default(),
                            SavedTab {
                                root: Some("/repo/api".into()),
                                path: Some("users/list.toml".into()),
                                preview: false,
                            },
                            SavedTab {
                                root: Some("/repo/api".into()),
                                path: Some("users/get.toml".into()),
                                preview: true,
                            },
                        ],
                        collapsed: vec!["/repo/api".into()],
                    },
                    // R-COL-36: a folded project and a folded folder.
                    folded: vec![
                        ("/repo/api".into(), PathBuf::new()),
                        ("/repo/api".into(), "users ü😀".into()),
                    ],
                    ..Workspace::default()
                },
                Workspace {
                    name: "Acme".into(),
                    file: Some("/src/acme.tortuga-workspace.toml".into()),
                    environment: Some("Staging".into()),
                    // R-WS-9: the environments every project has.
                    environments: vec![tortuga_core::workspace::WorkspaceEnvironment {
                        id: "staging".into(),
                        name: "Staging ü😀".into(),
                        protected: true,
                        servers: [("Users".to_owned(), "https://users.stg".to_owned())].into(),
                        secrets: ["token".to_owned()].into(),
                        ..Default::default()
                    }],
                    projects: vec![ProjectRef {
                        path: "/src/users".into(),
                        remote: Some("git@example.test:acme/users.git".into()),
                    }],
                    ..Workspace::default()
                },
            ],
            active_workspace: 1,
            ..LocalState::default()
        };
        let text = toml::to_string(&state).expect("serialize");
        assert_eq!(toml::from_str::<LocalState>(&text).expect("parse"), state);
        assert_eq!(
            toml::from_str::<LocalState>("").expect("empty file"),
            LocalState::default()
        );

        // Saved before workspaces (M6.5): becomes the first workspace.
        let mut old: LocalState = toml::from_str(
            "open_collections = [\"/repo/api\"]\n[active_environments]\n\"/repo/api\" = \"dev\"\n",
        )
        .expect("old file");
        old.migrate();
        assert_eq!(old.workspaces.len(), 1);
        assert_eq!(old.workspaces[0].projects, [ProjectRef::new("/repo/api")]);
        assert_eq!(old.open_collections, Vec::<PathBuf>::new());
    }
}
