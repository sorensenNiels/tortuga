//! App-wide state: settings, the HTTP client and the open collections.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

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
        local
            .open_collections
            .retain(|root| match Collection::open(host.storage(), root) {
                Ok(c) => {
                    match SecretValues::load(&c, host.secrets()) {
                        Ok(values) => drop(secrets.insert(root.clone(), values)),
                        Err(e) => problems.push(format!("Could not read secret values: {e}")),
                    }
                    collections.push(c);
                    true
                }
                Err(e) => {
                    problems.push(format!("Could not reopen a collection: {e}"));
                    false
                }
            });
        Self {
            host,
            settings,
            client,
            collections,
            local,
            secrets,
            notice: (!problems.is_empty()).then(|| problems.join("\n")),
        }
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
        self.collections.push(collection);
        self.local.open_collections.push(root.to_owned());
        self.save_local();
        Ok(())
    }

    pub fn create_collection(&mut self, root: &Path, name: &str) -> tortuga_core::Result<()> {
        Collection::create(self.storage(), root, name)?;
        self.open_collection(root)
    }

    pub fn close_collection(&mut self, root: &Path) {
        self.collections.retain(|c| c.root != root);
        self.secrets.remove(root);
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

fn root_key(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
