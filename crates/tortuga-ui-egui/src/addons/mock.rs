//! The mock server add-on (R-ADD-5): the projects' mock servers in the app (R-MOCK-2,
//! R-MOCK-4, D45), started and stopped per project, answering from what is on disk, with
//! a log of what they received and a "Mock" environment that sends requests to them.

mod dialog;
pub(crate) mod ui;

pub(crate) use ui::Command;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, PoisonError, RwLock};

use tortuga_core::format::Environment;
use tortuga_core::mock::{self, Hit, MockApi};

use crate::host::MockHandle;
use crate::state::{AppState, root_key};

/// Hits kept per project; older ones are dropped.
const MAX_HITS: usize = 500;

/// Ports above the default tried when it is taken (another project's mock, say).
const PORTS_TO_TRY: u16 = 20;

/// The running mock servers, by project root.
#[derive(Debug)]
pub struct Mocks {
    projects: BTreeMap<PathBuf, ProjectMock>,
    /// Requests go to the mocks: the "Mock" environment is chosen (R-MOCK-4).
    pub selected: bool,
    tx: Sender<(PathBuf, Hit)>,
    rx: Receiver<(PathBuf, Hit)>,
    /// The mock dialog, while it is open.
    dialog: Option<dialog::MockDialog>,
}

#[derive(Debug)]
struct ProjectMock {
    handle: MockHandle,
    api: Arc<RwLock<MockApi>>,
    /// Oldest first.
    hits: Vec<Hit>,
}

impl Default for Mocks {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            projects: BTreeMap::new(),
            selected: false,
            tx,
            rx,
            dialog: None,
        }
    }
}

impl Mocks {
    /// The port of the project's running mock.
    pub fn port(&self, root: &Path) -> Option<u16> {
        self.projects.get(root).map(|m| m.handle.port)
    }

    /// The ports of every running mock, in project order.
    pub fn ports(&self) -> Vec<u16> {
        self.projects.values().map(|m| m.handle.port).collect()
    }

    /// Whether requests go to the mocks (R-MOCK-4).
    pub fn in_use(&self) -> bool {
        self.selected && !self.projects.is_empty()
    }

    /// What the project's mock received, oldest first.
    pub fn hits(&self, root: &Path) -> &[Hit] {
        self.projects.get(root).map_or(&[], |m| &m.hits)
    }

    pub fn clear_hits(&mut self, root: &Path) {
        if let Some(m) = self.projects.get_mut(root) {
            m.hits.clear();
        }
    }

    /// Takes in what the mocks answered since the last call.
    pub fn take_hits(&mut self) {
        while let Ok((root, hit)) = self.rx.try_recv() {
            // A mock stopped meanwhile keeps no log.
            if let Some(m) = self.projects.get_mut(&root) {
                if m.hits.len() == MAX_HITS {
                    m.hits.remove(0);
                }
                m.hits.push(hit);
            }
        }
    }
}

impl AppState {
    /// The port of the project's running mock.
    pub fn mock_port(&self, root: &Path) -> Option<u16> {
        self.addons.mock.as_ref()?.port(root)
    }

    /// The ports of every running mock, in project order.
    pub fn mock_ports(&self) -> Vec<u16> {
        self.addons
            .mock
            .as_ref()
            .map(Mocks::ports)
            .unwrap_or_default()
    }

    /// Whether requests go to the mocks (R-MOCK-4).
    pub fn mocks_in_use(&self) -> bool {
        self.addons.mock.as_ref().is_some_and(Mocks::in_use)
    }

    /// Starts the mock of the project at `root` (R-MOCK-2) on its remembered port, else
    /// the first free one from 4010. Returns the port.
    pub fn start_mock(&mut self, root: &Path) -> Result<u16, String> {
        let Some(mocks) = &self.addons.mock else {
            return Err(t!("mock-addon-off"));
        };
        if let Some(port) = mocks.port(root) {
            return Ok(port);
        }
        let (tx, taken) = (mocks.tx.clone(), mocks.ports());
        let api = Arc::new(RwLock::new(self.mock_api(root)?));
        let remembered = self.workspace().mock_ports.get(&root_key(root)).copied();
        let ports: Vec<u16> = match remembered {
            Some(port) => vec![port],
            None => (0..PORTS_TO_TRY)
                .map(|n| mock::DEFAULT_PORT + n)
                .filter(|p| !taken.contains(p))
                .collect(),
        };
        let mut problem = t!("mock-no-port");
        for port in ports {
            let tx = tx.clone();
            let wake = self.waker();
            let for_root = root.to_owned();
            let on_hit = Box::new(move |hit| {
                // The receiver only goes away with the app.
                let _ = tx.send((for_root.clone(), hit));
                wake();
            });
            match self.host.start_mock(port, Arc::clone(&api), on_hit) {
                Ok(handle) => {
                    let port = handle.port;
                    if let Some(mocks) = &mut self.addons.mock {
                        let hits = Vec::new();
                        let mock = ProjectMock { handle, api, hits };
                        mocks.projects.insert(root.to_owned(), mock);
                    }
                    self.workspace_mut().mock_ports.insert(root_key(root), port);
                    self.save_local();
                    return Ok(port);
                }
                Err(e) => problem = e,
            }
        }
        Err(problem)
    }

    /// Stops the project's mock; with none left, requests stop going to the mocks.
    pub fn stop_mock(&mut self, root: &Path) {
        if let Some(mocks) = &mut self.addons.mock {
            mocks.projects.remove(root);
            if mocks.projects.is_empty() {
                mocks.selected = false;
            }
        }
    }

    /// The port the project's mock starts on next time; a running one restarts on it.
    pub fn set_mock_port(&mut self, root: &Path, port: u16) -> Result<u16, String> {
        let running = self.mock_port(root).is_some();
        self.stop_mock(root);
        self.workspace_mut().mock_ports.insert(root_key(root), port);
        self.save_local();
        if running {
            self.start_mock(root)
        } else {
            Ok(port)
        }
    }

    /// Sends requests of projects with a running mock to it (R-MOCK-4, D45).
    pub fn use_mock_environment(&mut self) {
        if let Some(mocks) = &mut self.addons.mock {
            mocks.selected = true;
        }
    }

    /// Requests no longer go to the mocks: another environment was chosen.
    pub(crate) fn leave_mock_environment(&mut self) {
        if let Some(mocks) = &mut self.addons.mock {
            mocks.selected = false;
        }
    }

    /// The environment that sends the project's requests to its mock, while the mock
    /// runs and is chosen.
    pub fn mock_environment(&self, root: &Path) -> Option<Environment> {
        let mocks = self.addons.mock.as_ref()?;
        if !mocks.selected {
            return None;
        }
        let port = mocks.port(root)?;
        let c = self.collection(root)?;
        let mut env = mock::environment(c, &format!("http://127.0.0.1:{port}"));
        // Variables keep the values of the environment chosen before, as with
        // `tortuga-cli run --mock --env` (R-MOCK-5).
        if let Some(chosen) = self.effective_environment(root) {
            env.variables = chosen.variables;
        }
        Some(env)
    }

    /// Gives the project's mock what is on disk now, with the active environment's
    /// variables (R-MOCK-2).
    pub(crate) fn refresh_mock(&self, root: &Path) {
        let Some(mock) = self.addons.mock.as_ref().and_then(|m| m.projects.get(root)) else {
            return;
        };
        let api = Arc::clone(&mock.api);
        if let Ok(fresh) = self.mock_api(root) {
            *api.write().unwrap_or_else(PoisonError::into_inner) = fresh;
        }
    }

    /// [`AppState::refresh_mock`] for every running mock, after the environment changed.
    pub(crate) fn refresh_mocks(&self) {
        let Some(mocks) = &self.addons.mock else {
            return;
        };
        for root in mocks.projects.keys() {
            self.refresh_mock(root);
        }
    }

    fn mock_api(&self, root: &Path) -> Result<MockApi, String> {
        let c = self
            .collection(root)
            .ok_or_else(|| t!("mock-project-not-open"))?;
        // Secret values never reach a mock answer (P6).
        match self.effective_environment_without_secrets(root) {
            Some(env) => {
                let id = env.id.clone();
                Ok(MockApi::new(&c.clone().with_environment(env), Some(&id)))
            }
            None => Ok(MockApi::new(c, None)),
        }
    }
}

#[cfg(test)]
mod tests;
