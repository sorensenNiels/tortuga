//! App-wide state: settings, the HTTP client and the open collections.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use tortuga_core::CookieJar;
use tortuga_core::git::GitStatus;
use tortuga_core::secrets::SecretValues;
use tortuga_core::storage::Storage;
use tortuga_core::{Client, ClientSettings, Collection, Request, vars};

use crate::format::sentence;
use crate::host::{Host, LocalState};

/// Identifies a request in an open collection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemKey {
    /// Collection root.
    pub root: PathBuf,
    /// Path relative to the root.
    pub path: PathBuf,
}

impl ItemKey {
    pub fn new(root: &Path, path: impl Into<PathBuf>) -> Self {
        Self {
            root: root.to_owned(),
            path: path.into(),
        }
    }

    /// This key after `old` (a file or folder) moved to `new`; `None` if unaffected.
    pub fn moved(&self, root: &Path, old: &Path, new: &Path) -> Option<Self> {
        if self.root != root {
            return None;
        }
        let rest = self.path.strip_prefix(old).ok()?;
        Some(Self::new(
            root,
            if rest.as_os_str().is_empty() {
                new.to_owned()
            } else {
                new.join(rest)
            },
        ))
    }
}

pub struct AppState {
    pub host: Box<dyn Host>,
    pub settings: ClientSettings,
    /// Rebuilt whenever the settings change.
    pub client: Client,
    /// Open collections, in the order they were opened (R-COL-5).
    pub collections: Vec<Collection>,
    /// Remembered on this machine only (R-LOC-1).
    pub local: LocalState,
    /// Secret variable values of each open collection, loaded when it opens (R-VAR-3).
    pub secrets: BTreeMap<PathBuf, SecretValues>,
    /// Cookie jar of each open collection (R-CK-1), saved locally (R-CK-3).
    pub cookies: BTreeMap<PathBuf, CookieJar>,
    /// Jar of the scratch request; kept for the session only.
    pub scratch_cookies: CookieJar,
    /// Git status of each open collection that is in a git repository (R-GIT-12).
    pub git: BTreeMap<PathBuf, GitStatus>,
    live: Live,
    /// Last problem or message to show the user; cleared when dismissed.
    pub notice: Option<String>,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("settings", &self.settings)
            .field("collections", &self.collections.len())
            .field("local", &self.local)
            .field("notice", &self.notice)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Loads saved settings and reopens the collections that were open last time.
    pub fn new(host: Box<dyn Host>) -> Self {
        let mut problems = Vec::new();
        let settings = host.load_settings().unwrap_or_else(|e| {
            problems.push(e);
            ClientSettings::default()
        });
        let client = Client::new(settings.clone()).unwrap_or_else(|e| {
            problems.push(e.to_string());
            Client::default()
        });
        let mut local = host.load_local().unwrap_or_else(|e| {
            problems.push(e);
            LocalState::default()
        });
        let mut collections = Vec::new();
        let mut secrets = BTreeMap::new();
        let mut cookies = BTreeMap::new();
        local
            .open_collections
            .retain(|root| match Collection::open(host.storage(), root) {
                Ok(c) => {
                    match SecretValues::load(&c, host.secrets()) {
                        Ok(values) => drop(secrets.insert(root.clone(), values)),
                        Err(e) => problems.push(format!("Could not read secret values: {e}")),
                    }
                    let (jar, problem) = load_jar(host.as_ref(), root);
                    problems.extend(problem);
                    cookies.insert(root.clone(), jar);
                    collections.push(c);
                    true
                }
                Err(e) => {
                    problems.push(format!("Could not reopen a collection: {e}"));
                    false
                }
            });
        let mut state = Self {
            host,
            settings,
            client,
            collections,
            local,
            secrets,
            cookies,
            scratch_cookies: CookieJar::default(),
            git: BTreeMap::new(),
            live: Live::default(),
            notice: (!problems.is_empty()).then(|| problems.join("\n")),
        };
        let roots: Vec<PathBuf> = state.collections.iter().map(|c| c.root.clone()).collect();
        for root in roots {
            state.start_watching(&root);
        }
        state
    }

    pub fn storage(&self) -> &dyn Storage {
        self.host.storage()
    }

    pub fn notify(&mut self, message: impl AsRef<str>) {
        self.notice = Some(sentence(message.as_ref()));
    }

    /// Applies new settings: rebuilds the client and saves them.
    pub fn apply_settings(&mut self, settings: &ClientSettings) -> Result<(), String> {
        self.client = Client::new(settings.clone()).map_err(|e| e.to_string())?;
        self.settings = settings.clone();
        self.host.save_settings(settings)
    }

    // ------------------------------------------------------------ collections

    pub fn collection(&self, root: &Path) -> Option<&Collection> {
        self.collections.iter().find(|c| c.root == root)
    }

    /// The saved version of a request.
    pub fn saved_request(&self, key: &ItemKey) -> Option<Request> {
        self.collection(&key.root)?
            .request(&key.path)
            .map(|r| r.request.clone())
    }

    /// Runs a change against the collection on disk, then reloads it so the UI
    /// shows what is on disk. Errors are shown to the user and give `None`.
    pub fn run<T>(
        &mut self,
        root: &Path,
        op: impl FnOnce(&Collection, &dyn Storage) -> tortuga_core::Result<T>,
    ) -> Option<T> {
        let result = op(self.collection(root)?, self.storage());
        self.reload(root);
        match result {
            Ok(value) => Some(value),
            Err(e) => {
                self.notify(e.to_string());
                None
            }
        }
    }

    /// Re-reads a collection from disk.
    pub fn reload(&mut self, root: &Path) {
        match Collection::open(self.storage(), root) {
            Ok(fresh) => {
                if let Some(slot) = self.collections.iter_mut().find(|c| c.root == root) {
                    *slot = fresh;
                }
            }
            Err(e) => self.notify(format!("Could not reload the collection: {e}")),
        }
    }

    /// Opens the collection at `root` (or does nothing if already open).
    pub fn open_collection(&mut self, root: &Path) -> tortuga_core::Result<()> {
        if self.collection(root).is_some() {
            return Ok(());
        }
        let collection = Collection::open(self.storage(), root)?;
        match SecretValues::load(&collection, self.host.secrets()) {
            Ok(values) => drop(self.secrets.insert(root.to_owned(), values)),
            Err(e) => self.notify(format!("Could not read secret values: {e}")),
        }
        let (jar, problem) = load_jar(self.host.as_ref(), root);
        if let Some(problem) = problem {
            self.notify(problem);
        }
        self.cookies.insert(root.to_owned(), jar);
        self.collections.push(collection);
        self.start_watching(root);
        self.local.open_collections.push(root.to_owned());
        self.save_local();
        Ok(())
    }

    pub fn create_collection(&mut self, root: &Path, name: &str) -> tortuga_core::Result<()> {
        Collection::create(self.storage(), root, name)?;
        self.open_collection(root)
    }

    pub fn close_collection(&mut self, root: &Path) {
        self.save_cookies(Some(root));
        self.host.unwatch(root);
        self.git.remove(root);
        self.collections.retain(|c| c.root != root);
        self.secrets.remove(root);
        self.cookies.remove(root);
        self.local.open_collections.retain(|r| r != root);
        self.local.active_environments.remove(&root_key(root));
        self.save_local();
    }

    // ------------------------------------------------------------ environments

    /// The active environment of a collection (R-SRV-5); defaults to the first one.
    pub fn active_environment(&self, root: &Path) -> Option<String> {
        let c = self.collection(root)?;
        match self.local.active_environments.get(&root_key(root)) {
            Some(id) if c.environment(id).is_some() => Some(id.clone()),
            _ => c.environments.first().map(|e| e.id.clone()),
        }
    }

    pub fn set_active_environment(&mut self, root: &Path, id: &str) {
        self.local
            .active_environments
            .insert(root_key(root), id.to_owned());
        self.save_local();
    }

    /// Servers and variables for requests of the collection at `root`; empty for scratch requests.
    pub fn context(&self, root: Option<&Path>) -> vars::Context {
        let Some(root) = root else {
            return vars::Context::default();
        };
        let env = self.active_environment(root);
        let secrets = self
            .secrets
            .get(root)
            .map(|s| s.for_environment(env.as_deref()))
            .unwrap_or_default();
        self.collection(root)
            .map(|c| vars::Context::for_collection(c, env.as_deref()).with_secrets(secrets))
            .unwrap_or_default()
    }

    // ------------------------------------------------------------ live changes (M6)

    /// Lets background threads wake the UI. Call once the egui context exists.
    pub fn set_repaint(&self, ctx: &egui::Context) {
        let _ = self.live.repaint.set(ctx.clone());
    }

    /// Watches a collection's files and git state (R-GIT-10) and reads its git status.
    fn start_watching(&mut self, root: &Path) {
        let tx = self.live.changed_tx.clone();
        let repaint = self.live.repaint.clone();
        let changed_root = root.to_owned();
        let on_change = move || {
            let _ = tx.send(changed_root.clone());
            if let Some(ctx) = repaint.get() {
                ctx.request_repaint();
            }
        };
        if let Err(e) = self.host.watch(root, Box::new(on_change)) {
            self.notify(format!(
                "Changes on disk won't show automatically (use \u{201c}Reload from disk\u{201d}): {e}"
            ));
        }
        self.refresh_git(root);
    }

    /// Reads the git status of a collection again, in the background (R-GIT-12).
    pub fn refresh_git(&mut self, root: &Path) {
        if !self.live.git_in_flight.insert(root.to_owned()) {
            // Already reading: read once more when that finishes, to see the latest state.
            self.live.git_again.insert(root.to_owned());
            return;
        }
        let tx = self.live.git_tx.clone();
        let repaint = self.live.repaint.clone();
        let for_root = root.to_owned();
        self.host.git_status(
            root,
            Box::new(move |result| {
                let _ = tx.send((for_root, result));
                if let Some(ctx) = repaint.get() {
                    ctx.request_repaint();
                }
            }),
        );
    }

    /// Takes in finished git statuses and returns the collections whose files changed
    /// on disk since the last call (each once).
    pub fn take_changes(&mut self) -> Vec<PathBuf> {
        while let Ok((root, result)) = self.live.git_rx.try_recv() {
            self.live.git_in_flight.remove(&root);
            match result {
                Ok(Some(status)) => drop(self.git.insert(root.clone(), status)),
                Ok(None) => drop(self.git.remove(&root)),
                Err(e) => {
                    self.git.remove(&root);
                    self.notify(format!("Could not read the git status: {e}"));
                }
            }
            if self.live.git_again.remove(&root) && self.collection(&root).is_some() {
                self.refresh_git(&root);
            }
        }
        let mut changed = BTreeSet::new();
        while let Ok(root) = self.live.changed_rx.try_recv() {
            if self.collection(&root).is_some() {
                changed.insert(root);
            }
        }
        changed.into_iter().collect()
    }

    /// The cookie jar requests of the collection at `root` use; the scratch jar for `None` (R-CK-1).
    pub fn cookie_jar(&self, root: Option<&Path>) -> CookieJar {
        root.and_then(|r| self.cookies.get(r))
            .unwrap_or(&self.scratch_cookies)
            .clone()
    }

    /// Writes a collection's jar to local data after a send changed it (R-CK-3).
    pub fn save_cookies(&mut self, root: Option<&Path>) {
        let Some((root, jar)) = root.and_then(|r| self.cookies.get_key_value(r)) else {
            return;
        };
        if let Err(e) = self.host.save_cookies(root, &jar.to_json()) {
            self.notify(format!("Could not save cookies: {e}"));
        }
    }

    /// Empties a collection's jar and saves it (R-CK-3).
    pub fn clear_cookies(&mut self, root: &Path) {
        if let Some(jar) = self.cookies.get(root) {
            jar.clear();
        }
        self.save_cookies(Some(root));
    }

    /// Stores new secret values for the collection at `root` on this machine (R-VAR-3).
    pub fn save_secrets(&mut self, root: &Path, values: SecretValues) -> Result<(), String> {
        let old = self.secrets.get(root).cloned().unwrap_or_default();
        tortuga_core::secrets::save(self.host.secrets(), root, &old, &values)
            .map_err(|e| sentence(&e.to_string()))?;
        self.secrets.insert(root.to_owned(), values);
        Ok(())
    }

    fn save_local(&mut self) {
        if let Err(e) = self.host.save_local(&self.local) {
            self.notify(e);
        }
    }
}

/// Results from background threads: changed collections and finished git statuses.
struct Live {
    changed_tx: Sender<PathBuf>,
    changed_rx: Receiver<PathBuf>,
    git_tx: Sender<(PathBuf, Result<Option<GitStatus>, String>)>,
    git_rx: Receiver<(PathBuf, Result<Option<GitStatus>, String>)>,
    repaint: Arc<OnceLock<egui::Context>>,
    git_in_flight: BTreeSet<PathBuf>,
    git_again: BTreeSet<PathBuf>,
}

impl Default for Live {
    fn default() -> Self {
        let (changed_tx, changed_rx) = channel();
        let (git_tx, git_rx) = channel();
        Self {
            changed_tx,
            changed_rx,
            git_tx,
            git_rx,
            repaint: Arc::default(),
            git_in_flight: BTreeSet::new(),
            git_again: BTreeSet::new(),
        }
    }
}

/// Loads the saved jar of `root`; a broken file gives an empty jar and a problem to report.
fn load_jar(host: &dyn Host, root: &Path) -> (CookieJar, Option<String>) {
    match host.load_cookies(root) {
        Ok(None) => (CookieJar::default(), None),
        Ok(Some(text)) => match CookieJar::from_json(&text) {
            Ok(jar) => (jar, None),
            Err(e) => (CookieJar::default(), Some(e.to_string())),
        },
        Err(e) => (
            CookieJar::default(),
            Some(format!("Could not read saved cookies: {e}")),
        ),
    }
}

fn root_key(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-CK-3: jars are saved as local data and come back when the collection opens again.
    #[test]
    fn cookie_jars_are_saved_locally_and_cleared() {
        use tortuga_core::storage::FsStorage;

        let root = std::env::temp_dir().join(format!("tortuga-cookies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let host = crate::test_host::TestHost {
            open: vec![root.clone()],
            ..Default::default()
        };
        let saved = host.cookies.clone();

        let mut state = AppState::new(Box::new(host));
        let url = url::Url::parse("http://api.test/").expect("url");
        state
            .cookie_jar(Some(&root))
            .store_response_cookies(["session=abc"], &url);
        state.save_cookies(Some(&root));
        assert!(saved.borrow()[&root].contains("session"));

        let reopened = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![root.clone()],
            cookies: saved.clone(),
            ..Default::default()
        }));
        assert_eq!(reopened.cookie_jar(Some(&root)).len(), 1);

        state.clear_cookies(&root);
        assert!(state.cookie_jar(Some(&root)).is_empty());
        assert!(
            !saved.borrow()[&root].contains("session"),
            "cleared jar is saved"
        );
        assert!(
            !std::fs::read_dir(&root).expect("collection dir").any(|e| e
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains("cookie")),
            "nothing about cookies in the collection directory"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn keys_follow_moved_items() {
        let root = Path::new("/c");
        let key = ItemKey::new(root, "users/list.toml");
        assert_eq!(
            key.moved(root, Path::new("users"), Path::new("people")),
            Some(ItemKey::new(root, "people/list.toml"))
        );
        assert_eq!(
            key.moved(root, Path::new("users/list.toml"), Path::new("x.toml")),
            Some(ItemKey::new(root, "x.toml"))
        );
        assert_eq!(key.moved(root, Path::new("other"), Path::new("x")), None);
        assert_eq!(
            key.moved(Path::new("/d"), Path::new("users"), Path::new("x")),
            None
        );
    }
}
