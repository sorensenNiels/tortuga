//! `tortuga-cli mock` and `tortuga-cli run --mock` end to end (R-MOCK-5).

use std::io::{BufRead as _, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use tortuga_core::format::{Environment, Server};
use tortuga_core::mock::Example;
use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::Step;
use tortuga_core::{Auth, Client, Collection, HttpMethod, Request};

const FS: FsStorage = FsStorage;

/// A project designed before its API exists: log in, then get a user, both on the
/// server "api" with examples; "Delete user" has none yet.
fn project(dir: &Path, delete_in_workflow: bool) {
    let col = Collection::create(&FS, dir, "Designed ü😀").expect("create");
    let mut manifest = col.manifest.clone();
    manifest.servers = vec![Server {
        id: "api".into(),
        name: "API".into(),
    }];
    manifest.variables.insert("user".into(), "ann".into());
    let staging = Environment {
        id: "staging".into(),
        name: "Staging".into(),
        servers: [("api".to_owned(), "https://staging.invalid".to_owned())].into(),
        variables: [("user".to_owned(), "bob".to_owned())].into(),
        ..Environment::default()
    };
    col.save_config(&FS, &manifest, &[staging]).expect("config");

    let mut login = Request::new(HttpMethod::Post, "/login");
    login.name = "Log in".into();
    login.server = Some("api".into());
    login.capture.insert("token".into(), "$.token".into());
    login.examples = vec![Example {
        body: r#"{"token": "t-{{user}}"}"#.into(),
        ..Example::new("ok")
    }];
    let mut get = Request::new(HttpMethod::Get, "/users/{{user}}");
    get.name = "Get user".into();
    get.server = Some("api".into());
    get.auth = Some(Auth::Bearer {
        token: "{{token}}".into(),
    });
    get.examples = vec![
        Example {
            body: r#"{"name": "Bob"}"#.into(),
            conditions: tortuga_core::mock::Conditions {
                headers: [("Authorization".to_owned(), "Bearer t-bob".to_owned())].into(),
                ..Default::default()
            },
            ..Example::new("bob")
        },
        Example {
            body: r#"{"name": "Ann"}"#.into(),
            ..Example::new("anyone")
        },
    ];
    get.scripts.tests = r#"test("is Ann", () => expect(res.json().name).toBe("Ann"));"#.into();
    let mut delete = Request::new(HttpMethod::Delete, "/users/{{user}}");
    delete.name = "Delete user".into();
    delete.server = Some("api".into());
    for r in [&login, &get, &delete] {
        Collection::open(&FS, dir)
            .expect("open")
            .new_request(&FS, Path::new(""), r)
            .expect("request");
    }
    let col = Collection::open(&FS, dir).expect("open");
    let id = col.new_workflow(&FS, "Lifecycle").expect("workflow");
    let col = Collection::open(&FS, dir).expect("open");
    let mut wf = col.workflow(&id).expect("loaded").clone();
    wf.steps = vec![
        Step::new(Path::new("log-in.toml")),
        Step::new(Path::new("get-user.toml")),
    ];
    if delete_in_workflow {
        wf.steps.push(Step::new(Path::new("delete-user.toml")));
    }
    col.save_workflow(&FS, &wf).expect("save");
}

fn cli(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"))
        .args(args)
        .output()
        .expect("run tortuga-cli");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn a_workflow_runs_against_the_mock_before_the_api_exists() {
    let dir = tempfile::tempdir().expect("temp dir");
    project(dir.path(), false);
    let p = dir.path().to_str().expect("utf-8");

    let (code, out) = cli(&["run", p, "Lifecycle", "--mock"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("mock on http://127.0.0.1:"), "{out}");
    assert!(out.contains("ok   Log in (200 OK"), "{out}");
    assert!(out.contains("ok   is Ann"), "{out}");

    // --env keeps its variables; only the servers point at the mock.
    let (code, out) = cli(&["run", p, "Lifecycle", "--mock", "--env", "staging"]);
    assert_eq!(code, 1, "Bob is not Ann: {out}");
    assert!(out.contains("environment Staging, mock on"), "{out}");
    assert!(out.contains("FAIL is Ann: Expected \"Bob\""), "{out}");
}

#[test]
fn a_request_the_mock_cannot_answer_fails_its_step_saying_why() {
    let dir = tempfile::tempdir().expect("temp dir");
    project(dir.path(), true);
    let p = dir.path().to_str().expect("utf-8");
    let (code, out) = cli(&["run", p, "Lifecycle", "--mock"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("FAIL Delete user (404"), "{out}");
    assert!(
        out.contains("mock: DELETE /users/ann: “Delete user” has no examples yet"),
        "{out}"
    );
}

#[test]
fn mock_options_are_checked() {
    let (code, out) = cli(&["mock", "somewhere", "Lifecycle"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("mock serves the whole project"), "{out}");
    let (code, out) = cli(&["run", "somewhere", "--mock", "--port", "http"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--port needs a port number"), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn mock_serves_the_project_and_prints_each_request() {
    let dir = tempfile::tempdir().expect("temp dir");
    project(dir.path(), false);
    let mut child = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"))
        .args(["mock", dir.path().to_str().expect("utf-8"), "--port", "0"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the mock");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let first = lines.next().expect("a line").expect("utf-8");
    assert!(
        first.starts_with("Mock of Designed ü😀 on http://127.0.0.1:"),
        "{first}"
    );
    assert!(first.contains("(2 requests with examples)"), "{first}");
    let base = first
        .split(" on ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .expect("address")
        .to_owned();

    let response = Client::default()
        .execute(&Request::new(HttpMethod::Get, format!("{base}/users/zoe")))
        .await
        .expect("send");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, br#"{"name": "Ann"}"#);
    let logged = lines.next().expect("a line").expect("utf-8");
    assert_eq!(logged, "200 GET /users/zoe → Get user / anyone");
    child.kill().expect("stop the mock");
    // Reaping it; its exit status after the kill says nothing.
    let _ = child.wait();
}
