//! What the environments dialog edits (R-WS-9): the workspace's list with its shared
//! values, and each project's servers, variables and values for those environments.

use std::collections::{BTreeMap, BTreeSet};

use tortuga_core::format::names_match;
use tortuga_core::secrets::SecretValues;
use tortuga_core::workspace::WorkspaceEnvironment;
use tortuga_core::{Collection, Environment, EnvironmentColor, Manifest, Server};

use crate::environments::SecretsByEnvironment;

/// Editable copy of a collection's servers, variables and environments.
/// Rows and columns are kept as parallel vectors so names can be edited
/// freely; ids are only assigned when saving (R-SRV-1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub environments: Vec<EnvColumn>,
    pub servers: Vec<ServerRow>,
    pub variables: Vec<VarRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvColumn {
    /// `None` until saved for the first time.
    pub id: Option<String>,
    pub name: String,
    /// Its dot's colour (R-SRV-8); `None`: the default.
    pub color: Option<EnvironmentColor>,
    /// Ask before writes are sent to it (R-SRV-9).
    pub protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerRow {
    pub id: Option<String>,
    pub name: String,
    /// One base URL per environment column.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarRow {
    pub name: String,
    /// Values stay on this machine instead of the collection files (R-VAR-3).
    pub secret: bool,
    pub default: String,
    /// One value per environment column; empty = use the default (R-VAR-7).
    pub values: Vec<String>,
}

/// What saving the environments dialog writes: files, and local secret values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedConfig {
    pub manifest: Manifest,
    pub environments: Vec<Environment>,
    pub secrets: SecretValues,
}

impl Draft {
    pub fn from_collection(c: &Collection, secrets: &SecretValues) -> Self {
        let envs = &c.environments;
        let mut var_names: BTreeSet<&String> = c.manifest.variables.keys().collect();
        var_names.extend(&c.manifest.secrets);
        for e in envs {
            var_names.extend(e.variables.keys());
        }
        let is_secret = |name: &str| c.manifest.secrets.iter().any(|s| s == name);
        Self {
            environments: envs
                .iter()
                .map(|e| EnvColumn {
                    id: Some(e.id.clone()),
                    name: e.name.clone(),
                    color: e.color,
                    protected: e.protected,
                })
                .collect(),
            servers: c
                .manifest
                .servers
                .iter()
                .map(|s| ServerRow {
                    id: Some(s.id.clone()),
                    name: s.name.clone(),
                    urls: envs
                        .iter()
                        .map(|e| e.servers.get(&s.id).cloned().unwrap_or_default())
                        .collect(),
                })
                .collect(),
            variables: var_names
                .into_iter()
                .map(|name| {
                    if is_secret(name) {
                        let local = |env: Option<&str>| {
                            secrets.get(env, name).unwrap_or_default().to_owned()
                        };
                        VarRow {
                            name: name.clone(),
                            secret: true,
                            default: local(None),
                            values: envs.iter().map(|e| local(Some(&e.id))).collect(),
                        }
                    } else {
                        VarRow {
                            name: name.clone(),
                            secret: false,
                            default: c.manifest.variables.get(name).cloned().unwrap_or_default(),
                            values: envs
                                .iter()
                                .map(|e| e.variables.get(name).cloned().unwrap_or_default())
                                .collect(),
                        }
                    }
                })
                .collect(),
        }
    }

    pub fn add_environment(&mut self) {
        let name = format!("Environment {}", self.environments.len() + 1);
        self.environments.push(EnvColumn {
            id: None,
            name,
            color: None,
            protected: false,
        });
        self.servers
            .iter_mut()
            .for_each(|s| s.urls.push(String::new()));
        self.variables
            .iter_mut()
            .for_each(|v| v.values.push(String::new()));
    }

    pub fn remove_environment(&mut self, col: usize) {
        self.environments.remove(col);
        self.servers
            .iter_mut()
            .for_each(|s| drop(s.urls.remove(col)));
        self.variables
            .iter_mut()
            .for_each(|v| drop(v.values.remove(col)));
    }

    /// Moves environment column `from` to position `to`, with its URLs and values (R-SRV-6).
    pub fn move_environment(&mut self, from: usize, to: usize) {
        if from == to || from >= self.environments.len() || to >= self.environments.len() {
            return;
        }
        move_item(&mut self.environments, from, to);
        for s in &mut self.servers {
            move_item(&mut s.urls, from, to);
        }
        for v in &mut self.variables {
            move_item(&mut v.values, from, to);
        }
    }

    /// Moves server row `from` to position `to` (R-SRV-6).
    pub fn move_server(&mut self, from: usize, to: usize) {
        if from < self.servers.len() && to < self.servers.len() {
            move_item(&mut self.servers, from, to);
        }
    }

    pub fn add_server(&mut self, name: String) {
        let urls = vec![String::new(); self.environments.len()];
        self.servers.push(ServerRow {
            id: None,
            name,
            urls,
        });
    }

    pub fn add_variable(&mut self, name: String) {
        let values = vec![String::new(); self.environments.len()];
        self.variables.push(VarRow {
            name,
            secret: false,
            default: String::new(),
            values,
        });
    }

    /// Checks the draft and converts it to what is written to disk, plus the
    /// secret values that stay on this machine.
    pub fn to_config(
        &self,
        base: &Manifest,
        old_envs: &[Environment],
    ) -> Result<SavedConfig, String> {
        let mut manifest = base.clone();
        let mut seen = BTreeSet::new();
        for e in &self.environments {
            if e.name.trim().is_empty() {
                return Err("Every environment needs a name.".into());
            }
            if !seen.insert(e.name.trim().to_lowercase()) {
                return Err(format!(
                    "There are two environments called “{}”.",
                    e.name.trim()
                ));
            }
        }

        manifest.servers = Vec::new();
        let mut server_ids = Vec::new();
        for s in self.servers.iter().filter(|s| !s.name.trim().is_empty()) {
            let id =
                s.id.clone()
                    .unwrap_or_else(|| Collection::new_server_id(&s.name, &manifest.servers));
            manifest.servers.push(Server {
                id: id.clone(),
                name: s.name.trim().to_owned(),
            });
            server_ids.push((id, s));
        }

        let mut var_seen = BTreeSet::new();
        let vars: Vec<&VarRow> = self
            .variables
            .iter()
            .filter(|v| !v.name.trim().is_empty())
            .collect();
        for v in &vars {
            let name = v.name.trim();
            if name.contains(['{', '}']) {
                return Err(format!("Variable names cannot contain braces: “{name}”."));
            }
            if !var_seen.insert(name) {
                return Err(format!("There are two variables called “{name}”."));
            }
        }
        let (secret_vars, vars): (Vec<&VarRow>, Vec<&VarRow>) =
            vars.into_iter().partition(|v| v.secret);
        manifest.variables = vars
            .iter()
            .filter(|v| !v.default.is_empty())
            .map(|v| (v.name.trim().to_owned(), v.default.clone()))
            .collect();
        manifest.secrets = secret_vars
            .iter()
            .map(|v| v.name.trim().to_owned())
            .collect();
        manifest.secrets.sort();
        let mut secrets = SecretValues::default();
        for v in &secret_vars {
            if !v.default.is_empty() {
                secrets
                    .values
                    .insert((None, v.name.trim().to_owned()), v.default.clone());
            }
        }

        let mut envs: Vec<Environment> = Vec::new();
        for (col, (seq, e)) in (1..).zip(&self.environments).enumerate() {
            let id = e.id.clone().unwrap_or_else(|| {
                let taken: Vec<Environment> = old_envs.iter().chain(&envs).cloned().collect();
                Collection::new_environment_id(&e.name, &taken)
            });
            let servers: BTreeMap<String, String> = server_ids
                .iter()
                .filter(|(_, s)| !s.urls[col].trim().is_empty())
                .map(|(id, s)| (id.clone(), s.urls[col].trim().to_owned()))
                .collect();
            let variables = vars
                .iter()
                .filter(|v| !v.values[col].is_empty())
                .map(|v| (v.name.trim().to_owned(), v.values[col].clone()))
                .collect();
            for v in secret_vars.iter().filter(|v| !v.values[col].is_empty()) {
                secrets.values.insert(
                    (Some(id.clone()), v.name.trim().to_owned()),
                    v.values[col].clone(),
                );
            }
            envs.push(Environment {
                id,
                name: e.name.trim().to_owned(),
                seq: Some(seq),
                color: e.color,
                protected: e.protected,
                servers,
                variables,
            });
        }
        Ok(SavedConfig {
            manifest,
            environments: envs,
            secrets,
        })
    }
}

pub(super) fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) {
    let item = list.remove(from);
    list.insert(to, item);
}

impl Draft {
    /// The draft with one column per name in `names`, in that order: the project's file
    /// of that name, or a new empty one (R-WS-9).
    #[must_use]
    pub fn aligned(mut self, names: &[String]) -> Self {
        let mut out = Self {
            servers: self.servers.clone(),
            variables: self.variables.clone(),
            ..Self::default()
        };
        out.servers.iter_mut().for_each(|s| s.urls.clear());
        out.variables.iter_mut().for_each(|v| v.values.clear());
        for name in names {
            let found = self
                .environments
                .iter()
                .position(|e| names_match(&e.name, name));
            if let Some(col) = found {
                out.environments.push(self.environments[col].clone());
                for (to, from) in out.servers.iter_mut().zip(&self.servers) {
                    to.urls.push(from.urls[col].clone());
                }
                for (to, from) in out.variables.iter_mut().zip(&self.variables) {
                    to.values.push(from.values[col].clone());
                }
                // Taken, so a second name matching it gets a new column.
                self.environments[col].name.clear();
            } else {
                out.add_environment();
                if let Some(e) = out.environments.last_mut() {
                    e.name.clone_from(name);
                }
            }
        }
        out
    }

    /// As [`Draft::to_config`], without files for environments the project had none for
    /// and still sets nothing in: the workspace's values serve it (R-WS-9).
    pub fn to_project_config(
        &self,
        base: &Manifest,
        old_envs: &[Environment],
    ) -> Result<SavedConfig, String> {
        let mut saved = self.to_config(base, old_envs)?;
        let new_ids: Vec<String> = saved
            .environments
            .iter()
            .zip(&self.environments)
            .filter(|(_, col)| col.id.is_none())
            .map(|(e, _)| e.id.clone())
            .collect();
        let secrets = &saved.secrets;
        saved.environments.retain(|e| {
            let has_secrets = secrets
                .values
                .keys()
                .any(|(env, _)| env.as_deref() == Some(e.id.as_str()));
            !new_ids.contains(&e.id)
                || !e.servers.is_empty()
                || !e.variables.is_empty()
                || has_secrets
        });
        Ok(saved)
    }
}

/// One environment as edited.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvDraft {
    /// `None` until saved for the first time.
    pub id: Option<String>,
    pub name: String,
    pub color: Option<EnvironmentColor>,
    pub protected: bool,
    /// (server name, base URL).
    pub servers: Vec<(String, String)>,
    pub variables: Vec<VarDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarDraft {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

impl EnvDraft {
    pub fn new(env: &WorkspaceEnvironment, secrets: Option<&BTreeMap<String, String>>) -> Self {
        let mut variables: Vec<VarDraft> = env
            .variables
            .iter()
            .map(|(name, value)| VarDraft {
                name: name.clone(),
                value: value.clone(),
                secret: false,
            })
            .chain(env.secrets.iter().map(|name| {
                VarDraft {
                    name: name.clone(),
                    value: secrets
                        .and_then(|s| s.get(name))
                        .cloned()
                        .unwrap_or_default(),
                    secret: true,
                }
            }))
            .collect();
        variables.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            id: Some(env.id.clone()),
            name: env.name.clone(),
            color: env.color,
            protected: env.protected,
            servers: env
                .servers
                .iter()
                .map(|(n, u)| (n.clone(), u.clone()))
                .collect(),
            variables,
        }
    }
}

/// The environments to save, and this machine's secret values for them; or why they
/// can't be saved.
pub fn to_environments(
    drafts: &[EnvDraft],
) -> Result<(Vec<WorkspaceEnvironment>, SecretsByEnvironment), String> {
    let mut out: Vec<WorkspaceEnvironment> = Vec::new();
    let mut secrets = BTreeMap::new();
    for d in drafts {
        let name = d.name.trim();
        if name.is_empty() {
            return Err("Every environment needs a name.".into());
        }
        if out.iter().any(|e| e.is_named(name)) {
            return Err(format!("There are two environments called “{name}”."));
        }
        let mut env = WorkspaceEnvironment {
            id: d
                .id
                .clone()
                .unwrap_or_else(|| WorkspaceEnvironment::new_id(name, &out)),
            name: name.to_owned(),
            color: d.color,
            protected: d.protected,
            ..WorkspaceEnvironment::default()
        };
        for (server, url) in &d.servers {
            let server = server.trim();
            if server.is_empty() || url.trim().is_empty() {
                continue;
            }
            if env.servers.keys().any(|s| names_match(s, server)) {
                return Err(format!("“{name}” names the server “{server}” twice."));
            }
            env.servers.insert(server.to_owned(), url.trim().to_owned());
        }
        let mut values = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for v in d.variables.iter().filter(|v| !v.name.trim().is_empty()) {
            let var = v.name.trim();
            if var.contains(['{', '}']) {
                return Err(format!("Variable names cannot contain braces: “{var}”."));
            }
            if !seen.insert(var) {
                return Err(format!("“{name}” has two variables called “{var}”."));
            }
            if v.secret {
                env.secrets.insert(var.to_owned());
                values.insert(var.to_owned(), v.value.clone());
            } else {
                env.variables.insert(var.to_owned(), v.value.clone());
            }
        }
        secrets.insert(env.id.clone(), values);
        out.push(env);
    }
    Ok((out, secrets))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn draft() -> Draft {
        let mut d = Draft::default();
        d.add_environment();
        d.environments[0].name = "Development".into();
        d.add_environment();
        d.environments[1].name = "Staging".into();
        d.add_server("Users API".into());
        d.servers[0].urls = vec!["http://localhost:8080".into(), String::new()];
        d.add_variable("token".into());
        d.variables[0].default = "none".into();
        d.variables[0].values = vec!["dev-token".into(), String::new()];
        d
    }

    #[test]
    fn draft_converts_to_files_with_generated_ids() {
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
        assert_eq!(
            manifest.servers,
            [Server {
                id: "users-api".into(),
                name: "Users API".into()
            }]
        );
        assert_eq!(
            manifest.variables,
            BTreeMap::from([("token".into(), "none".into())])
        );
        assert_eq!(
            envs.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["development", "staging"]
        );
        assert_eq!(envs[0].servers["users-api"], "http://localhost:8080");
        assert!(envs[1].servers.is_empty(), "empty cells are not written");
        assert_eq!(envs[0].variables["token"], "dev-token");
        assert!(
            envs[1].variables.is_empty(),
            "empty cell falls back to the default"
        );
    }

    #[test]
    fn existing_ids_survive_renames() {
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
        let col = Collection {
            root: PathBuf::new(),
            manifest,
            read_only: false,
            items: vec![],
            environments: envs,
            broken_environments: vec![],
            workflows: vec![],
            broken_workflows: vec![],
        };
        let mut d = Draft::from_collection(&col, &SecretValues::default());
        d.servers[0].name = "People API".into();
        d.environments[0].name = "Local".into();
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = d
            .to_config(&col.manifest, &col.environments)
            .expect("valid");
        assert_eq!(manifest.servers[0].id, "users-api");
        assert_eq!(
            (envs[0].id.as_str(), envs[0].name.as_str()),
            ("development", "Local")
        );
    }

    #[test]
    fn rejects_duplicates_and_blank_environment_names() {
        let mut d = draft();
        d.add_variable("token".into());
        assert!(
            d.to_config(&Manifest::new("C"), &[])
                .expect_err("dup")
                .contains("two variables")
        );
        let mut d = draft();
        d.environments[1].name = " ".into();
        assert!(d.to_config(&Manifest::new("C"), &[]).is_err());
    }

    #[test]
    fn secret_values_go_to_the_local_store_not_the_files() {
        let mut d = draft();
        d.variables[0].secret = true;
        let saved = d.to_config(&Manifest::new("C"), &[]).expect("valid");
        assert_eq!(saved.manifest.secrets, ["token"]);
        assert!(saved.manifest.variables.is_empty(), "default stays local");
        assert!(saved.environments.iter().all(|e| e.variables.is_empty()));
        assert_eq!(saved.secrets.get(None, "token"), Some("none"));
        assert_eq!(
            saved.secrets.get(Some("development"), "token"),
            Some("dev-token")
        );

        // Reading back shows the local values in the same cells.
        let col = Collection {
            root: PathBuf::new(),
            manifest: saved.manifest,
            read_only: false,
            items: vec![],
            environments: saved.environments,
            broken_environments: vec![],
            workflows: vec![],
            broken_workflows: vec![],
        };
        let back = Draft::from_collection(&col, &saved.secrets);
        assert!(back.variables[0].secret);
        assert_eq!(back.variables[0].values, ["dev-token", ""]);
    }

    #[test]
    fn removing_an_environment_removes_its_column() {
        let mut d = draft();
        d.remove_environment(0);
        assert_eq!(d.servers[0].urls, [""]);
        assert_eq!(d.variables[0].values, [""]);
    }

    /// R-SRV-6: moving an environment moves its URLs and values; the order is saved.
    #[test]
    fn environments_and_servers_can_be_reordered() {
        let mut d = draft();
        d.servers[0].urls = vec!["http://dev".into(), "http://prod".into()];
        d.add_server("Auth".into());
        d.move_environment(1, 0);
        assert_eq!(d.servers[0].urls, ["http://prod", "http://dev"]);
        d.move_server(1, 0);
        assert_eq!(d.servers[0].name, "Auth");
        assert_eq!(d.variables[0].values, ["", "dev-token"], "values move too");
        let saved = d.to_config(&Manifest::new("C"), &[]).expect("valid");
        let names: Vec<&str> = saved.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Staging", "Development"]);
        assert_eq!(saved.environments[0].seq, Some(1));
        assert_eq!(saved.manifest.servers[0].name, "Auth");
        d.move_environment(0, 9);
        d.move_server(5, 0);
    }

    /// R-WS-9: drafts become environments; secret values go apart, by environment.
    #[test]
    fn drafts_become_environments_and_secret_values() {
        let drafts = vec![EnvDraft {
            id: None,
            name: " Staging ü😀 ".into(),
            color: Some(EnvironmentColor::Red),
            protected: true,
            servers: vec![
                ("Users".into(), " https://users.stg ".into()),
                ("Orders".into(), String::new()),
            ],
            variables: vec![
                VarDraft {
                    name: "tenant".into(),
                    value: "acme".into(),
                    secret: false,
                },
                VarDraft {
                    name: "token".into(),
                    value: "s3cret".into(),
                    secret: true,
                },
            ],
        }];
        let (envs, secrets) = to_environments(&drafts).expect("valid");
        assert_eq!(envs[0].name, "Staging ü😀");
        assert_ne!(envs[0].id, "");
        assert_eq!(
            envs[0].servers.len(),
            1,
            "a server without a URL is left out"
        );
        assert_eq!(envs[0].servers["Users"], "https://users.stg");
        assert_eq!(envs[0].variables["tenant"], "acme");
        assert!(
            !envs[0].variables.contains_key("token"),
            "never in the file"
        );
        assert_eq!(secrets[&envs[0].id]["token"], "s3cret");
        assert_eq!(
            EnvDraft::new(&envs[0], secrets.get(&envs[0].id)).variables,
            drafts[0].variables
        );

        let mut twice = drafts.clone();
        twice.push(EnvDraft {
            name: "staging ü😀".into(),
            ..EnvDraft::default()
        });
        assert!(
            to_environments(&twice).is_err(),
            "names match whatever their case"
        );
        let blank = vec![EnvDraft::default()];
        assert!(to_environments(&blank).is_err());
        let mut braces = drafts;
        braces[0].variables[0].name = "{x}".into();
        assert!(to_environments(&braces).is_err());
    }

    /// R-WS-9: a project's columns follow the workspace's list, matched by name.
    #[test]
    fn columns_follow_the_workspace_list() {
        let d = draft().aligned(&["Prod ü😀".into(), "staging".into(), "Development".into()]);
        let names: Vec<&str> = d.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Prod ü😀", "Staging", "Development"]);
        assert_eq!(d.servers[0].urls, ["", "", "http://localhost:8080"]);
        assert_eq!(d.variables[0].values, ["", "", "dev-token"]);

        // Only environments with something in them get a file.
        let saved = d
            .to_project_config(&Manifest::new("C"), &[])
            .expect("valid");
        let names: Vec<&str> = saved.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Development"]);
    }
}
