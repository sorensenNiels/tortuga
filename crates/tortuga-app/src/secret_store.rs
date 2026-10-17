//! Where secret variable values live on the desktop (R-VAR-3, R-LOC-2).
//!
//! The OS credential store (macOS Keychain, Windows Credential Manager, Linux
//! Secret Service) when it works; otherwise a file in the app-data directory that
//! only the current user can read, and the app shows a warning.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tortuga_core::secrets::{SecretKey, SecretStore};
use tortuga_core::{Error, Result};

/// Service name of every Tortuga entry in the OS credential store.
const SERVICE: &str = "Tortuga";

/// Opens the best available store. The second value is a warning to show when
/// values have to be kept in a file instead of the OS credential store.
pub fn open(fallback_path: Option<PathBuf>) -> (Box<dyn SecretStore>, Option<String>) {
    let reason = match probe_keyring() {
        Ok(()) => return (Box::new(KeyringStore), None),
        Err(reason) => reason,
    };
    match fallback_path {
        Some(path) => {
            let warning = format!(
                "No OS credential store is available ({reason}). Secret values are kept in {}, \
                 readable only by your user account.",
                path.display()
            );
            match FileStore::open(path) {
                Ok(store) => (Box::new(store), Some(warning)),
                Err(e) => (
                    Box::new(tortuga_core::secrets::MemorySecretStore::default()),
                    Some(format!(
                        "{warning} That file could not be read ({e}), so secret values are only kept until the app closes."
                    )),
                ),
            }
        }
        None => (
            Box::new(tortuga_core::secrets::MemorySecretStore::default()),
            Some(format!(
                "No OS credential store is available ({reason}) and no app-data folder was found. \
                 Secret values are only kept until the app closes."
            )),
        ),
    }
}

/// Checks that the credential store answers: a lookup of an entry that does not exist.
fn probe_keyring() -> std::result::Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, "probe").map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The OS credential store, one entry per secret value.
struct KeyringStore;

fn store_error(e: &keyring::Error) -> Error {
    Error::SecretStore(e.to_string())
}

impl SecretStore for KeyringStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, &key.account()).map_err(|e| store_error(&e))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(store_error(&e)),
        }
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, &key.account())
            .and_then(|entry| entry.set_password(value))
            .map_err(|e| store_error(&e))
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, &key.account()).map_err(|e| store_error(&e))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(store_error(&e)),
        }
    }
}

/// Fallback: a TOML file (account → value) readable only by the current user.
struct FileStore {
    path: PathBuf,
    values: RefCell<BTreeMap<String, String>>,
}

impl FileStore {
    fn open(path: PathBuf) -> std::result::Result<Self, String> {
        let values = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self {
            path,
            values: RefCell::new(values),
        })
    }

    fn write(&self) -> Result<()> {
        let text = toml::to_string(&*self.values.borrow())
            .map_err(|e| Error::SecretStore(e.to_string()))?;
        write_private(&self.path, &text).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>> {
        Ok(self.values.borrow().get(&key.account()).cloned())
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<()> {
        self.values
            .borrow_mut()
            .insert(key.account(), value.to_owned());
        self.write()
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        if self.values.borrow_mut().remove(&key.account()).is_some() {
            self.write()?;
        }
        Ok(())
    }
}

/// Writes `text` to `path` through a temporary file, so a failed write never
/// leaves half a file. On Unix the file is created with mode 0600; on Windows
/// the per-user app-data folder is already private to the user.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_persists_and_is_private() {
        let dir = std::env::temp_dir().join(format!("tortuga-secrets-{}", std::process::id()));
        let path = dir.join("secrets.toml");
        let key = SecretKey::new(Path::new("/repo/api"), Some("dev"), "token");

        let store = FileStore::open(path.clone()).expect("open");
        store.set(&key, "s3cret").expect("set");
        let reopened = FileStore::open(path.clone()).expect("reopen");
        assert_eq!(reopened.get(&key).expect("get").as_deref(), Some("s3cret"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "only the owner may read secrets");
        }

        reopened.delete(&key).expect("delete");
        assert_eq!(
            FileStore::open(path).expect("open").get(&key).expect("get"),
            None
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
