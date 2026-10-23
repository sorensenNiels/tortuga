//! Where secret variable values live on the desktop (R-VAR-3, R-LOC-2).
//!
//! The OS credential store (macOS Keychain, Windows Credential Manager, Linux
//! Secret Service) when it works; otherwise a file in the app-data directory that
//! only the current user can read, and the app shows a warning.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tortuga_ui_egui::SecretsWarning;

use tortuga_core::secrets::{SecretKey, SecretStore};
use tortuga_core::{Error, Result};

/// Service name of every Tortuga entry in the OS credential store.
const SERVICE: &str = "Tortuga";

/// Opens the best available store. The second value is a warning to show when
/// values have to be kept in a file instead of the OS credential store.
pub fn open(fallback_path: Option<PathBuf>) -> (Box<dyn SecretStore>, Option<SecretsWarning>) {
    let reason = match probe_keyring() {
        Ok(()) => return (Box::new(KeyringStore), None),
        Err(reason) => reason,
    };
    match fallback_path {
        Some(path) => match FileStore::open(path.clone()) {
            Ok(store) => (
                Box::new(store),
                Some(SecretsWarning::InFile { reason, path }),
            ),
            Err(error) => (
                Box::new(tortuga_core::secrets::MemorySecretStore::default()),
                Some(SecretsWarning::FileUnreadable {
                    reason,
                    path,
                    error,
                }),
            ),
        },
        None => (
            Box::new(tortuga_core::secrets::MemorySecretStore::default()),
            Some(SecretsWarning::NoFolder { reason }),
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
        crate::local_store::write_private(&self.path, &text).map_err(|source| Error::Io {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

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
