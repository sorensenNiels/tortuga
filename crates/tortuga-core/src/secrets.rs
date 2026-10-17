//! Secret variable values (R-VAR-3, R-LOC-1/2).
//!
//! A collection only lists the *names* of its secret variables
//! ([`crate::Manifest::secrets`]). The values live on this machine, in a
//! [`SecretStore`] the app provides (the OS credential store on desktop),
//! keyed by collection path, environment and variable name. They are never
//! written into the collection directory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::collection::Collection;

/// Identifies one secret value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecretKey {
    /// Collection root.
    pub collection: PathBuf,
    /// Environment id; `None` = the default used by every environment without its own value.
    pub environment: Option<String>,
    pub name: String,
}

impl SecretKey {
    pub fn new(collection: &Path, environment: Option<&str>, name: &str) -> Self {
        Self {
            collection: collection.to_owned(),
            environment: environment.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    /// A single string for stores that key entries by text, e.g.
    /// `/repo/api|dev|token` (`*` for the default).
    pub fn account(&self) -> String {
        format!(
            "{}|{}|{}",
            self.collection.display(),
            self.environment.as_deref().unwrap_or("*"),
            self.name
        )
    }
}

/// Where secret values are kept on this machine.
pub trait SecretStore {
    /// `Ok(None)` when no value is stored.
    fn get(&self, key: &SecretKey) -> Result<Option<String>>;
    fn set(&self, key: &SecretKey, value: &str) -> Result<()>;
    /// Removing a value that does not exist is not an error.
    fn delete(&self, key: &SecretKey) -> Result<()>;
}

/// Keeps values in memory for the session (web build, tests).
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    values: RefCell<BTreeMap<SecretKey, String>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>> {
        Ok(self.values.borrow().get(key).cloned())
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<()> {
        self.values
            .borrow_mut()
            .insert(key.clone(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        self.values.borrow_mut().remove(key);
        Ok(())
    }
}

/// The local values of one collection's secret variables.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecretValues {
    /// (environment id or `None` for the default, variable name) → value.
    pub values: BTreeMap<(Option<String>, String), String>,
}

impl SecretValues {
    /// Reads every secret of `collection` (default and per environment) from `store`.
    pub fn load(collection: &Collection, store: &dyn SecretStore) -> Result<Self> {
        let mut values = BTreeMap::new();
        let envs = std::iter::once(None)
            .chain(collection.environments.iter().map(|e| Some(e.id.as_str())));
        for env in envs {
            for name in &collection.manifest.secrets {
                let key = SecretKey::new(&collection.root, env, name);
                if let Some(value) = store.get(&key)? {
                    values.insert((env.map(str::to_owned), name.clone()), value);
                }
            }
        }
        Ok(Self { values })
    }

    pub fn get(&self, environment: Option<&str>, name: &str) -> Option<&str> {
        self.values
            .get(&(environment.map(str::to_owned), name.to_owned()))
            .map(String::as_str)
    }

    /// Values for `environment`: its own value, else the default (R-VAR-4). Empty values are skipped.
    pub fn for_environment(&self, environment: Option<&str>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for ((env, name), value) in &self.values {
            if value.is_empty() {
                continue;
            }
            match env.as_deref() {
                None => {
                    out.entry(name.clone()).or_insert_with(|| value.clone());
                }
                Some(e) if Some(e) == environment => {
                    out.insert(name.clone(), value.clone());
                }
                Some(_) => {}
            }
        }
        out
    }
}

/// Writes `values` for `collection` to `store`: entries in `values` are set,
/// entries in `old` but not in `values` are deleted.
pub fn save(
    store: &dyn SecretStore,
    collection: &Path,
    old: &SecretValues,
    values: &SecretValues,
) -> Result<()> {
    for ((env, name), value) in &values.values {
        if old.values.get(&(env.clone(), name.clone())) != Some(value) {
            store.set(&SecretKey::new(collection, env.as_deref(), name), value)?;
        }
    }
    for (env, name) in old.values.keys() {
        if !values.values.contains_key(&(env.clone(), name.clone())) {
            store.delete(&SecretKey::new(collection, env.as_deref(), name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(entries: &[(Option<&str>, &str, &str)]) -> SecretValues {
        SecretValues {
            values: entries
                .iter()
                .map(|(e, n, v)| ((e.map(str::to_owned), (*n).to_owned()), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn environment_value_wins_over_default() {
        let v = values(&[
            (None, "token", "default-token"),
            (Some("dev"), "token", "dev-token"),
            (None, "key", "k"),
            (Some("dev"), "empty", ""),
        ]);
        let dev = v.for_environment(Some("dev"));
        assert_eq!(dev["token"], "dev-token");
        assert_eq!(dev["key"], "k");
        assert!(!dev.contains_key("empty"), "empty values are not set");
        assert_eq!(v.for_environment(Some("prod"))["token"], "default-token");
        assert_eq!(v.for_environment(None)["token"], "default-token");
    }

    #[test]
    fn save_sets_changed_and_deletes_removed_values() {
        let store = MemorySecretStore::default();
        let root = Path::new("/c");
        let first = values(&[(None, "a", "1"), (Some("dev"), "b", "2")]);
        save(&store, root, &SecretValues::default(), &first).expect("save");
        let key_a = SecretKey::new(root, None, "a");
        let key_b = SecretKey::new(root, Some("dev"), "b");
        assert_eq!(store.get(&key_a).expect("get").as_deref(), Some("1"));

        let second = values(&[(None, "a", "3")]);
        save(&store, root, &first, &second).expect("save");
        assert_eq!(store.get(&key_a).expect("get").as_deref(), Some("3"));
        assert_eq!(store.get(&key_b).expect("get"), None);
    }

    #[test]
    fn account_names_are_unambiguous_per_environment() {
        let root = Path::new("/repo/api");
        assert_eq!(
            SecretKey::new(root, None, "token").account(),
            "/repo/api|*|token"
        );
        assert_eq!(
            SecretKey::new(root, Some("dev"), "token").account(),
            "/repo/api|dev|token"
        );
    }
}
