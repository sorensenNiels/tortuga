//! Workflows on disk, run against a mock server (R-FLOW-1, R-FLOW-3, R-FLOW-4).

use std::collections::BTreeMap;
use std::path::Path;

use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::{self, Progress, RunOptions, Step, Workflow};
use tortuga_core::{Client, Collection, HttpMethod, Request};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FS: FsStorage = FsStorage;

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "token": "t-1" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/ann"))
        .and(header("authorization", "Bearer t-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "name": "Ann" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/bob"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    server
}

/// A project with "auth/login.toml" (captures `token`) and "users/get.toml"
/// (uses `{{token}}` and `{{user}}`), and a workflow running both.
fn project(dir: &Path, base: &str) -> Collection {
    let col = Collection::create(&FS, dir, "Shop").expect("create");
    let auth = col.new_folder(&FS, Path::new(""), "Auth").expect("folder");
    let mut login = Request::new(HttpMethod::Post, format!("{base}/login"));
    login.name = "Log in".into();
    login.capture.insert("token".into(), "$.token".into());
    Collection::open(&FS, dir)
        .expect("open")
        .new_request(&FS, &auth, &login)
        .expect("login");
    let users = Collection::open(&FS, dir)
        .expect("open")
        .new_folder(&FS, Path::new(""), "Users")
        .expect("folder");
    let mut get = Request::new(HttpMethod::Get, format!("{base}/users/{{{{user}}}}"));
    get.name = "Get user".into();
    get.auth = Some(tortuga_core::Auth::Bearer {
        token: "{{token}}".into(),
    });
    get.scripts.tests = r#"test("found", () => expect(res.status).toBe(200));"#.into();
    Collection::open(&FS, dir)
        .expect("open")
        .new_request(&FS, &users, &get)
        .expect("get");
    let col = Collection::open(&FS, dir).expect("open");
    let id = col.new_workflow(&FS, "User lifecycle").expect("workflow");
    let mut wf = Collection::open(&FS, dir)
        .expect("open")
        .workflow(&id)
        .expect("loaded")
        .clone();
    wf.steps = vec![
        Step::new(Path::new("auth/log-in.toml")),
        Step {
            tests: r#"test("named", () => expect(res.json().name).toBe(vars.get("name")));"#.into(),
            ..Step::new(Path::new("users/get-user.toml"))
        },
    ];
    let col = Collection::open(&FS, dir).expect("open");
    col.save_workflow(&FS, &wf).expect("save");
    Collection::open(&FS, dir).expect("open")
}

fn row(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[tokio::test]
async fn data_driven_run_with_captures() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), &server.uri());
    assert_eq!(col.workflows.len(), 1);
    assert!(
        col.items.iter().all(|i| i.name() != "workflows"),
        "workflows/ is not a folder of requests"
    );
    let wf = &col.workflows[0];
    let options = RunOptions {
        rows: vec![
            row(&[("user", "ann"), ("name", "Ann")]),
            row(&[("user", "bob"), ("name", "Bob")]),
        ],
        ..RunOptions::default()
    };
    let mut events = Vec::new();
    let result = workflow::run(&Client::default(), &col, wf, &options, |p| {
        events.push(match p {
            Progress::Started { iteration, step } => format!("start {iteration}.{step}"),
            Progress::Finished { iteration, step } => format!("done {iteration} {}", step.name),
        });
    })
    .await;

    assert_eq!(
        events,
        [
            "start 0.0",
            "done 0 Log in",
            "start 0.1",
            "done 0 Get user",
            "start 1.0",
            "done 1 Log in",
            "start 1.1",
            "done 1 Get user"
        ]
    );
    assert_eq!(result.iterations.len(), 2);
    let ann = &result.iterations[0];
    assert!(
        !ann.failed(),
        "{:?}",
        ann.steps
            .iter()
            .map(|s| s.outcome.failure())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ann.steps[1].outcome.tests.len(),
        2,
        "the request's test and the step's"
    );
    let bob = &result.iterations[1];
    assert!(bob.failed());
    assert_eq!(
        bob.steps[1].outcome.failure().as_deref(),
        Some("test “found” failed: Expected 404 to be 200")
    );
    assert!(!result.passed());
    assert_eq!(result.counts(), (2, 2, 1), "Bob's two tests fail");
}

#[tokio::test]
async fn failures_stop_unless_told_to_continue() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), &server.uri());
    let mut wf = col.workflows[0].clone();
    wf.steps.insert(0, Step::new(Path::new("gone.toml")));
    let run = |wf: Workflow| {
        let col = col.clone();
        async move {
            let options = RunOptions {
                rows: vec![row(&[("user", "ann"), ("name", "Ann")])],
                ..RunOptions::default()
            };
            workflow::run(&Client::default(), &col, &wf, &options, |_| {}).await
        }
    };
    let stopped = run(wf.clone()).await;
    assert_eq!(stopped.iterations[0].steps.len(), 1);
    assert_eq!(stopped.iterations[0].skipped, 2);
    assert!(
        stopped.iterations[0].steps[0]
            .outcome
            .failure()
            .is_some_and(|f| f.contains("not in the project"))
    );

    wf.continue_on_failure = true;
    let continued = run(wf).await;
    assert_eq!(continued.iterations[0].steps.len(), 3);
    assert!(!continued.iterations[0].steps[2].outcome.failed());
}

/// R-FLOW-1: steps follow a request renamed or moved in Tortuga.
#[test]
fn steps_follow_renames() {
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), "http://x.test");
    let new = col
        .move_to(&FS, Path::new("users/get-user.toml"), Path::new("auth"))
        .expect("move");
    let col = Collection::open(&FS, dir.path()).expect("open");
    col.retarget_workflows(&FS, Path::new("users/get-user.toml"), &new)
        .expect("retarget");
    let col = Collection::open(&FS, dir.path()).expect("open");
    assert_eq!(col.workflows[0].steps[1].request, "auth/get-user.toml");
}

/// R-CLI-1: a folder runs its requests in order.
#[test]
fn folder_workflows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), "http://x.test");
    let all = Workflow::from_folder(&col, Path::new(""));
    let steps: Vec<&str> = all.steps.iter().map(|s| s.request.as_str()).collect();
    assert_eq!(steps, ["auth/log-in.toml", "users/get-user.toml"]);
    assert_eq!(
        Workflow::from_folder(&col, Path::new("users")).name,
        "Users"
    );
    assert_eq!(
        Workflow::single(&col, Path::new("auth/log-in.toml")).name,
        "Log in"
    );
}
