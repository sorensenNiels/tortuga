use std::path::Path;
use std::time::SystemTime;

use tortuga_core::secrets::SecretValues;
use tortuga_core::storage::FsStorage;
use tortuga_core::{Collection, Environment};

use super::*;
use crate::test_host::{StartedMcp, TestHost};

/// A project "Shop ü😀" with the environments Dev and Prod and the secret `token`.
fn project(root: &Path) {
    let c = Collection::create(&FsStorage, root, "Shop ü😀").expect("create");
    let mut manifest = c.manifest.clone();
    manifest.secrets = vec!["token".into()];
    let env = |id: &str, name: &str| Environment {
        id: id.into(),
        name: name.into(),
        ..Environment::default()
    };
    c.save_config(
        &FsStorage,
        &manifest,
        &[env("dev", "Dev"), env("prod", "Prod")],
    )
    .expect("config");
}

fn state(root: &Path, taken_ports: Vec<u16>) -> (AppState, TestHost) {
    let mut settings = tortuga_core::AppSettings::default();
    settings
        .addons
        .set_enabled(tortuga_core::settings::AddOn::McpServer, true);
    let host = TestHost {
        open: vec![root.to_owned()],
        taken_ports,
        settings,
        ..TestHost::default()
    };
    let probe = TestHost {
        mcp: std::rc::Rc::clone(&host.mcp),
        ..TestHost::default()
    };
    (AppState::new(Box::new(host)), probe)
}

fn server(state: &AppState) -> &McpServer {
    state.addons.mcp.as_ref().expect("the add-on is on")
}

fn served(probe: &TestHost) -> Vec<McpProject> {
    let started = probe.mcp.borrow();
    let StartedMcp(projects, _) = started.as_ref().expect("started");
    projects.read().expect("projects").clone()
}

/// R-MCP-5: starts on 4040, else a free port; starting again keeps it; stop ends it.
#[test]
fn starts_on_4040_else_a_free_port() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, _) = state(dir.path(), vec![]);
    assert_eq!(state.start_mcp(), Ok("http://127.0.0.1:4040/mcp".into()));
    assert_eq!(state.start_mcp(), Ok("http://127.0.0.1:4040/mcp".into()));
    state.stop_mcp();
    assert!(server(&state).running().is_none());

    let (mut taken, _) = state_with_taken(dir.path());
    assert_eq!(taken.start_mcp(), Ok("http://127.0.0.1:0/mcp".into()));
}

fn state_with_taken(root: &Path) -> (AppState, TestHost) {
    state(root, vec![tortuga_core::mcp::DEFAULT_PORT])
}

/// R-MCP-5: the server serves the open projects with the chosen environment, follows a
/// switch, gets this machine's secrets by environment, and loses a closed project.
#[test]
fn projects_follow_the_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let root = dir.path();
    let (mut state, probe) = state(root, vec![]);
    state.set_workspace_environment("Dev");
    state.start_mcp().expect("start");

    let projects = served(&probe);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "Shop ü😀");
    assert_eq!(projects[0].environment.as_deref(), Some("Dev"));

    state.set_workspace_environment("Prod");
    assert_eq!(served(&probe)[0].environment.as_deref(), Some("Prod"));

    let mut values = SecretValues::default();
    values.values.insert(
        (Some("prod".into()), "token".into()),
        "prod-ü😀-secret".into(),
    );
    values
        .values
        .insert((None, "token".into()), "default-secret".into());
    state.save_secrets(root, values).expect("save");
    let secrets = &served(&probe)[0].secrets;
    assert_eq!(secrets["Prod"]["token"], "prod-ü😀-secret");
    assert_eq!(secrets["Dev"]["token"], "default-secret");
    assert_eq!(secrets[""]["token"], "default-secret");

    state.close_collection(root);
    assert_eq!(served(&probe).len(), 0);
}

/// R-MCP-5: what agents did reaches the log, newest last, until cleared.
#[test]
fn actions_are_logged() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, probe) = state(dir.path(), vec![]);
    state.start_mcp().expect("start");
    {
        let started = probe.mcp.borrow();
        let StartedMcp(_, on_action) = started.as_ref().expect("started");
        for summary in ["Sent get-user", "Create folder Orders"] {
            on_action(McpAction {
                at: SystemTime::now(),
                project: "Shop ü😀".into(),
                summary: summary.into(),
                outcome: "ok".into(),
                failed: false,
            });
        }
    }

    state.poll_addons();

    let log = server(&state).log();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].summary, "Create folder Orders");
    state.addons.mcp.as_mut().expect("on").clear_log();
    assert_eq!(server(&state).log(), &[]);
}
