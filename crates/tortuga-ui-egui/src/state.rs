//! App-wide state: settings, the HTTP client, the workspaces and their open projects.
//!
//! In code a project is still a [`Collection`] (the engine's name for it); the UI and
//! the docs say "project" (D12).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use tortuga_core::CookieJar;
use tortuga_core::git::GitStatus;
use tortuga_core::secrets::SecretValues;
use tortuga_core::storage::Storage;
use tortuga_core::workspace::{self, ProjectRef, WorkspaceVariables};
use tortuga_core::{Client, ClientSettings, Collection, Request, vars};

use crate::format::sentence;
use crate::host::{Host, LocalState, Workspace};

/// Identifies a workflow of an open project (R-FLOW-2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkflowKey {
    pub root: PathBuf,
    /// The workflow file's stem.
    pub id: String,
}

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
    /// Open projects of the active workspace, in workspace order (R-COL-5, R-WS-1).
    pub collections: Vec<Collection>,
    /// Projects of the active workspace that could not be opened (moved, or not
    /// cloned on this machine yet), with the reason (R-WS-5).
    pub missing: Vec<(ProjectRef, String)>,
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
    /// Values captured or set by scripts, per project (`None`: the scratch request);
    /// kept until the app closes, never on disk (R-TEST-5, D26).
    pub runtime: BTreeMap<Option<PathBuf>, BTreeMap<String, String>>,
    /// Values captured or set as `workspace.<name>`, per workspace id (R-WS-8); kept
    /// like `runtime`.
    pub workspace_runtime: BTreeMap<String, BTreeMap<String, String>>,
    /// This machine's values of the active workspace's secret variables (R-WS-8).
    pub workspace_secrets: BTreeMap<String, String>,
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

/// The client for `settings`, trusting its CA certificate files (R-HTTP-16), which are
/// read through the host. Problems with those files come back as messages; the other
/// certificates still apply.
fn make_client(
    host: &dyn Host,
    settings: &ClientSettings,
) -> Result<(Client, Vec<String>), String> {
    let mut problems = Vec::new();
    let mut files = Vec::new();
    for path in &settings.ca_certificates {
        let read = match host.file_reader() {
            Some(reader) => (reader.0)(path).map_err(|e| e.to_string()),
            None => Err("files can't be read here".to_owned()),
        };
        match read {
            Ok(pem) => files.push((path.clone(), pem)),
            Err(e) => problems.push(format!("CA certificate {}: {e}", path.display())),
        }
    }
    let (client, bad) =
        Client::with_ca_certificates(settings.clone(), &files).map_err(|e| e.to_string())?;
    problems.extend(bad.iter().map(ToString::to_string));
    Ok((with_files(client, host), problems))
}

/// Lets the client upload files where the host can read them (R-HTTP-15).
fn with_files(client: Client, host: &dyn Host) -> Client {
    match host.file_reader() {
        Some(files) => client.with_file_reader(files),
        None => client,
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
        let client = match make_client(host.as_ref(), &settings) {
            Ok((client, ca_problems)) => {
                problems.extend(ca_problems);
                client
            }
            Err(e) => {
                problems.push(e);
                with_files(Client::default(), host.as_ref())
            }
        };
        let mut local = host.load_local().unwrap_or_else(|e| {
            problems.push(e);
            LocalState::default()
        });
        local.migrate();
        let mut state = Self {
            host,
            settings,
            client,
            collections: Vec::new(),
            missing: Vec::new(),
            local,
            secrets: BTreeMap::new(),
            cookies: BTreeMap::new(),
            scratch_cookies: CookieJar::default(),
            git: BTreeMap::new(),
            live: Live::default(),
            notice: (!problems.is_empty()).then(|| problems.join("\n")),
            runtime: BTreeMap::new(),
            workspace_runtime: BTreeMap::new(),
            workspace_secrets: BTreeMap::new(),
        };
        state.open_workspace();
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
        let (client, problems) = make_client(self.host.as_ref(), settings)?;
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        self.client = client;
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
            Err(e) => self.notify(format!("Could not reload the project: {e}")),
        }
    }

    /// Opens the project at `root` and adds it to the workspace (or does nothing if already open).
    pub fn open_collection(&mut self, root: &Path) -> tortuga_core::Result<()> {
        if self.collection(root).is_some() {
            return Ok(());
        }
        self.load_project(root)?;
        self.missing.retain(|(p, _)| p.path != root);
        let ws = self.workspace_mut();
        if !ws.projects.iter().any(|p| p.path == root) {
            ws.projects.push(ProjectRef::new(root));
        }
        self.workspace_changed();
        Ok(())
    }

    pub fn create_collection(&mut self, root: &Path, name: &str) -> tortuga_core::Result<()> {
        Collection::create(self.storage(), root, name)?;
        self.open_collection(root)
    }

    /// Closes a project and removes it from the workspace.
    pub fn close_collection(&mut self, root: &Path) {
        self.unload_project(root);
        let ws = self.workspace_mut();
        ws.projects.retain(|p| p.path != root);
        ws.project_environments.remove(&root_key(root));
        self.workspace_changed();
    }

    /// Reads a project from disk with its secrets and cookies, and starts watching it.
    fn load_project(&mut self, root: &Path) -> tortuga_core::Result<()> {
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
        Ok(())
    }

    /// Closes a project without changing the workspace.
    fn unload_project(&mut self, root: &Path) {
        self.save_cookies(Some(root));
        self.host.unwatch(root);
        self.git.remove(root);
        self.collections.retain(|c| c.root != root);
        self.secrets.remove(root);
        self.cookies.remove(root);
    }

    // ------------------------------------------------------------ workspaces (M6.5)

    /// The active workspace (R-WS-1).
    pub fn workspace(&self) -> &Workspace {
        &self.local.workspaces[self.local.active_workspace]
    }

    fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.local.workspaces[self.local.active_workspace]
    }

    /// Opens the projects of the active workspace. A workspace saved as a file is
    /// read from it first, so a `git pull` of the file brings teammates' projects (R-WS-5).
    fn open_workspace(&mut self) {
        if let Some(file) = self.workspace().file.clone() {
            match self.read_workspace_file(&file) {
                Ok(read) => {
                    let ws = self.workspace_mut();
                    ws.variables = read.variables;
                    let projects = read
                        .projects
                        .into_iter()
                        .map(|mut p| {
                            if p.remote.is_none() {
                                p.remote = ws
                                    .projects
                                    .iter()
                                    .find(|old| old.path == p.path)
                                    .and_then(|old| old.remote.clone());
                            }
                            p
                        })
                        .collect();
                    ws.projects = projects;
                }
                Err(e) => self.notify(format!("Could not read the workspace file: {e}")),
            }
        }
        self.load_workspace_secrets();
        for project in self.workspace().projects.clone() {
            if self.collection(&project.path).is_some() {
                continue; // listed twice
            }
            if let Err(e) = self.load_project(&project.path) {
                self.missing.push((project, sentence(&e.to_string())));
            }
        }
        self.save_local();
    }

    /// Closes the projects of the active workspace (they stay in it).
    fn close_workspace(&mut self) {
        let roots: Vec<PathBuf> = self.collections.iter().map(|c| c.root.clone()).collect();
        for root in roots {
            self.unload_project(&root);
        }
        self.missing.clear();
    }

    pub fn switch_workspace(&mut self, index: usize) {
        if index == self.local.active_workspace || index >= self.local.workspaces.len() {
            return;
        }
        self.close_workspace();
        self.local.active_workspace = index;
        self.open_workspace();
    }

    /// Adds an empty workspace and switches to it.
    pub fn new_workspace(&mut self, name: &str) {
        self.local.workspaces.push(Workspace {
            name: name.to_owned(),
            ..Workspace::default()
        });
        self.local.give_ids();
        self.switch_workspace(self.local.workspaces.len() - 1);
    }

    pub fn rename_workspace(&mut self, name: &str) {
        name.clone_into(&mut self.workspace_mut().name);
        self.workspace_changed();
    }

    /// Removes the active workspace from this machine (its projects and file stay on
    /// disk). The last workspace can't be removed.
    pub fn delete_workspace(&mut self) {
        if self.local.workspaces.len() < 2 {
            return;
        }
        self.close_workspace();
        self.local.workspaces.remove(self.local.active_workspace);
        self.local.active_workspace = self.local.active_workspace.saturating_sub(1);
        self.open_workspace();
    }

    /// Saves the active workspace as a shareable file and keeps it in sync from now on (R-WS-5).
    pub fn save_workspace_as(&mut self, file: &Path) -> Result<(), String> {
        self.workspace_mut().file = Some(file.to_owned());
        self.write_workspace_file()?;
        self.save_local();
        Ok(())
    }

    /// Opens a workspace file: switches to the workspace saved in it, adding it to
    /// this machine's workspaces the first time (R-WS-5).
    pub fn open_workspace_file(&mut self, file: &Path) -> Result<(), String> {
        let read = self.read_workspace_file(file)?;
        if let Some(index) = self
            .local
            .workspaces
            .iter()
            .position(|w| w.file.as_deref() == Some(file))
        {
            self.switch_workspace(index);
            return Ok(());
        }
        self.local.workspaces.push(Workspace {
            name: read.name,
            file: Some(file.to_owned()),
            projects: read.projects,
            variables: read.variables,
            ..Workspace::default()
        });
        self.local.give_ids();
        self.switch_workspace(self.local.workspaces.len() - 1);
        Ok(())
    }

    fn read_workspace_file(&self, file: &Path) -> Result<workspace::WorkspaceFile, String> {
        let text = self
            .storage()
            .read_to_string(file)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        workspace::read(&text, file).map_err(|e| e.to_string())
    }

    /// Writes the workspace file, if the workspace has one and it changed.
    fn write_workspace_file(&self) -> Result<(), String> {
        let ws = self.workspace();
        let Some(file) = &ws.file else {
            return Ok(());
        };
        let text = workspace::write(&ws.name, &ws.projects, &ws.variables, file);
        if self.storage().read_to_string(file).ok().as_deref() == Some(text.as_str()) {
            return Ok(()); // unchanged: leave the file (and git) alone
        }
        self.storage()
            .write(file, &text)
            .map_err(|e| format!("Could not save the workspace file {}: {e}", file.display()))
    }

    /// Saves local state and the workspace file after the workspace changed.
    fn workspace_changed(&mut self) {
        self.save_local();
        if let Err(e) = self.write_workspace_file() {
            self.notify(e);
        }
    }

    // ------------------------------------------------------------ workspace variables (R-WS-8)

    /// Reads this machine's values of the workspace's secret variables.
    fn load_workspace_secrets(&mut self) {
        self.workspace_secrets.clear();
        let ws = self.workspace();
        let mut problem = None;
        let mut values = BTreeMap::new();
        for name in &ws.variables.secrets {
            match self
                .host
                .secrets()
                .get(&workspace::secret_key(&ws.id, name))
            {
                Ok(Some(value)) => drop(values.insert(name.clone(), value)),
                Ok(None) => {}
                Err(e) => problem = Some(e),
            }
        }
        self.workspace_secrets = values;
        if let Some(e) = problem {
            self.notify(format!("Could not read secret values: {e}"));
        }
    }

    /// The workspace's variables as requests use them: shared values plus this
    /// machine's secret values.
    pub fn workspace_values(&self) -> BTreeMap<String, String> {
        self.workspace().variables.resolved(&self.workspace_secrets)
    }

    /// Saves the workspace's variables: shared ones to local data and the workspace
    /// file, secret values (`secret_values`, by name) to this machine's secret store.
    /// Values of secrets that were removed are deleted from the store.
    pub fn save_workspace_variables(
        &mut self,
        variables: WorkspaceVariables,
        secret_values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let id = self.workspace().id.clone();
        let old: Vec<String> = self.workspace().variables.secrets.iter().cloned().collect();
        let store = self.host.secrets();
        for name in old.iter().filter(|n| !variables.secrets.contains(*n)) {
            store
                .delete(&workspace::secret_key(&id, name))
                .map_err(|e| e.to_string())?;
        }
        for name in &variables.secrets {
            let key = workspace::secret_key(&id, name);
            match secret_values.get(name).filter(|v| !v.is_empty()) {
                Some(value) => store.set(&key, value),
                None => store.delete(&key),
            }
            .map_err(|e| e.to_string())?;
        }
        self.workspace_mut().variables = variables;
        self.workspace_changed();
        self.load_workspace_secrets();
        Ok(())
    }

    /// Values captured or set as `workspace.<name>` in the active workspace.
    pub fn workspace_runtime_vars(&self) -> Option<&BTreeMap<String, String>> {
        self.workspace_runtime
            .get(&self.workspace().id)
            .filter(|v| !v.is_empty())
    }

    pub fn clear_workspace_runtime(&mut self) {
        let id = self.workspace().id.clone();
        self.workspace_runtime.remove(&id);
    }

    /// Removes a missing project from the workspace.
    pub fn remove_missing(&mut self, path: &Path) {
        self.missing.retain(|(p, _)| p.path != path);
        self.workspace_mut().projects.retain(|p| p.path != path);
        self.workspace_changed();
    }

    /// The user found a missing project at `new`: use that folder instead (R-WS-5).
    pub fn locate_missing(&mut self, old: &Path, new: &Path) -> tortuga_core::Result<()> {
        if self.collection(new).is_none() {
            self.load_project(new)?;
        }
        self.missing.retain(|(p, _)| p.path != old);
        let ws = self.workspace_mut();
        // The project's own environment choice moves with it.
        if let Some(id) = ws.project_environments.remove(&root_key(old)) {
            ws.project_environments.entry(root_key(new)).or_insert(id);
        }
        let already_listed = ws.projects.iter().any(|p| p.path == new);
        if already_listed {
            ws.projects.retain(|p| p.path != old);
        } else if let Some(p) = ws.projects.iter_mut().find(|p| p.path == old) {
            new.clone_into(&mut p.path);
        }
        self.workspace_changed();
        Ok(())
    }

    /// Open projects grouped by git repository, in sidebar order (R-WS-4). Projects
    /// outside git, or whose status is not known yet, are groups of their own.
    pub fn project_groups(&self) -> Vec<(Option<PathBuf>, Vec<&Collection>)> {
        let mut groups: Vec<(Option<PathBuf>, Vec<&Collection>)> = Vec::new();
        for c in &self.collections {
            let repo = self.git.get(&c.root).map(|g| g.repository.clone());
            match groups
                .iter_mut()
                .find(|(r, _)| repo.is_some() && *r == repo)
            {
                Some((_, members)) => members.push(c),
                None => groups.push((repo, vec![c])),
            }
        }
        groups
    }

    /// Remembers the selected request in the workspace (`None`: the scratch request).
    pub fn remember_selection(&mut self, key: Option<&ItemKey>) {
        let selection = key.map(|k| (k.root.clone(), k.path.clone()));
        if self.workspace().selection != selection {
            self.workspace_mut().selection = selection;
            self.save_local();
        }
    }

    /// Keeps the workspace's open tabs (R-COL-13), writing local data only if they changed.
    pub fn remember_tabs(&mut self, tabs: crate::host::SavedTabs) {
        if self.workspace().tabs != tabs {
            self.workspace_mut().tabs = tabs;
            self.save_local();
        }
    }

    /// The open tabs of the workspace as last remembered.
    pub fn remembered_tabs(&self) -> &crate::host::SavedTabs {
        &self.workspace().tabs
    }

    /// The remembered selection of the workspace, if that request still exists.
    pub fn remembered_selection(&self) -> Option<ItemKey> {
        let (root, path) = self.workspace().selection.clone()?;
        let key = ItemKey::new(&root, path);
        self.saved_request(&key).is_some().then_some(key)
    }

    // ------------------------------------------------------------ environments

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

    /// Switches every project that has an environment called `name` (R-WS-6).
    pub fn set_workspace_environment(&mut self, name: &str) {
        self.workspace_mut().environment = Some(name.to_owned());
        self.save_local();
    }

    /// The environment a project uses when it has none named like the workspace's.
    pub fn set_project_environment(&mut self, root: &Path, id: &str) {
        self.workspace_mut()
            .project_environments
            .insert(root_key(root), id.to_owned());
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
        let secrets = self
            .secrets
            .get(root)
            .map(|s| s.for_environment(env.as_deref()))
            .unwrap_or_default();
        let ctx = self
            .collection(root)
            .map(|c| vars::Context::for_request(c, env.as_deref(), path).with_secrets(secrets))
            .unwrap_or_default();
        self.with_runtime(ctx, Some(root))
    }

    // ------------------------------------------------------------ live changes (M6)

    /// Lets background threads wake the UI. Call once the egui context exists.
    pub fn set_repaint(&self, ctx: &egui::Context) {
        let _ = self.live.repaint.set(ctx.clone());
    }

    /// A function that wakes the UI from any thread, e.g. when background work is done.
    pub fn waker(&self) -> impl Fn() + Send + Sync + 'static {
        let repaint = self.live.repaint.clone();
        move || {
            if let Some(ctx) = repaint.get() {
                ctx.request_repaint();
            }
        }
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
            if self.collection(&root).is_none() {
                self.live.git_again.remove(&root);
                continue; // closed meanwhile
            }
            match result {
                Ok(Some(status)) => {
                    self.remember_remote(&root, status.remote.as_deref());
                    self.git.insert(root.clone(), status);
                }
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

    /// Keeps the project's git remote in the workspace, so a missing project can show
    /// where to clone it from, and a workspace file can tell teammates (R-WS-5).
    fn remember_remote(&mut self, root: &Path, remote: Option<&str>) {
        let Some(remote) = remote else {
            return;
        };
        let Some(project) = self
            .workspace_mut()
            .projects
            .iter_mut()
            .find(|p| p.path == root)
        else {
            return;
        };
        if project.remote.as_deref() != Some(remote) {
            project.remote = Some(remote.to_owned());
            self.workspace_changed();
        }
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

    /// A fresh temp folder for a test.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tortuga-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// A project on disk with one request and the given environment names.
    fn project(root: &Path, name: &str, environments: &[&str]) -> PathBuf {
        use tortuga_core::storage::FsStorage;

        let c = Collection::create(&FsStorage, root, name).expect("create");
        let envs: Vec<tortuga_core::Environment> = environments
            .iter()
            .map(|e| tortuga_core::Environment {
                id: e.to_lowercase(),
                name: (*e).to_owned(),
                ..Default::default()
            })
            .collect();
        c.save_config(&FsStorage, &c.manifest, &envs)
            .expect("config");
        let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/");
        request.name = "List".into();
        c.new_request(&FsStorage, Path::new(""), &request)
            .expect("request")
    }

    fn roots(state: &AppState) -> Vec<PathBuf> {
        state.collections.iter().map(|c| c.root.clone()).collect()
    }

    /// R-WS-1: each workspace opens its own projects and remembers its selection,
    /// also after a restart; state from before workspaces becomes the first one.
    #[test]
    fn workspaces_open_their_own_projects() {
        let dir = temp("switch");
        let (a, b) = (dir.join("a"), dir.join("b"));
        let request = project(&a, "A", &[]);
        project(&b, "B", &[]);
        let host = crate::test_host::TestHost {
            open: vec![a.clone()],
            ..Default::default()
        };
        let saved = host.local.clone();
        let mut state = AppState::new(Box::new(host));
        assert_eq!(state.workspace().name, LocalState::DEFAULT_WORKSPACE);
        assert_eq!(roots(&state), std::slice::from_ref(&a));
        state.remember_selection(Some(&ItemKey::new(&a, request.clone())));

        state.new_workspace("Side project");
        assert_eq!(roots(&state), Vec::<PathBuf>::new());
        state.open_collection(&b).expect("open b");
        assert_eq!(roots(&state), std::slice::from_ref(&b));

        state.switch_workspace(0);
        assert_eq!(roots(&state), std::slice::from_ref(&a));
        assert_eq!(
            state.remembered_selection(),
            Some(ItemKey::new(&a, request.clone()))
        );

        let restarted = AppState::new(Box::new(crate::test_host::TestHost {
            local: saved,
            ..Default::default()
        }));
        assert_eq!(restarted.local.workspaces.len(), 2);
        assert_eq!(roots(&restarted), std::slice::from_ref(&a));

        state.switch_workspace(1);
        state.delete_workspace();
        assert_eq!(state.local.workspaces.len(), 1);
        assert_eq!(roots(&state), [a]);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-5: a workspace saved as a file opens on another machine; projects that
    /// aren't there show as missing until located.
    #[test]
    fn workspace_files_are_shared_and_missing_projects_located() {
        let dir = temp("file");
        let (users, orders) = (dir.join("users"), dir.join("platform/orders"));
        project(&users, "Users", &[]);
        project(&orders, "Orders", &[]);
        let mut state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![users.clone(), orders.clone()],
            ..Default::default()
        }));
        state.rename_workspace("Acme");
        let file = dir.join("platform/acme.tortuga-workspace.toml");
        state.save_workspace_as(&file).expect("save");
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(text.contains("path = \"../users\""), "{text}");
        assert!(text.contains("path = \"orders\""), "{text}");

        // A teammate who has only cloned the platform repository so far.
        let moved = dir.join("elsewhere/users");
        std::fs::create_dir_all(dir.join("elsewhere")).expect("mkdir");
        std::fs::rename(&users, &moved).expect("move");
        let mut teammate = AppState::new(Box::new(crate::test_host::TestHost::default()));
        teammate.open_workspace_file(&file).expect("open file");
        assert_eq!(teammate.workspace().name, "Acme");
        assert_eq!(roots(&teammate), std::slice::from_ref(&orders));
        assert_eq!(teammate.missing.len(), 1);
        assert_eq!(teammate.missing[0].0.path, users);

        teammate.set_project_environment(&users, "chosen-before-it-moved");
        teammate.locate_missing(&users, &moved).expect("locate");
        assert_eq!(
            teammate
                .workspace()
                .project_environments
                .get(&root_key(&moved))
                .map(String::as_str),
            Some("chosen-before-it-moved"),
            "the project's environment choice follows it"
        );
        assert_eq!(teammate.missing.len(), 0);
        assert_eq!(roots(&teammate), [orders, moved]);
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(
            text.contains("path = \"../elsewhere/users\""),
            "file follows: {text}"
        );

        // Opening the same file again switches to that workspace instead of adding one.
        teammate.open_workspace_file(&file).expect("again");
        assert_eq!(teammate.local.workspaces.len(), 2);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-6: one environment choice switches every project that has it.
    #[test]
    fn environments_switch_by_name_across_the_workspace() {
        let dir = temp("env");
        let (a, b) = (dir.join("a"), dir.join("b"));
        project(&a, "A", &["Dev", "Staging"]);
        project(&b, "B", &["Local", "Dev"]);
        let mut state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![a.clone(), b.clone()],
            ..Default::default()
        }));
        assert_eq!(state.environment_names(), ["Dev", "Staging", "Local"]);

        state.set_workspace_environment("Dev");
        assert_eq!(state.active_environment(&a).as_deref(), Some("dev"));
        assert_eq!(state.active_environment(&b).as_deref(), Some("dev"));

        state.set_workspace_environment("Staging");
        assert_eq!(state.active_environment(&a).as_deref(), Some("staging"));
        assert!(!state.follows_workspace_environment(&b));
        let first = state.collection(&b).expect("b").environments[0].id.clone();
        assert_eq!(state.active_environment(&b), Some(first), "keeps its first");
        state.set_project_environment(&b, "local");
        assert_eq!(state.active_environment(&b).as_deref(), Some("local"));
        assert!(state.follows_workspace_environment(&a));
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-8: workspace variables reach every project below the project's own values,
    /// go into the workspace file (secrets by name only), and a `workspace.` capture in
    /// one project is used by the others.
    #[test]
    fn workspace_variables_are_shared_by_every_project() {
        use tortuga_core::storage::FsStorage;

        let dir = temp("ws-vars");
        let (auth, orders) = (dir.join("auth"), dir.join("orders"));
        project(&auth, "Auth", &[]);
        project(&orders, "Orders", &[]);
        let c = Collection::open(&FsStorage, &orders).expect("orders");
        let manifest = tortuga_core::Manifest {
            variables: [("tenant".to_owned(), "orders-own".to_owned())].into(),
            ..c.manifest.clone()
        };
        c.save_config(&FsStorage, &manifest, &c.environments)
            .expect("config");
        let mut state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![auth.clone(), orders.clone()],
            ..Default::default()
        }));
        state.rename_workspace("Acme");
        let file = dir.join("acme.tortuga-workspace.toml");
        state.save_workspace_as(&file).expect("save");
        assert!(
            std::fs::read_to_string(&file)
                .expect("file")
                .starts_with("format = 1\n"),
            "no variables: older versions can still read it"
        );

        let vars = WorkspaceVariables {
            values: [("tenant".to_owned(), "acme".to_owned())].into(),
            secrets: ["api_key".to_owned()].into(),
        };
        let secrets = BTreeMap::from([("api_key".to_owned(), "k-1".to_owned())]);
        state
            .save_workspace_variables(vars.clone(), &secrets)
            .expect("save variables");
        let value = |state: &AppState, root: Option<&Path>, name: &str| {
            state.context(root).variables.get(name).cloned()
        };
        assert_eq!(
            value(&state, Some(&auth), "tenant").as_deref(),
            Some("acme")
        );
        assert_eq!(
            value(&state, Some(&orders), "tenant").as_deref(),
            Some("orders-own"),
            "the project's own value wins"
        );
        assert_eq!(value(&state, None, "api_key").as_deref(), Some("k-1"));
        assert!(state.context(Some(&auth)).secrets.contains("api_key"));
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(text.contains("tenant = \"acme\""), "{text}");
        assert!(text.contains("secrets = [\"api_key\"]"), "{text}");
        assert!(!text.contains("k-1"), "secret values stay local: {text}");

        // A capture in Auth named `workspace.token`; Orders uses {{token}}.
        let changes = BTreeMap::from([
            ("workspace.token".to_owned(), Some("t-1".to_owned())),
            ("own".to_owned(), Some("auth-only".to_owned())),
        ]);
        state.apply_runtime(Some(&auth), &changes);
        assert_eq!(
            value(&state, Some(&orders), "token").as_deref(),
            Some("t-1")
        );
        assert_eq!(value(&state, Some(&orders), "own"), None);
        assert_eq!(
            value(&state, Some(&auth), "own").as_deref(),
            Some("auth-only")
        );
        state.clear_workspace_runtime();
        assert_eq!(value(&state, Some(&orders), "token"), None);

        // A teammate opens the file: shared values come along; the secret is unset.
        let mut teammate = AppState::new(Box::new(crate::test_host::TestHost::default()));
        teammate.open_workspace_file(&file).expect("open");
        assert_eq!(teammate.workspace().variables, vars);
        assert_eq!(
            value(&teammate, Some(&auth), "tenant").as_deref(),
            Some("acme")
        );
        assert_eq!(value(&teammate, Some(&auth), "api_key"), None);

        // Renaming keeps the secret value: it is keyed by the workspace's id.
        let key = workspace::secret_key(&state.workspace().id, "api_key");
        state.rename_workspace("Acme platform");
        assert_eq!(workspace::secret_key(&state.workspace().id, "api_key"), key);
        assert_eq!(
            state.host.secrets().get(&key).expect("store").as_deref(),
            Some("k-1")
        );

        // Removing the secret deletes its value from this machine.
        state
            .save_workspace_variables(WorkspaceVariables::default(), &BTreeMap::new())
            .expect("remove");
        assert_eq!(state.host.secrets().get(&key).expect("store"), None);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-4: projects in the same repository are grouped, in sidebar order.
    #[test]
    fn projects_are_grouped_by_repository() {
        let dir = temp("groups");
        let (a, b, c) = (dir.join("a"), dir.join("b"), dir.join("c"));
        for (root, name) in [(&a, "A"), (&b, "B"), (&c, "C")] {
            project(root, name, &[]);
        }
        let mut state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![a.clone(), b.clone(), c.clone()],
            ..Default::default()
        }));
        let in_repo = |repo: &str| GitStatus {
            repository: PathBuf::from(repo),
            ..GitStatus::default()
        };
        state.git.insert(a.clone(), in_repo("/repo"));
        state.git.insert(c.clone(), in_repo("/repo"));
        let groups: Vec<(Option<PathBuf>, Vec<PathBuf>)> = state
            .project_groups()
            .into_iter()
            .map(|(repo, members)| (repo, members.iter().map(|c| c.root.clone()).collect()))
            .collect();
        assert_eq!(
            groups,
            [(Some(PathBuf::from("/repo")), vec![a, c]), (None, vec![b]),]
        );
        std::fs::remove_dir_all(dir).expect("clean up");
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

    /// R-HTTP-16: CA certificate files are read through the host; a bad one is refused
    /// with its name, a good one is applied.
    #[test]
    fn ca_certificates_are_applied_or_reported() {
        let dir = std::env::temp_dir().join(format!("tortuga-ca-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let good = dir.join("company-ca.pem");
        std::fs::write(
            &good,
            include_str!("../../tortuga-core/tests/fixtures/test-ca.pem"),
        )
        .expect("write");
        let bad = dir.join("notes.txt");
        std::fs::write(&bad, "not a certificate").expect("write");
        let mut state = AppState::new(Box::new(crate::test_host::TestHost::default()));

        let with = |files: Vec<PathBuf>| ClientSettings {
            ca_certificates: files,
            ..ClientSettings::default()
        };
        let err = state
            .apply_settings(&with(vec![good.clone(), bad.clone()]))
            .expect_err("bad file");
        assert!(err.contains("notes.txt"), "{err}");
        let err = state
            .apply_settings(&with(vec![dir.join("gone.pem")]))
            .expect_err("missing file");
        assert!(err.contains("gone.pem"), "{err}");
        state
            .apply_settings(&with(vec![good.clone()]))
            .expect("good file");
        assert_eq!(state.settings.ca_certificates, [good]);
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
