//! Workflows on disk, run against a mock server (R-FLOW-1, R-FLOW-3, R-FLOW-4,
//! R-FLOW-6, R-FLOW-10 to R-FLOW-12, R-FLOW-15).

use std::collections::BTreeMap;
use std::path::Path;

use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::{
    self, FlowEnd, Progress, Repeat, RunOptions, RunResult, Step, Workflow,
};
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
            Progress::Started {
                iteration, step, ..
            } => format!("start {iteration}.{step}"),
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
    assert_eq!(stopped.iterations[0].skipped.len(), 2);
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

/// R-SET-2: a workflow run sends the workspace's headers and auth to requests whose
/// project and folders set none.
#[tokio::test]
async fn runs_use_the_workspace_headers_and_auth() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .and(header("authorization", "Bearer ws-token"))
        .and(header("x-tenant", "acme"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let col = Collection::create(&FS, dir.path(), "Ping").expect("create");
    let mut ping = Request::new(HttpMethod::Get, format!("{}/ping", server.uri()));
    ping.name = "Ping".into();
    col.new_request(&FS, Path::new(""), &ping).expect("ping");
    let col = Collection::open(&FS, dir.path()).expect("open");
    let wf = Workflow {
        name: "Ping".into(),
        steps: vec![Step::new(Path::new("ping.toml"))],
        ..Workflow::default()
    };
    let options = RunOptions {
        workspace: row(&[("tenant", "acme")]),
        workspace_defaults: tortuga_core::workspace::WorkspaceDefaults {
            headers: vec![tortuga_core::KeyValue::new("X-Tenant", "{{tenant}}")],
            auth: tortuga_core::Auth::Bearer {
                token: "ws-token".into(),
            },
            ..Default::default()
        },
        ..RunOptions::default()
    };
    let result = workflow::run(&Client::default(), &col, &wf, &options, |_| {}).await;
    let step = &result.iterations[0].steps[0];
    assert!(!step.outcome.failed(), "{:?}", step.outcome.failure());
}

/// Runs the project's workflow for Ann with `login_tests` and `get_tests` as the
/// steps' own tests.
async fn run_with(dir: &Path, login_tests: &str, get_tests: &str) -> RunResult {
    let server = server().await;
    let col = project(dir, &server.uri());
    let mut wf = col.workflows[0].clone();
    wf.steps[0].tests = login_tests.into();
    wf.steps[1].tests = get_tests.into();
    let options = RunOptions {
        rows: vec![row(&[("user", "ann"), ("name", "Ann")])],
        ..RunOptions::default()
    };
    workflow::run(&Client::default(), &col, &wf, &options, |_| {}).await
}

fn names(result: &RunResult) -> Vec<(usize, &str)> {
    result.iterations[0]
        .steps
        .iter()
        .map(|s| (s.index, s.name.as_str()))
        .collect()
}

/// R-FLOW-6: `workflow.next(name)` runs that step next, here to poll until a
/// condition holds; scripts see the iteration in `info` (R-TEST-16).
#[tokio::test]
async fn scripts_choose_the_next_step() {
    let dir = tempfile::tempdir().expect("temp dir");
    let result = run_with(
        dir.path(),
        "",
        r#"const n = Number(vars.get("tries") || 0) + 1;
vars.set("tries", n);
vars.set("where", info.event + " " + info.requestName + " " + info.iteration + "/" + info.iterationCount);
if (n < 3) workflow.next("Get user");"#,
    )
    .await;
    assert_eq!(
        names(&result),
        [
            (0, "Log in"),
            (1, "Get user"),
            (1, "Get user"),
            (1, "Get user")
        ]
    );
    let iteration = &result.iterations[0];
    assert!(!iteration.failed());
    assert_eq!(iteration.ended, None);
    let last = &iteration.steps[3].outcome.vars;
    assert_eq!(last["tries"].as_deref(), Some("3"));
    assert_eq!(last["where"].as_deref(), Some("test Get user 0/1"));
}

/// R-FLOW-6: `workflow.stop()` ends the iteration without a failure; a step name the
/// workflow doesn't have fails the step.
#[tokio::test]
async fn scripts_stop_the_run_or_name_a_missing_step() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stopped = run_with(dir.path().join("a").as_path(), "workflow.stop();", "").await;
    assert_eq!(names(&stopped), [(0, "Log in")]);
    assert_eq!(stopped.iterations[0].ended, Some(FlowEnd::Stopped));
    assert_eq!(stopped.iterations[0].skipped, Vec::<usize>::new());
    assert!(stopped.passed());

    let missing = run_with(
        dir.path().join("b").as_path(),
        "workflow.next(\"Nope ü\");",
        "",
    )
    .await;
    let iteration = &missing.iterations[0];
    assert!(iteration.failed());
    assert_eq!(iteration.skipped.len(), 1);
    assert_eq!(
        iteration.steps[0].outcome.failure().as_deref(),
        Some("workflow.next(\"Nope ü\"): the workflow has no step called “Nope ü”.")
    );
}

/// Runs the project's workflow, changed by `change`, with `options`; the login step
/// counts `n` up by one.
async fn run_counting(
    dir: &Path,
    change: impl FnOnce(&mut Workflow),
    options: RunOptions,
) -> RunResult {
    let server = server().await;
    let col = project(dir, &server.uri());
    let mut wf = col.workflows[0].clone();
    wf.steps[0].tests = r#"vars.set("n", Number(vars.get("n") || 0) + 1);
vars.set("seen", info.iteration + "/" + info.iterationCount + " " + vars.get("user"));"#
        .into();
    change(&mut wf);
    workflow::run(&Client::default(), &col, &wf, &options, |_| {}).await
}

/// The `n` the login step set in each iteration.
fn counts(result: &RunResult) -> Vec<&str> {
    result
        .iterations
        .iter()
        .map(|i| i.steps[0].outcome.vars["n"].as_deref().unwrap_or_default())
        .collect()
}

fn ann() -> BTreeMap<String, String> {
    row(&[("user", "ann"), ("name", "Ann")])
}

/// R-FLOW-8: without a data file a workflow runs `iterations` times, and scripts see
/// how many; with one, its rows decide.
#[tokio::test]
async fn iterations_repeat_the_run_unless_a_data_file_decides() {
    let dir = tempfile::tempdir().expect("temp dir");
    let options = RunOptions {
        overrides: ann(),
        ..RunOptions::default()
    };
    let three = run_counting(&dir.path().join("a"), |w| w.iterations = 3, options).await;
    assert_eq!(three.iterations.len(), 3);
    assert!(three.passed());
    // Each iteration starts without the values of the last (R-FLOW-3).
    assert_eq!(counts(&three), ["1", "1", "1"]);
    let seen = three.iterations[2].steps[0].outcome.vars["seen"].as_deref();
    assert_eq!(seen, Some("2/3 ann"));

    let options = RunOptions {
        rows: vec![ann()],
        ..RunOptions::default()
    };
    let rows = run_counting(&dir.path().join("b"), |w| w.iterations = 3, options).await;
    assert_eq!(rows.iterations.len(), 1);
}

/// R-FLOW-8: `delay_ms` passes between steps, also between iterations, and not
/// before the first.
#[tokio::test]
async fn the_delay_passes_between_steps() {
    let dir = tempfile::tempdir().expect("temp dir");
    let options = RunOptions {
        overrides: ann(),
        ..RunOptions::default()
    };
    let start = std::time::Instant::now();
    let result = run_counting(
        dir.path(),
        |w| {
            w.iterations = 2;
            w.delay_ms = 150;
        },
        options,
    )
    .await;
    let took = start.elapsed();
    assert_eq!(result.iterations.len(), 2);
    // Four steps, three waits.
    assert!(took >= std::time::Duration::from_millis(450), "{took:?}");
}

/// R-FLOW-9, D66: a workflow that keeps variable values starts from the project's
/// runtime variables and carries what it sets into the next iterations, below the
/// data row; without it, runtime variables are not seen.
#[tokio::test]
async fn kept_variables_carry_over_below_the_data_row() {
    let dir = tempfile::tempdir().expect("temp dir");
    let runtime = row(&[("n", "5"), ("user", "bob"), ("only", "ü😀")]);
    let options = RunOptions {
        runtime: runtime.clone(),
        rows: vec![ann(), ann()],
        ..RunOptions::default()
    };
    let kept = run_counting(&dir.path().join("a"), |w| w.keep_variables = true, options).await;
    assert_eq!(counts(&kept), ["6", "7"]);
    // The data row's user wins over the kept one.
    assert!(
        kept.passed(),
        "{:?}",
        kept.iterations[0].steps[1].outcome.failure()
    );
    let seen = kept.iterations[1].steps[0].outcome.vars["seen"].as_deref();
    assert_eq!(seen, Some("1/2 ann"));

    let options = RunOptions {
        runtime,
        rows: vec![ann(), ann()],
        ..RunOptions::default()
    };
    let clean = run_counting(&dir.path().join("b"), |_| {}, options).await;
    assert_eq!(counts(&clean), ["1", "1"]);
}

/// R-WS-8, P6: a workspace secret is sent, but hidden in the step's request as shown.
#[tokio::test]
async fn workspace_secrets_are_hidden_in_step_results() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), &server.uri());
    let options = RunOptions {
        workspace: row(&[("user", "ann")]),
        secret_names: ["user".to_owned()].into(),
        ..RunOptions::default()
    };

    let result = workflow::run(
        &Client::default(),
        &col,
        &col.workflows[0],
        &options,
        |_| {},
    )
    .await;

    let get = &result.iterations[0].steps[1].outcome;
    let url = |r: Option<&Request>| r.map(|r| r.url.clone()).unwrap_or_default();
    assert!(url(get.sent.as_ref()).ends_with("/users/ann"));
    assert!(
        url(get.shown()).ends_with("/users/<secret:user>"),
        "{}",
        url(get.shown())
    );
}

/// A project with "job-status.toml" (`GET /job`) and "next.toml" (`GET /next`), and a
/// workflow that polls the first until its state is "done", then sends the second.
async fn polling(server: &MockServer, dir: &Path, repeat: Repeat, tests: &str) -> Workflow {
    Mock::given(method("GET"))
        .and(path("/next"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
    Collection::create(&FS, dir, "Jobs").expect("create");
    for (name, at) in [("Job status", "/job"), ("Next", "/next")] {
        let mut request = Request::new(HttpMethod::Get, format!("{}{at}", server.uri()));
        request.name = name.into();
        Collection::open(&FS, dir)
            .expect("open")
            .new_request(&FS, Path::new(""), &request)
            .expect("request");
    }
    Workflow {
        name: "Poll".into(),
        steps: vec![
            Step {
                tests: tests.into(),
                repeat: Some(repeat),
                ..Step::new(Path::new("job-status.toml"))
            },
            Step::new(Path::new("next.toml")),
        ],
        ..Workflow::default()
    }
}

fn job_state(state: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "state": state }))
}

const UNTIL_DONE: &str = r#"vars.set("polls", String(Number(vars.get("polls") || 0) + 1));
test("done", () => expect(res.json().state).toBe("done"));"#;

/// R-FLOW-10: a repeating step is sent until a try passes; each try sees what the
/// tries before it set, and the result is the last try with the count.
#[tokio::test]
async fn a_step_repeats_until_it_passes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("running"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("done"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let repeat = Repeat {
        times: 5,
        wait_ms: 10,
    };
    let wf = polling(&server, dir.path(), repeat, UNTIL_DONE).await;
    let col = Collection::open(&FS, dir.path()).expect("open");
    let mut attempts = Vec::new();
    let result = workflow::run(&Client::default(), &col, &wf, &RunOptions::default(), |p| {
        if let Progress::Started { step, attempt, .. } = p {
            attempts.push((step, attempt));
        }
    })
    .await;
    assert!(
        result.passed(),
        "{:?}",
        result.iterations[0].steps[0].outcome.failure()
    );
    assert_eq!(attempts, [(0, 1), (0, 2), (0, 3), (1, 1)]);
    let steps = &result.iterations[0].steps;
    assert_eq!(steps.len(), 2, "one result per step, not per try");
    assert_eq!(steps[0].tries.map(|t| t.count), Some(3));
    assert_eq!(steps[1].tries, None);
    assert_eq!(
        steps[0].outcome.vars.get("polls"),
        Some(&Some("3".to_owned()))
    );
}

/// R-FLOW-10: when no try passes, the step fails after the last one, having waited
/// between tries, and the run stops as after any failed step (R-FLOW-3).
#[tokio::test]
async fn a_step_that_never_passes_fails_after_its_tries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("running"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let repeat = Repeat {
        times: 3,
        wait_ms: 120,
    };
    let wf = polling(&server, dir.path(), repeat, UNTIL_DONE).await;
    let col = Collection::open(&FS, dir.path()).expect("open");
    let start = std::time::Instant::now();
    let result = workflow::run(
        &Client::default(),
        &col,
        &wf,
        &RunOptions::default(),
        |_| {},
    )
    .await;
    let took = start.elapsed();
    let iteration = &result.iterations[0];
    let polled = &iteration.steps[0];
    assert!(polled.outcome.failed());
    assert_eq!(polled.tries.map(|t| t.count), Some(3));
    assert!(
        polled
            .outcome
            .errors
            .iter()
            .any(|e| e == "No try passed in 3 tries."),
        "{:?}",
        polled.outcome.errors
    );
    assert_eq!(iteration.skipped.len(), 1);
    // Three tries, two waits.
    assert!(took >= std::time::Duration::from_millis(240), "{took:?}");
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 3);
}

/// R-FLOW-10, R-FLOW-6: a try whose script says what runs next ends the repeating.
#[tokio::test]
async fn a_script_choosing_the_next_step_ends_the_repeating() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("failed"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let tests = r#"if (res.json().state === "failed") workflow.stop();
test("done", () => expect(res.json().state).toBe("done"));"#;
    let wf = polling(&server, dir.path(), Repeat::default(), tests).await;
    let col = Collection::open(&FS, dir.path()).expect("open");
    let result = workflow::run(
        &Client::default(),
        &col,
        &wf,
        &RunOptions::default(),
        |_| {},
    )
    .await;
    let steps = &result.iterations[0].steps;
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].tries.map(|t| t.count), Some(1));
    assert!(steps[0].outcome.failed(), "its test still failed");
}

/// The polling project's workflow with a third step, "Clean up" (`GET /next`, always
/// run), after a status step whose test fails while the job is "running".
async fn with_cleanup(server: &MockServer, dir: &Path, status_tests: &str) -> Workflow {
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("running"))
        .mount(server)
        .await;
    let mut wf = polling(server, dir, Repeat::default(), status_tests).await;
    wf.steps[0].repeat = None;
    wf.steps.push(Step {
        name: Some("Clean up".into()),
        always: true,
        ..Step::new(Path::new("next.toml"))
    });
    wf
}

fn ran(result: &RunResult) -> Vec<&str> {
    result.iterations[0]
        .steps
        .iter()
        .map(|s| s.name.as_str())
        .collect()
}

/// R-FLOW-11: after a failed step, an always-run step still runs and the others are
/// skipped; after `workflow.stop()` it runs too, and nothing counts as skipped.
#[tokio::test]
async fn always_run_steps_run_after_a_failure_or_a_stop() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let wf = with_cleanup(&server, &dir.path().join("a"), UNTIL_DONE).await;
    let col = Collection::open(&FS, &dir.path().join("a")).expect("open");
    let result = workflow::run(
        &Client::default(),
        &col,
        &wf,
        &RunOptions::default(),
        |_| {},
    )
    .await;
    assert_eq!(ran(&result), ["Job status", "Clean up"]);
    assert_eq!(result.iterations[0].skipped, [1]);
    assert!(!result.iterations[0].steps[1].failed());
    assert!(!result.passed(), "the failed step still fails the run");

    let stop = "workflow.stop();";
    let wf = with_cleanup(&server, &dir.path().join("b"), stop).await;
    let col = Collection::open(&FS, &dir.path().join("b")).expect("open");
    let result = workflow::run(
        &Client::default(),
        &col,
        &wf,
        &RunOptions::default(),
        |_| {},
    )
    .await;
    assert_eq!(ran(&result), ["Job status", "Clean up"]);
    assert_eq!(result.iterations[0].skipped, Vec::<usize>::new());
    assert_eq!(result.iterations[0].ended, Some(FlowEnd::Stopped));
    assert!(result.passed());
}

/// R-FLOW-12: a step whose condition holds is not sent and neither passes nor
/// fails; one that doesn't hold is sent; a condition that throws fails the step.
#[tokio::test]
async fn steps_are_skipped_when_their_condition_holds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/job"))
        .respond_with(job_state("done"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut wf = polling(&server, dir.path(), Repeat::default(), "").await;
    wf.steps[0].repeat = None;
    let col = Collection::open(&FS, dir.path()).expect("open");
    let options = RunOptions {
        overrides: row(&[("role", "guest ü😀")]),
        ..RunOptions::default()
    };
    let run = |wf: Workflow| {
        let (col, options) = (&col, &options);
        async move { workflow::run(&Client::default(), col, &wf, options, |_| {}).await }
    };

    wf.steps[0].skip_when = r#"vars.get("role") !== "admin" // only admins"#.into();
    let result = run(wf.clone()).await;
    let steps = &result.iterations[0].steps;
    assert_eq!(
        steps[0].skipped_by.as_deref(),
        Some(wf.steps[0].skip_when.as_str())
    );
    assert!(!steps[0].failed());
    assert!(steps[1].skipped_by.is_none());
    assert!(result.passed());
    let sent = server.received_requests().await.expect("recorded");
    assert!(
        sent.iter().all(|r| r.url.path() != "/job"),
        "skipped: not sent"
    );

    wf.steps[0].skip_when = r#"vars.get("role") === "admin""#.into();
    let result = run(wf.clone()).await;
    assert!(result.iterations[0].steps[0].skipped_by.is_none());
    assert!(result.passed());

    wf.steps[0].skip_when = "vars.get(".into();
    let result = run(wf.clone()).await;
    let first = &result.iterations[0].steps[0];
    assert!(first.failed());
    assert!(
        first
            .outcome
            .errors
            .iter()
            .any(|e| e.starts_with("Skip when: ")),
        "{:?}",
        first.outcome.errors
    );
}

/// R-FLOW-15: users run the steps side by side, take the data rows in turn, and each
/// step's sends, failures and times are counted.
#[tokio::test]
async fn a_load_test_counts_every_users_sends() {
    use tortuga_core::workflow::{LoadLimit, LoadOptions};
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project(dir.path(), &server.uri());
    let wf = &col.workflows[0];
    let options = RunOptions {
        rows: vec![
            row(&[("user", "ann"), ("name", "Ann")]),
            row(&[("user", "bob"), ("name", "Bob")]),
        ],
        ..RunOptions::default()
    };
    let load = LoadOptions {
        users: 2,
        limit: LoadLimit::Iterations(2),
        ramp_up: std::time::Duration::ZERO,
    };
    let mut updates = 0;
    let report = workflow::load(&Client::default(), &col, wf, &options, load, |_| {
        updates += 1;
    })
    .await;
    assert_eq!(report.iterations, 4);
    assert_eq!(updates, 4, "after every iteration of every user");
    let names: Vec<&str> = report.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Log in", "Get user"]);
    assert_eq!((report.steps[0].sends, report.steps[0].failures), (4, 0));
    // Bob's rows get a 404: half the users' iterations.
    assert_eq!((report.steps[1].sends, report.steps[1].failures), (4, 2));
    assert!(report.steps[0].median_ms().is_some());
    assert_eq!(report.sends(), (8, 2));

    let timed = LoadOptions {
        users: 3,
        limit: LoadLimit::Duration(std::time::Duration::from_millis(200)),
        ramp_up: std::time::Duration::from_millis(60),
    };
    let report = workflow::load(&Client::default(), &col, wf, &options, timed, |_| {}).await;
    assert!(report.iterations >= 3, "{}", report.iterations);
    assert!(report.elapsed >= std::time::Duration::from_millis(200));
    assert!(report.sends_per_second() > 0.0);
}
