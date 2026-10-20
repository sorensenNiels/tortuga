//! Importing Postman environment files into a project's environments (R-COL-20): into
//! the project a collection import is about to create, or into one that exists.
//!
//! A value whose name is a server's becomes that server's base URL in the environment
//! (D36); a secret one (Postman's "secret" type, or a name the project already keeps
//! secret) becomes a secret variable whose value goes to this machine's secret store
//! for that environment only (D35); the rest become environment variables. An
//! environment named like one the project has updates it (D49).

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

use crate::collection::Collection;
use crate::format::{Environment, Manifest};
use crate::import::{Import, base_url, no_base_url};
use crate::{Error, Result};

/// One value of a Postman environment.
#[derive(Clone, PartialEq, Eq)]
struct EnvValue {
    key: String,
    value: String,
    /// Postman's "secret" type.
    secret: bool,
}

/// A Postman environment file, read (R-COL-20).
#[derive(Clone, PartialEq, Eq)]
pub struct PostmanEnvironment {
    pub name: String,
    values: Vec<EnvValue>,
}

// Hand-written: the values may be secrets.
impl fmt::Debug for PostmanEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self.values.iter().map(|v| v.key.as_str()).collect();
        f.debug_struct("PostmanEnvironment")
            .field("name", &self.name)
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

/// Reads a Postman environment file, as exported from Postman or as its API returns it
/// (wrapped in `{"environment": …}`).
pub fn parse_environment(text: &str) -> Result<PostmanEnvironment> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let doc: Value =
        serde_json::from_str(text).map_err(|e| invalid(format!("it is not valid JSON: {e}")))?;
    let doc = unwrap(&doc);
    match scope(doc) {
        Some("globals") => {
            return Err(invalid(
                "this is a file of Postman's globals, not an environment. Export an \
                 environment from Postman's Environments, then import that",
            ));
        }
        _ if super::looks_like_collection(doc) => {
            return Err(invalid(
                "this is a Postman collection: import it with + Add → Import Postman \
                 collection…",
            ));
        }
        _ => {}
    }
    let Some(values) = doc.get("values").and_then(Value::as_array) else {
        return Err(invalid(
            "this is not a Postman environment: it has no `values`",
        ));
    };
    let name = doc
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Imported");
    let values = values
        .iter()
        .filter(|v| v.get("enabled").and_then(Value::as_bool) != Some(false))
        .filter_map(|v| {
            let key = v.get("key").and_then(Value::as_str)?.trim();
            (!key.is_empty()).then(|| EnvValue {
                key: key.to_owned(),
                value: super::text(v.get("value")),
                secret: v.get("type").and_then(Value::as_str) == Some("secret"),
            })
        })
        .collect();
    Ok(PostmanEnvironment {
        name: name.to_owned(),
        values,
    })
}

/// Whether `doc` is a Postman environment file.
pub(crate) fn looks_like_environment(doc: &Value) -> bool {
    let doc = unwrap(doc);
    scope(doc) == Some("environment")
        || (doc.get("values").is_some_and(Value::is_array)
            && doc.get("name").is_some()
            && doc.get("item").is_none())
}

fn unwrap(doc: &Value) -> &Value {
    match doc.get("environment") {
        Some(inner) if inner.get("values").is_some() => inner,
        _ => doc,
    }
}

fn scope(doc: &Value) -> Option<&str> {
    doc.get("_postman_variable_scope").and_then(Value::as_str)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidPostmanEnvironment(message.into())
}

/// A project's configuration with Postman environments imported into it.
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

/// Imports `files` into a project with `manifest` and `environments` (R-COL-20, D49).
pub fn import_environments(
    manifest: &Manifest,
    environments: &[Environment],
    files: &[PostmanEnvironment],
) -> EnvironmentImport {
    let mut out = EnvironmentImport {
        manifest: manifest.clone(),
        environments: environments.to_vec(),
        added: Vec::new(),
        updated: Vec::new(),
        secret_values: BTreeMap::new(),
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
    /// This collection import with Postman environments (R-COL-20): they replace the
    /// "Default" environment, and a server none of them sets keeps the collection's
    /// base URL.
    #[must_use]
    pub fn with_environments(&self, files: &[PostmanEnvironment]) -> Import {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Server;

    const STAGING: &str = r#"{
      "id": "5f0c", "name": "Staging",
      "values": [
        { "key": "baseUrl", "value": "https://staging.shop.test/", "type": "default", "enabled": true },
        { "key": "limit", "value": "10", "type": "default", "enabled": true },
        { "key": "token", "value": "st-ü😀", "type": "secret", "enabled": true },
        { "key": "apiKey", "value": "k1", "type": "default", "enabled": true },
        { "key": "off", "value": "x", "enabled": false },
        { "key": "", "value": "no key" },
        { "key": "retries", "value": 3 }
      ],
      "_postman_variable_scope": "environment",
      "_postman_exported_using": "Postman/11.2.0"
    }"#;

    fn manifest() -> Manifest {
        let mut manifest = Manifest::new("Shop");
        manifest.servers = vec![Server {
            id: "baseurl".into(),
            name: "baseUrl".into(),
        }];
        manifest.secrets = vec!["apiKey".into()];
        manifest
    }

    #[test]
    fn reads_an_environment_file() {
        let env = parse_environment(STAGING).expect("parse");
        assert_eq!(env.name, "Staging");
        let keys: Vec<_> = env.values.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys, ["baseUrl", "limit", "token", "apiKey", "retries"]);
        assert!(env.values[2].secret);
        assert_eq!(env.values[4].value, "3");
        assert!(!format!("{env:?}").contains("st-"), "Debug hides values");

        let wrapped = format!(r#"{{"environment": {STAGING}}}"#);
        assert_eq!(parse_environment(&wrapped).expect("wrapped"), env);
        let doc: Value = serde_json::from_str(STAGING).expect("json");
        assert!(looks_like_environment(&doc));
    }

    #[test]
    fn refuses_what_is_not_an_environment() {
        let message = |text: &str| crate::error::message(&parse_environment(text).expect_err(text));
        assert!(message("{").contains("not valid JSON"));
        assert!(message(r#"{"name": "x"}"#).contains("no `values`"));
        assert!(
            message(r#"{"name": "G", "values": [], "_postman_variable_scope": "globals"}"#)
                .contains("globals")
        );
        assert!(message(r#"{"info": {"name": "C"}, "item": []}"#).contains("collection"));
        // The collection entry recognises an environment file.
        let as_collection =
            crate::import::parse(STAGING, "staging.json", crate::import::ImportKind::Postman)
                .expect_err("not a collection");
        assert!(crate::error::message(&as_collection).contains("Add environment files"));
        let unnamed = parse_environment(r#"{"values": []}"#).expect("unnamed");
        assert_eq!(unnamed.name, "Imported");
    }

    /// R-COL-20: base URLs into servers, secrets into the secret store for that
    /// environment, the rest as environment variables; nothing secret in the files.
    #[test]
    fn maps_values_into_a_new_environment() {
        let staging = parse_environment(STAGING).expect("parse");
        let out = import_environments(&manifest(), &[], &[staging]);
        assert_eq!(out.added, ["Staging"]);
        assert_eq!(out.updated, Vec::<String>::new());
        let env = &out.environments[0];
        assert_eq!(env.id, "staging");
        assert_eq!(env.seq, Some(1));
        assert_eq!(env.servers["baseurl"], "https://staging.shop.test");
        assert_eq!(env.variables["limit"], "10");
        assert_eq!(env.variables["retries"], "3");
        assert!(!env.variables.contains_key("token"));
        assert!(!env.variables.contains_key("apiKey"), "already a secret");
        assert_eq!(out.manifest.secrets, ["apiKey", "token"]);
        let key = |name: &str| (Some("staging".to_owned()), name.to_owned());
        assert_eq!(out.secret_values[&key("token")], "st-ü😀");
        assert_eq!(out.secret_values[&key("apiKey")], "k1");
        let written =
            crate::format::write_environment(env) + &crate::format::write_manifest(&out.manifest);
        assert!(!written.contains("st-ü") && !written.contains("k1"));
        assert!(!format!("{out:?}").contains("st-ü"), "Debug hides values");
    }

    /// D49: an environment with the file's name is updated; what the file doesn't set
    /// stays. A project variable that a file marks secret moves to the secret store.
    #[test]
    fn updates_an_environment_with_the_same_name() {
        let mut manifest = manifest();
        manifest
            .variables
            .insert("token".into(), "project-token".into());
        let existing = Environment {
            id: "stage".into(),
            name: "Staging".into(),
            seq: Some(4),
            servers: [("baseurl".to_owned(), "https://old.test".to_owned())].into(),
            variables: [
                ("limit".to_owned(), "5".to_owned()),
                ("kept".to_owned(), "yes".to_owned()),
            ]
            .into(),
            ..Environment::default()
        };
        let staging = parse_environment(STAGING).expect("parse");
        let prod = parse_environment(
            r#"{"name": "Production", "values": [{"key": "limit", "value": "50"}]}"#,
        )
        .expect("parse");
        let out = import_environments(&manifest, &[existing], &[staging, prod]);
        assert_eq!(out.updated, ["Staging"]);
        assert_eq!(out.added, ["Production"]);
        let stage = &out.environments[0];
        assert_eq!(stage.id, "stage", "same file");
        assert_eq!(stage.servers["baseurl"], "https://staging.shop.test");
        assert_eq!(stage.variables["limit"], "10");
        assert_eq!(stage.variables["kept"], "yes");
        assert_eq!(out.environments[1].seq, Some(5));
        assert!(!out.manifest.variables.contains_key("token"));
        assert_eq!(
            out.secret_values[&(None, "token".to_owned())],
            "project-token"
        );

        assert!(
            !out.secrets_complete(&BTreeMap::new()),
            "Production has no token"
        );
        let mut stored = BTreeMap::new();
        stored.insert(
            (Some("production".to_owned()), "apiKey".to_owned()),
            "k2".to_owned(),
        );
        assert!(out.secrets_complete(&stored));
    }

    /// R-COL-20: a collection import with environment files gets them instead of
    /// "Default"; base URLs only the collection has are kept, and the warning about a
    /// base URL without a value goes once every environment sets it.
    #[test]
    fn collection_import_takes_the_environments() {
        let collection = r#"{"info": {"name": "Shop"},
            "variable": [{"key": "auth", "value": "https://auth.shop.test"}],
            "item": [
              {"name": "List", "request": {"method": "GET", "url": "{{baseUrl}}/orders"}},
              {"name": "Login", "request": {"method": "POST", "url": "{{auth}}/login"}}
            ]}"#;
        let import = crate::postman::parse(collection).expect("parse");
        assert!(import.warnings.contains(&no_base_url("baseUrl")));
        assert_eq!(import.with_environments(&[]), import);

        let staging = parse_environment(STAGING).expect("parse");
        let with = import.with_environments(&[staging]);
        let names: Vec<_> = with.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Staging"]);
        let env = &with.environments[0];
        assert_eq!(env.servers["baseurl"], "https://staging.shop.test");
        assert_eq!(env.servers["auth"], "https://auth.shop.test");
        assert!(!with.warnings.contains(&no_base_url("baseUrl")));
        assert_eq!(
            with.environment_secret_values[&("staging".to_owned(), "token".to_owned())],
            "st-ü😀"
        );
        assert!(with.manifest.secrets.contains(&"token".to_owned()));
    }
}
