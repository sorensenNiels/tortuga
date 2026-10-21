//! Environment files imported into a project's environments: Postman environments
//! (R-COL-20, [`crate::postman::parse_environment`]) and the HTTP clients' environment
//! files (R-COL-29, [`crate::http_file::parse_environments`]). Into the project an
//! import is about to create, or into one that exists.
//!
//! A value whose name is a server's becomes that server's base URL in the environment
//! (D36); a secret one (Postman's "secret" type, a private HTTP Client file, or a name
//! the project already keeps secret) becomes a secret variable whose value goes to this
//! machine's secret store for that environment only (D35); the rest become environment
//! variables. An environment named like one the project has updates it (D49).

use std::collections::BTreeMap;
use std::fmt;

use crate::collection::Collection;
use crate::format::{Environment, Manifest};
use crate::import::{Import, base_url, no_base_url};

/// One value of an imported environment.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EnvValue {
    pub key: String,
    pub value: String,
    pub secret: bool,
}

/// One environment read from an environment file.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportedEnvironment {
    pub name: String,
    pub(crate) values: Vec<EnvValue>,
    /// What of it could not be imported as written, as sentences for the user.
    pub(crate) warnings: Vec<String>,
}

// Hand-written: the values may be secrets.
impl fmt::Debug for ImportedEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self.values.iter().map(|v| v.key.as_str()).collect();
        f.debug_struct("ImportedEnvironment")
            .field("name", &self.name)
            .field("keys", &keys)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// A project's configuration with environments imported into it.
#[derive(Clone, PartialEq, Eq)]
pub struct EnvironmentImport {
    /// The project's manifest, with secret variables the files added.
    pub manifest: Manifest,
    /// Every environment of the project, the imported ones updated or added.
    pub environments: Vec<Environment>,
    /// Names of environments the import adds, and of those it updates.
    pub added: Vec<String>,
    pub updated: Vec<String>,
    /// Values for this machine's secret store: (environment id, or `None` for the
    /// default of every environment, variable name) → value. Never written to files.
    pub secret_values: BTreeMap<(Option<String>, String), String>,
    /// What the files had that could not be imported as written.
    pub warnings: Vec<String>,
}

// Hand-written: `secret_values` are secrets.
impl fmt::Debug for EnvironmentImport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secrets: Vec<_> = self.secret_values.keys().collect();
        f.debug_struct("EnvironmentImport")
            .field("manifest", &self.manifest)
            .field("environments", &self.environments)
            .field("added", &self.added)
            .field("updated", &self.updated)
            .field("secret_values", &secrets)
            .field("warnings", &self.warnings)
            .finish()
    }
}

impl EnvironmentImport {
    /// Whether every secret variable has a value in every environment after the import,
    /// from the secret store's `stored` values or from the files.
    pub fn secrets_complete(&self, stored: &BTreeMap<(Option<String>, String), String>) -> bool {
        let has = |env: Option<&str>, name: &str| {
            let key = (env.map(str::to_owned), name.to_owned());
            [stored, &self.secret_values]
                .iter()
                .any(|m| m.get(&key).is_some_and(|v| !v.is_empty()))
        };
        self.manifest.secrets.iter().all(|name| {
            has(None, name)
                || self
                    .environments
                    .iter()
                    .all(|e| has(Some(e.id.as_str()), name))
        })
    }
}

/// Imports `files` into a project with `manifest` and `environments` (R-COL-20,
/// R-COL-29, D49).
pub fn import_environments(
    manifest: &Manifest,
    environments: &[Environment],
    files: &[ImportedEnvironment],
) -> EnvironmentImport {
    let mut out = EnvironmentImport {
        manifest: manifest.clone(),
        environments: environments.to_vec(),
        added: Vec::new(),
        updated: Vec::new(),
        secret_values: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for file in files {
        let index = if let Some(i) = out.environments.iter().position(|e| e.name == file.name) {
            if !out.updated.contains(&file.name) && !out.added.contains(&file.name) {
                out.updated.push(file.name.clone());
            }
            i
        } else {
            let seq = out.environments.iter().filter_map(|e| e.seq).max();
            out.environments.push(Environment {
                id: Collection::new_environment_id(&file.name, &out.environments),
                name: file.name.clone(),
                seq: Some(seq.unwrap_or(0) + 1),
                ..Environment::default()
            });
            out.added.push(file.name.clone());
            out.environments.len() - 1
        };
        for value in &file.values {
            out.apply(index, value);
        }
        for warning in &file.warnings {
            if !out.warnings.contains(warning) {
                out.warnings.push(warning.clone());
            }
        }
    }
    out.manifest.secrets.sort();
    out
}

impl EnvironmentImport {
    /// Puts one value into the environment at `index`.
    fn apply(&mut self, index: usize, value: &EnvValue) {
        let EnvValue { key, value, secret } = value;
        let server = self
            .manifest
            .servers
            .iter()
            .find(|s| s.name == *key || s.id == *key)
            .map(|s| s.id.clone());
        let env = &mut self.environments[index];
        if let Some(server) = server {
            env.servers.insert(server, base_url(value));
            return;
        }
        if !(*secret || self.manifest.secrets.contains(key)) {
            env.variables.insert(key.clone(), value.clone());
            return;
        }
        // A secret: no value of it may stay in the files (D35).
        env.variables.remove(key);
        if !self.manifest.secrets.contains(key) {
            self.manifest.secrets.push(key.clone());
        }
        if let Some(project_value) = self.manifest.variables.remove(key)
            && !project_value.is_empty()
        {
            self.secret_values
                .entry((None, key.clone()))
                .or_insert(project_value);
        }
        if !value.is_empty() {
            self.secret_values
                .insert((Some(env.id.clone()), key.clone()), value.clone());
        }
    }
}

impl Import {
    /// This import with environments from files (R-COL-20, R-COL-29): they replace the
    /// "Default" environment, and a server none of them sets keeps the base URL the
    /// imported file gave it.
    #[must_use]
    pub fn with_environments(&self, files: &[ImportedEnvironment]) -> Import {
        let mut import = self.clone();
        if files.is_empty() {
            return import;
        }
        let imported = import_environments(&self.manifest, &[], files);
        let collection_urls: BTreeMap<String, String> = self
            .environments
            .iter()
            .flat_map(|e| e.servers.clone())
            .filter(|(_, url)| !url.is_empty())
            .collect();
        import.environments = imported.environments;
        for env in &mut import.environments {
            for (server, url) in &collection_urls {
                env.servers.entry(server.clone()).or_insert(url.clone());
            }
        }
        for server in &imported.manifest.servers {
            let set = import
                .environments
                .iter()
                .all(|e| e.servers.get(&server.id).is_some_and(|u| !u.is_empty()));
            if set {
                let warning = no_base_url(&server.name);
                import.warnings.retain(|w| *w != warning);
            }
        }
        for warning in imported.warnings {
            if !import.warnings.contains(&warning) {
                import.warnings.push(warning);
            }
        }
        import.manifest = imported.manifest;
        for ((env, name), value) in imported.secret_values {
            match env {
                None => {
                    import.secret_values.entry(name).or_insert(value);
                }
                Some(env) => {
                    import.environment_secret_values.insert((env, name), value);
                }
            }
        }
        import
    }
}
