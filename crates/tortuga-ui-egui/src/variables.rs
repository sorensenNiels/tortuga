//! Environments, runtime values and the variables a request is sent with: which
//! environment each project uses (R-SRV-4, R-WS-6), values captured or set by scripts
//! (R-TEST-5, R-WS-8), and the context a request resolves its variables in (R-VAR-1).

use std::collections::BTreeMap;
use std::path::Path;

use tortuga_core::vars::{Source, dynamic};
use tortuga_core::{Environment, Request, vars};

use crate::components::{VarHelp, VarInfo};
use crate::state::{AppState, ItemKey, root_key};

/// Longest value shown next to a suggested name.
const NOTE_BYTES: usize = 40;

impl AppState {
    /// What the request's fields can tell about its variables (R-VAR-5, R-VAR-9): the
    /// undefined ones, the value and source of each it uses (secrets masked, P6), and
    /// the names to suggest after `{{`.
    pub fn var_help(
        &self,
        key: Option<&ItemKey>,
        request: &Request,
        ctx: &vars::Context,
    ) -> VarHelp {
        let shown = |name: &str| -> Option<String> {
            if ctx.secrets.contains(name) {
                return Some("secret, hidden".to_owned());
            }
            ctx.variables.get(name).cloned()
        };
        let known = vars::names_in(request, ctx)
            .into_iter()
            .filter_map(|name| {
                let source = self.variable_source(key, &name)?;
                let value = match source {
                    Source::Dynamic => dynamic::describe(&name)?.to_owned(),
                    _ => shown(&name)?,
                };
                Some((
                    name,
                    VarInfo {
                        value,
                        from: Some(source.describe()),
                    },
                ))
            })
            .collect();
        let note = |name: &str| {
            let value = shown(name).unwrap_or_default();
            let (cut, more) = crate::format::truncate(&value, NOTE_BYTES);
            if more {
                format!("{cut}…")
            } else {
                cut.to_owned()
            }
        };
        let names = ctx
            .variables
            .keys()
            .map(|n| (n.clone(), note(n)))
            .chain(
                dynamic::SUGGESTED
                    .iter()
                    .map(|(n, what)| ((*n).to_owned(), (*what).to_owned())),
            )
            .collect();
        VarHelp {
            undefined: vars::undefined_in(request, ctx),
            known,
            names,
            open: std::cell::Cell::new(None),
        }
    }

    /// Where `name` gets its value for the request `key` (`None`: the scratch request),
    /// in the order its context is built (R-VAR-9): runtime values, this machine's
    /// secrets, the environment, folders and project, the workspace, dynamic variables.
    pub fn variable_source(&self, key: Option<&ItemKey>, name: &str) -> Option<Source> {
        let root = key.map(|k| k.root.as_path());
        let has =
            |map: Option<&BTreeMap<String, String>>| map.is_some_and(|m| m.contains_key(name));
        if has(self.runtime.get(&root.map(Path::to_path_buf))) {
            return Some(Source::Runtime);
        }
        if has(self.workspace_runtime.get(&self.workspace().id)) {
            return Some(Source::WorkspaceRuntime);
        }
        if let (Some(key), Some(c)) = (key, root.and_then(|r| self.collection(r))) {
            let env_id = self.active_environment(&key.root);
            let secret = self
                .secrets
                .get(&key.root)
                .map(|s| s.for_environment(env_id.as_deref()));
            if has(secret.as_ref()) {
                return Some(Source::Secret);
            }
            let mock = self.mock_environment(&key.root);
            let env = mock
                .as_ref()
                .or_else(|| env_id.as_deref().and_then(|id| c.environment(id)));
            if let Some(source) = vars::project_source(c, env, Some(&key.path), name) {
                return Some(source);
            }
        }
        if self.workspace_values().contains_key(name) {
            return Some(Source::Workspace);
        }
        dynamic::is_dynamic(name).then_some(Source::Dynamic)
    }

    /// The active environment of a project: the workspace's environment if the project
    /// has one with that name (R-WS-6), else the project's own choice, else its first.
    pub fn active_environment(&self, root: &Path) -> Option<String> {
        let c = self.collection(root)?;
        let ws = self.workspace();
        if let Some(name) = &ws.environment
            && let Some(env) = c.environments.iter().find(|e| e.name == *name)
        {
            return Some(env.id.clone());
        }
        match ws.project_environments.get(&root_key(root)) {
            Some(id) if c.environment(id).is_some() => Some(id.clone()),
            _ => c.environments.first().map(|e| e.id.clone()),
        }
    }

    /// The name of the protected environment requests of the project at `root` go to
    /// now, if they go to one (R-SRV-9). The mock's never is.
    pub fn protected_environment(&self, root: &Path) -> Option<String> {
        if self.mock_environment(root).is_some() {
            return None;
        }
        let id = self.active_environment(root)?;
        let env = self.collection(root)?.environment(&id)?;
        env.protected.then(|| env.name.clone())
    }

    /// False when the workspace's environment is one this project doesn't have (R-WS-6).
    pub fn follows_workspace_environment(&self, root: &Path) -> bool {
        let Some(name) = &self.workspace().environment else {
            return true;
        };
        self.collection(root).is_none_or(|c| {
            c.environments.is_empty() || c.environments.iter().any(|e| e.name == *name)
        })
    }

    /// Environment names of all open projects, each once, in order of appearance.
    pub fn environment_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for env in self.collections.iter().flat_map(|c| &c.environments) {
            if !names.contains(&env.name) {
                names.push(env.name.clone());
            }
        }
        names
    }

    /// The colour of the environment called `name` (R-SRV-8): the one `root` gives it,
    /// else the first project's that gives it one, since the picker switches them all.
    pub fn environment_color(
        &self,
        root: &Path,
        name: &str,
    ) -> Option<tortuga_core::EnvironmentColor> {
        let own = self
            .collection(root)
            .and_then(|c| c.environments.iter().find(|e| e.name == name))
            .and_then(|e| e.color);
        own.or_else(|| {
            self.collections
                .iter()
                .flat_map(|c| &c.environments)
                .filter(|e| e.name == name)
                .find_map(|e| e.color)
        })
    }

    /// Switches every project that has an environment called `name` (R-WS-6).
    pub fn set_workspace_environment(&mut self, name: &str) {
        self.mocks.selected = false;
        self.refresh_mocks();
        self.workspace_mut().environment = Some(name.to_owned());
        self.save_local();
    }

    /// Keeps the project at `root` on its environment when a save renamed it (R-SRV-7): the
    /// workspace picks environments by name (R-WS-6). `old` are its environments before.
    pub fn follow_environment_renames(&mut self, root: &Path, old: &[Environment]) {
        let (Some(chosen), Some(c)) = (self.workspace().environment.clone(), self.collection(root))
        else {
            return;
        };
        let Some(renamed) = old
            .iter()
            .filter(|e| e.name == chosen)
            .find_map(|e| c.environment(&e.id))
            .filter(|e| e.name != chosen)
        else {
            return;
        };
        let (id, name) = (renamed.id.clone(), renamed.name.clone());
        if self.environment_names().contains(&chosen) {
            // Other projects still have the old name and stay on it; only this one moves.
            self.set_project_environment(root, &id);
        } else {
            self.set_workspace_environment(&name);
        }
    }

    /// The environment a project uses when it has none named like the workspace's.
    pub fn set_project_environment(&mut self, root: &Path, id: &str) {
        self.workspace_mut()
            .project_environments
            .insert(root_key(root), id.to_owned());
        self.refresh_mock(root);
        self.save_local();
    }

    /// Servers and variables for requests of the collection at `root`; empty for scratch requests.
    pub fn context(&self, root: Option<&Path>) -> vars::Context {
        match root {
            Some(root) => self.context_at(root, None),
            None => self.with_runtime(vars::Context::default(), None),
        }
    }

    /// The context of a request: the project's, with what its folders define (R-COL-11).
    /// `None` is the scratch request.
    pub fn request_context(&self, key: Option<&ItemKey>) -> vars::Context {
        match key {
            Some(key) => self.context_at(&key.root, Some(&key.path)),
            None => self.with_runtime(vars::Context::default(), None),
        }
    }

    /// Adds the workspace's variables below everything else (R-WS-8), then runtime
    /// variables above everything else (R-TEST-5): the workspace's captured values,
    /// then the project's.
    fn with_runtime(&self, ctx: vars::Context, root: Option<&Path>) -> vars::Context {
        let ws = self.workspace();
        let mut ctx = ctx.with_workspace(&self.workspace_values(), &ws.variables.secrets);
        if let Some(runtime) = self.workspace_runtime.get(&ws.id) {
            ctx.variables.extend(runtime.clone());
        }
        if let Some(runtime) = self.runtime.get(&root.map(Path::to_path_buf)) {
            ctx.variables.extend(runtime.clone());
        }
        ctx
    }

    /// The runtime variables of a project (`None`: the scratch request) (R-TEST-5).
    pub fn runtime_vars(&self, root: Option<&Path>) -> Option<&BTreeMap<String, String>> {
        self.runtime
            .get(&root.map(Path::to_path_buf))
            .filter(|v| !v.is_empty())
    }

    /// Applies variables set or unset by a run (R-TEST-5); `workspace.<name>` ones go
    /// to the workspace (R-WS-8).
    pub fn apply_runtime(
        &mut self,
        root: Option<&Path>,
        changes: &BTreeMap<String, Option<String>>,
    ) {
        let (project, workspace) = tortuga_core::runner::split_changes(changes);
        if !project.is_empty() {
            let vars = self.runtime.entry(root.map(Path::to_path_buf)).or_default();
            tortuga_core::runner::apply_changes(vars, &project);
        }
        if !workspace.is_empty() {
            let id = self.workspace().id.clone();
            let vars = self.workspace_runtime.entry(id).or_default();
            tortuga_core::runner::apply_changes(vars, &workspace);
        }
    }

    pub fn clear_runtime(&mut self, root: Option<&Path>) {
        self.runtime.remove(&root.map(Path::to_path_buf));
    }

    fn context_at(&self, root: &Path, path: Option<&Path>) -> vars::Context {
        let env = self.active_environment(root);
        self.context_in(root, path, env.as_deref(), true)
    }

    /// The context of the request at `path` with environment `env` (by id): its values,
    /// this machine's secret values for it, and the runtime values (R-SRV-10). `mock`:
    /// the mock's environment wins while it is chosen (R-MOCK-4).
    pub fn context_in(
        &self,
        root: &Path,
        path: Option<&Path>,
        env: Option<&str>,
        mock: bool,
    ) -> vars::Context {
        let secrets = self
            .secrets
            .get(root)
            .map(|s| s.for_environment(env))
            .unwrap_or_default();
        let defaults = &self.workspace().defaults;
        // While the mock is chosen, its environment sends this project's requests there
        // (R-MOCK-4); secrets keep the values of the project's own environment.
        let mock = if mock {
            self.mock_environment(root)
        } else {
            None
        };
        let ctx = self
            .collection(root)
            .map(|c| {
                let env = mock
                    .as_ref()
                    .or_else(|| env.and_then(|id| c.environment(id)));
                vars::Context::for_request_in(c, env, path)
                    .with_workspace_defaults(defaults)
                    .with_secrets(secrets)
            })
            .unwrap_or_default();
        self.with_runtime(ctx, Some(root))
    }
}

#[cfg(test)]
mod tests {
    use tortuga_core::format::Environment;
    use tortuga_core::secrets::SecretValues;
    use tortuga_core::storage::FsStorage;
    use tortuga_core::{Collection, HttpMethod};

    use super::*;

    /// R-VAR-9: each name the request uses with its value and source, secrets masked;
    /// dynamic ones say what they make; undefined ones are only marked; a runtime value
    /// wins over the environment's.
    #[test]
    fn a_request_knows_where_its_values_come_from() {
        let root = std::env::temp_dir().join(format!("tortuga-var-help-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let c = Collection::create(&FsStorage, &root, "Shop").expect("create");
        let mut manifest = c.manifest.clone();
        manifest.variables = [("version".to_owned(), "v1".to_owned())].into();
        manifest.secrets = vec!["token".into()];
        let dev = Environment {
            id: "dev".into(),
            name: "Dev".into(),
            variables: [("host".to_owned(), "dev.test".to_owned())].into(),
            ..Environment::default()
        };
        c.save_config(&FsStorage, &manifest, &[dev])
            .expect("config");
        let mut state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![root.clone()],
            ..crate::test_host::TestHost::default()
        }));
        let mut secrets = SecretValues::default();
        secrets
            .values
            .insert((None, "token".into()), "s3cret".into());
        state.secrets.insert(root.clone(), secrets);
        let key = ItemKey::new(&root, "get.toml");
        let request = Request::new(
            HttpMethod::Get,
            "https://{{host}}/{{version}}/{{token}}/{{missing}}/{{$guid}}",
        );
        let ctx = state.request_context(Some(&key));
        let help = state.var_help(Some(&key), &request, &ctx);
        let from = |n: &str| help.known.get(n).and_then(|i| i.from.clone());
        assert_eq!(from("host").as_deref(), Some("the environment “Dev”"));
        assert_eq!(from("version").as_deref(), Some("the project"));
        assert_eq!(
            from("token").as_deref(),
            Some("this computer's secret value")
        );
        assert_eq!(help.known["token"].value, "secret, hidden");
        assert!(!format!("{help:?}").contains("s3cret"), "never in Debug");
        assert_eq!(help.known["$guid"].value, "a random UUID");
        assert_eq!(help.undefined, ["missing"]);
        assert!(!help.known.contains_key("missing"));
        assert!(
            help.names
                .iter()
                .any(|(n, note)| n == "token" && note == "secret, hidden")
        );
        assert!(help.names.iter().any(|(n, _)| n == "$timestamp"));

        state.runtime.insert(
            Some(root.clone()),
            [("host".to_owned(), "captured.test".to_owned())].into(),
        );
        assert_eq!(
            state.variable_source(Some(&key), "host"),
            Some(Source::Runtime)
        );
        std::fs::remove_dir_all(&root).expect("clean up");
    }
}
