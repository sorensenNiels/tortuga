//! The workspace's environments (R-WS-9, D62): one list, kept by the workspace, that
//! every project uses. A project only fills in values for them, in its own environment
//! files; the workspace's values fill in what it leaves empty. Names match whatever their
//! case. An environment a project brings that the list lacks is added to it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tortuga_core::workspace::{self, WorkspaceEnvironment};
use tortuga_core::{Environment, EnvironmentColor};

use crate::components::SavedConfig;
use crate::state::AppState;

/// This machine's secret values of the workspace's environments, by environment id, then
/// name.
pub type SecretsByEnvironment = BTreeMap<String, BTreeMap<String, String>>;

impl AppState {
    /// The workspace's environment called `name` (R-WS-9).
    pub fn workspace_environment(&self, name: &str) -> Option<&WorkspaceEnvironment> {
        self.workspace()
            .environments
            .iter()
            .find(|e| e.is_named(name))
    }

    /// The environment every project uses now: the one chosen, else the first.
    pub fn chosen_environment(&self) -> Option<&WorkspaceEnvironment> {
        let ws = self.workspace();
        ws.environment
            .as_deref()
            .and_then(|name| self.workspace_environment(name))
            .or_else(|| ws.environments.first())
    }

    /// The name of the environment every project uses now.
    pub fn chosen_environment_name(&self) -> Option<String> {
        self.chosen_environment().map(|e| e.name.clone())
    }

    /// The id of the project's own file for the environment every project uses now, if
    /// it has one: its secret values go by it.
    pub fn active_environment(&self, root: &Path) -> Option<String> {
        let name = &self.chosen_environment()?.name;
        let c = self.collection(root)?;
        c.environments
            .iter()
            .find(|e| e.is_named(name))
            .map(|e| e.id.clone())
    }

    /// The environment requests of the project at `root` are sent with now.
    pub fn effective_environment(&self, root: &Path) -> Option<Environment> {
        let name = self.chosen_environment()?.name.clone();
        self.environment_named(root, &name)
    }

    /// [`Self::effective_environment`] without this machine's secret values of the
    /// workspace's environment, for what the app gives away, such as mock answers (P6).
    pub fn effective_environment_without_secrets(&self, root: &Path) -> Option<Environment> {
        let shared = self.chosen_environment()?;
        let c = self.collection(root)?;
        let own = c.environments.iter().find(|e| e.is_named(&shared.name));
        workspace::effective_environment(Some(shared), &BTreeMap::new(), own, &c.manifest)
    }

    /// The environment called `name` as the project's requests use it: the workspace's,
    /// with its server URLs and this machine's secret values, and the project's own
    /// values over it.
    pub fn environment_named(&self, root: &Path, name: &str) -> Option<Environment> {
        let c = self.collection(root)?;
        let shared = self.workspace_environment(name);
        let no_secrets = BTreeMap::new();
        let secrets = shared
            .and_then(|e| self.workspace_env_secrets.get(&e.id))
            .unwrap_or(&no_secrets);
        let own = c.environments.iter().find(|e| e.is_named(name));
        workspace::effective_environment(shared, secrets, own, &c.manifest)
    }

    /// Every environment, as the project at `root` uses it (R-SRV-10).
    pub fn environments_of(&self, root: &Path) -> Vec<Environment> {
        self.environment_names()
            .iter()
            .filter_map(|name| self.environment_named(root, name))
            .collect()
    }

    /// The name of the protected environment requests of the project at `root` go to
    /// now, if they go to one (R-SRV-9). The mock's never is.
    pub fn protected_environment(&self, root: &Path) -> Option<String> {
        if self.mock_environment(root).is_some() {
            return None;
        }
        let env = self.effective_environment(root)?;
        env.protected.then_some(env.name)
    }

    /// The names of the workspace's environments, in its order.
    pub fn environment_names(&self) -> Vec<String> {
        self.workspace()
            .environments
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    /// The colour of the environment called `name` (R-SRV-8).
    pub fn environment_color(&self, name: &str) -> Option<EnvironmentColor> {
        self.workspace_environment(name).and_then(|e| e.color)
    }

    /// Switches every project to the environment called `name` (R-WS-6).
    pub fn set_workspace_environment(&mut self, name: &str) {
        self.mocks.selected = false;
        self.refresh_mocks();
        self.workspace_mut().environment = Some(name.to_owned());
        self.save_local();
    }

    /// Adds the environments open projects have that the workspace doesn't list yet,
    /// with their colour and protection: a project opened for the first time, or a
    /// teammate's new environment pulled with git (R-WS-9).
    pub(crate) fn adopt_environments(&mut self) {
        let mut list = self.workspace().environments.clone();
        let before = list.len();
        for env in self.collections.iter().flat_map(|c| &c.environments) {
            if list.iter().any(|e| e.is_named(&env.name)) {
                continue;
            }
            list.push(WorkspaceEnvironment {
                id: WorkspaceEnvironment::new_id(&env.name, &list),
                name: env.name.clone(),
                color: env.color,
                protected: env.protected,
                ..WorkspaceEnvironment::default()
            });
        }
        if list.len() != before {
            self.workspace_mut().environments = list;
            self.workspace_changed();
        }
    }

    /// Saves what the environments dialog edited (R-WS-9): the projects' files and
    /// secret values first, then the workspace's list, shared values and secret values.
    /// The workspace stays on its environment when it was renamed.
    pub fn save_environments(
        &mut self,
        list: Vec<WorkspaceEnvironment>,
        secrets: &SecretsByEnvironment,
        projects: Vec<(PathBuf, SavedConfig)>,
    ) -> Result<(), String> {
        let chosen_id = self.chosen_environment().map(|e| e.id.clone());
        for (root, saved) in projects {
            self.save_secrets(&root, saved.secrets)?;
            self.run(&root, |c, s| {
                c.save_config(s, &saved.manifest, &saved.environments)
            });
        }
        let renamed = chosen_id
            .and_then(|id| list.iter().find(|e| e.id == id))
            .map(|e| e.name.clone());
        self.save_workspace_environments(list, secrets)?;
        if let Some(name) = renamed {
            self.workspace_mut().environment = Some(name);
            self.save_local();
        }
        Ok(())
    }

    /// Saves the workspace's environments to local data and the workspace file, and their
    /// secret values (`secret_values`, by environment id, then name) to this machine's
    /// secret store. Values no environment keeps as secret any more are deleted.
    pub fn save_workspace_environments(
        &mut self,
        environments: Vec<WorkspaceEnvironment>,
        secret_values: &SecretsByEnvironment,
    ) -> Result<(), String> {
        let id = self.workspace().id.clone();
        let store = self.host.secrets();
        let keeps = |env_id: &str, name: &str| {
            environments
                .iter()
                .any(|e| e.id == env_id && e.secrets.contains(name))
        };
        for old in &self.workspace().environments {
            for name in old.secrets.iter().filter(|n| !keeps(&old.id, n)) {
                store
                    .delete(&workspace::environment_secret_key(&id, &old.id, name))
                    .map_err(|e| e.to_string())?;
            }
        }
        for env in &environments {
            for name in &env.secrets {
                let key = workspace::environment_secret_key(&id, &env.id, name);
                let value = secret_values
                    .get(&env.id)
                    .and_then(|values| values.get(name))
                    .filter(|v| !v.is_empty());
                match value {
                    Some(value) => store.set(&key, value),
                    None => store.delete(&key),
                }
                .map_err(|e| e.to_string())?;
            }
        }
        self.workspace_mut().environments = environments;
        self.workspace_changed();
        self.load_workspace_secrets();
        self.refresh_mocks();
        Ok(())
    }
}

/// Whether two lists name the same environments, whatever their case.
#[cfg(test)]
fn same_names(a: &[String], b: &[&str]) -> bool {
    use tortuga_core::format::names_match;
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| names_match(x, y))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use tortuga_core::workspace::WorkspaceEnvironment;
    use tortuga_core::{Environment, EnvironmentColor, Server};

    use super::same_names;
    use crate::state::AppState;
    use crate::state::tests::{project, temp};

    fn open(roots: &[&Path]) -> AppState {
        AppState::new(Box::new(crate::test_host::TestHost {
            open: roots.iter().map(|r| r.to_path_buf()).collect(),
            ..Default::default()
        }))
    }

    /// Gives the project at `root` a server "Users" and these environments.
    fn configure(state: &mut AppState, root: &Path, envs: &[Environment]) {
        let mut manifest = state.collection(root).expect("open").manifest.clone();
        manifest.servers = vec![Server {
            id: "users".into(),
            name: "Users".into(),
        }];
        state
            .run(root, |c, s| c.save_config(s, &manifest, envs))
            .expect("saved");
    }

    /// R-WS-9: environments projects bring join the workspace's list once, whatever
    /// their case, with their colour and protection.
    #[test]
    fn projects_environments_join_the_list() {
        let dir = temp("ws-env-adopt");
        let (a, b) = (dir.join("a"), dir.join("b"));
        project(&a, "A", &["Dev", "Staging"]);
        project(&b, "B", &["dev", "Prod ü😀"]);
        let mut state = open(&[&a, &b]);
        assert!(same_names(
            &state.environment_names(),
            &["Dev", "Staging", "Prod ü😀"]
        ));
        assert_eq!(state.chosen_environment_name().as_deref(), Some("Dev"));

        let mut envs = state.collection(&b).expect("b").environments.clone();
        envs.push(Environment {
            id: "qa".into(),
            name: "QA".into(),
            color: Some(EnvironmentColor::Teal),
            protected: true,
            ..Environment::default()
        });
        state
            .run(&b, |c, s| c.save_config(s, &c.manifest, &envs))
            .expect("saved");
        let qa = state.workspace_environment("qa").expect("joined on reload");
        assert_eq!(
            (qa.color, qa.protected),
            (Some(EnvironmentColor::Teal), true)
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-9, D62: every project uses the chosen environment: the workspace's values,
    /// server URLs by name and secret values, with the project's own values over them.
    #[test]
    fn every_project_uses_the_chosen_environment() {
        let dir = temp("ws-envs");
        let (a, b) = (dir.join("a"), dir.join("b"));
        project(&a, "A", &[]);
        project(&b, "B", &[]);
        let mut state = open(&[&a, &b]);
        let own = Environment {
            id: "stg".into(),
            name: "STAGING".into(),
            variables: [("tenant".to_owned(), "b".to_owned())].into(),
            ..Environment::default()
        };
        configure(&mut state, &b, &[own]);
        let staging = WorkspaceEnvironment {
            id: "staging".into(),
            name: "Staging".into(),
            protected: true,
            servers: [("users".to_owned(), "https://users.stg".to_owned())].into(),
            variables: [("tenant".to_owned(), "acme ü😀".to_owned())].into(),
            secrets: ["token".to_owned()].into(),
            ..WorkspaceEnvironment::default()
        };
        let secrets = [(
            "staging".to_owned(),
            [("token".to_owned(), "s3cret".to_owned())].into(),
        )]
        .into();
        state
            .save_workspace_environments(vec![staging], &secrets)
            .expect("saved");
        state.set_workspace_environment("staging");

        assert_eq!(state.active_environment(&a), None, "A has no file for it");
        let a_env = state.effective_environment(&a).expect("A uses it");
        assert_eq!(a_env.variables["tenant"], "acme ü😀");
        assert_eq!(a_env.variables["token"], "s3cret");
        assert_eq!(state.active_environment(&b).as_deref(), Some("stg"));
        let ctx = state.context(Some(&b));
        assert_eq!(ctx.variables["tenant"], "b", "B's own wins");
        assert_eq!(ctx.variables["token"], "s3cret");
        // The preview, the hover box, curl copies and workflow results hide it (P6).
        assert!(state.workspace_secret_names().contains("token"));
        assert_eq!(ctx.masked().variables["token"], "<secret:token>");
        assert_eq!(
            state.context(Some(&a)).masked().variables["token"],
            "<secret:token>"
        );
        assert_eq!(
            ctx.server_urls["users"], "https://users.stg",
            "by server name"
        );
        assert_eq!(state.protected_environment(&a).as_deref(), Some("Staging"));

        // Removing it takes its secret values off this machine.
        state
            .save_workspace_environments(Vec::new(), &BTreeMap::new())
            .expect("saved");
        assert!(state.workspace_env_secrets.is_empty());
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-9: saving writes the projects' files, and renaming the chosen environment
    /// keeps the workspace on it.
    #[test]
    fn saving_keeps_the_workspace_on_a_renamed_environment() {
        let dir = temp("ws-env-save");
        let a = dir.join("a");
        project(&a, "A", &["Dev", "Staging"]);
        let mut state = open(&[&a]);
        state.set_workspace_environment("Staging");
        let mut list = state.workspace().environments.clone();
        list[1].name = "Pre-prod ü😀".into();
        let c = state.collection(&a).expect("a");
        let mut envs = c.environments.clone();
        envs[1].name = "Pre-prod ü😀".into();
        let saved = crate::components::SavedConfig {
            manifest: c.manifest.clone(),
            environments: envs,
            secrets: tortuga_core::secrets::SecretValues::default(),
        };
        state
            .save_environments(list, &BTreeMap::new(), vec![(a.clone(), saved)])
            .expect("saved");
        assert_eq!(
            state.chosen_environment_name().as_deref(),
            Some("Pre-prod ü😀")
        );
        assert!(same_names(
            &state.environment_names(),
            &["Dev", "Pre-prod ü😀"]
        ));
        assert_eq!(state.active_environment(&a).as_deref(), Some("staging"));
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
