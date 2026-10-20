use std::path::Path;
use std::time::SystemTime;

use tortuga_core::format::Server;
use tortuga_core::mock::{Example, MockRequest, Outcome};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Collection, Environment, HttpMethod, Request};

use super::*;
use crate::test_host::TestHost;

/// A project with the server "api", the environment "Dev" and one request with an example
/// that says the environment's greeting.
fn project(root: &Path) {
    let c = Collection::create(&FsStorage, root, "Shop ü😀").expect("create");
    let mut manifest = c.manifest.clone();
    manifest.servers = vec![Server {
        id: "api".into(),
        name: "API".into(),
    }];
    let dev = Environment {
        id: "dev".into(),
        name: "Dev".into(),
        servers: [("api".to_owned(), "https://dev.test".to_owned())].into(),
        variables: [("greeting".to_owned(), "hej".to_owned())].into(),
        ..Environment::default()
    };
    c.save_config(&FsStorage, &manifest, &[dev])
        .expect("config");
    let mut hello = Request::new(HttpMethod::Get, "/hello");
    hello.name = "Hello".into();
    hello.server = Some("api".into());
    hello.examples = vec![Example {
        body: "{{greeting}}".into(),
        ..Example::new("hi")
    }];
    Collection::open(&FsStorage, root)
        .expect("open")
        .new_request(&FsStorage, Path::new(""), &hello)
        .expect("request");
}

fn answer(host_mock: &TestHost) -> String {
    let mock = host_mock.mock.borrow();
    let crate::test_host::StartedMock(api, _) = mock.as_ref().expect("a mock started");
    let answer = api
        .read()
        .expect("api")
        .answer(&MockRequest::new("GET", "/hello"));
    String::from_utf8(answer.body).expect("utf-8")
}

fn state(root: &Path, taken_ports: Vec<u16>) -> (AppState, TestHost) {
    let host = TestHost {
        open: vec![root.to_owned()],
        taken_ports,
        ..TestHost::default()
    };
    let probe = TestHost {
        mock: std::rc::Rc::clone(&host.mock),
        local: std::rc::Rc::clone(&host.local),
        ..TestHost::default()
    };
    (AppState::new(Box::new(host)), probe)
}

/// R-MOCK-2: the mock starts on the first free port from 4010, which is remembered and
/// used again, and stops again.
#[test]
fn starts_on_a_free_port_and_remembers_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, probe) = state(dir.path(), vec![4010]);
    let root = dir.path();
    assert_eq!(state.start_mock(root), Ok(4011));
    assert_eq!(state.mocks.port(root), Some(4011));
    assert_eq!(state.start_mock(root), Ok(4011), "already running");
    let saved = probe.local.borrow().clone().expect("saved");
    assert_eq!(
        saved.workspaces[0].mock_ports.get(&root_key(root)),
        Some(&4011)
    );
    state.stop_mock(root);
    assert_eq!(state.mocks.port(root), None);
    assert_eq!(state.start_mock(root), Ok(4011), "the remembered port");
    assert_eq!(state.set_mock_port(root, 5000), Ok(5000), "restarts on it");
    assert_eq!(state.mocks.port(root), Some(5000));
}

/// R-MOCK-2: a remembered port that is taken is an error, not another port.
#[test]
fn a_taken_remembered_port_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, _) = state(dir.path(), vec![4500]);
    let root = dir.path();
    assert_eq!(state.set_mock_port(root, 4500), Ok(4500), "not running yet");
    assert_eq!(state.start_mock(root), Err("port 4500 is in use".into()));
    assert_eq!(state.mocks.port(root), None);
}

/// R-MOCK-4, D45: while the mock environment is chosen, the project's servers point at
/// its mock; choosing another environment, or stopping the mock, ends that.
#[test]
fn the_mock_environment_points_servers_at_the_mock() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, _) = state(dir.path(), vec![]);
    let root = dir.path();
    let url = |state: &AppState| state.context(Some(root)).server_urls["api"].clone();
    assert_eq!(url(&state), "https://dev.test");
    state.start_mock(root).expect("start");
    assert_eq!(url(&state), "https://dev.test", "not chosen yet");
    state.use_mock_environment();
    assert!(state.mocks.in_use());
    assert_eq!(url(&state), "http://127.0.0.1:4010");
    assert_eq!(
        state.context(Some(root)).variables["greeting"],
        "hej",
        "the project's variables stay"
    );
    state.set_workspace_environment("Dev");
    assert_eq!(url(&state), "https://dev.test");
    state.use_mock_environment();
    state.stop_mock(root);
    assert!(!state.mocks.in_use());
    assert_eq!(url(&state), "https://dev.test");
}

/// R-MOCK-2: the mock answers with what is on disk after a save, and logs what it received.
#[test]
fn answers_follow_the_files_and_hits_are_logged() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, probe) = state(dir.path(), vec![]);
    let root = dir.path();
    state.start_mock(root).expect("start");
    assert_eq!(answer(&probe), "hej");

    state.run(root, |c, s| {
        let item = c.request(Path::new("hello.toml")).expect("request");
        let mut changed = item.request.clone();
        changed.examples[0].body = "{{greeting}} ü😀".into();
        c.save_request(s, &item.path, &changed)
    });
    assert_eq!(answer(&probe), "hej ü😀");

    let hit = Hit {
        at: SystemTime::now(),
        method: "GET".into(),
        target: "/hello".into(),
        status: 200,
        outcome: Outcome::NoMatch("x".into()),
    };
    {
        let mock = probe.mock.borrow();
        let crate::test_host::StartedMock(_, on_hit) = mock.as_ref().expect("started");
        on_hit(hit.clone());
        on_hit(hit);
    }
    state.mocks.take_hits();
    assert_eq!(state.mocks.hits(root).len(), 2);
    state.mocks.clear_hits(root);
    assert_eq!(state.mocks.hits(root).len(), 0);

    state.close_collection(root);
    assert_eq!(
        state.mocks.port(root),
        None,
        "closing the project stops its mock"
    );
    assert!(state.workspace().mock_ports.is_empty());
}

/// R-MOCK-4: a workflow run in the app goes to the mock too, while it is chosen.
#[test]
fn workflow_runs_use_the_mock_environment() {
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let (mut state, _) = state(dir.path(), vec![]);
    let root = dir.path();
    let url = |state: &AppState| {
        let (collection, env) = state.collection_for_run(root).expect("open");
        tortuga_core::vars::Context::for_collection(&collection, env.as_deref()).server_urls["api"]
            .clone()
    };
    assert_eq!(url(&state), "https://dev.test");
    state.start_mock(root).expect("start");
    state.use_mock_environment();
    assert_eq!(url(&state), "http://127.0.0.1:4010");
    let (collection, _) = state.collection_for_run(root).expect("open");
    assert_eq!(
        state.collection(root).map(|c| c.environments.len()),
        Some(collection.environments.len() - 1),
        "the project itself is left as it is on disk"
    );
}
