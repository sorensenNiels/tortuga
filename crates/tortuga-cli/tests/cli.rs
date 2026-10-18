//! `tortuga-cli` end to end (R-CLI-1..3), against a mock server.

use std::path::Path;
use std::process::Command;

use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::Step;
use tortuga_core::{Auth, Collection, HttpMethod, Request};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FS: FsStorage = FsStorage;

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .and(header("x-api-key", "k-1"))
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
    server
}

/// A project whose login needs the secret `api_key`, then gets `{{user}}` with the token.
fn project(dir: &Path, base: &str) {
    let col = Collection::create(&FS, dir, "Shop").expect("create");
    let mut manifest = col.manifest.clone();
    manifest.secrets = vec!["api_key".into()];
    col.save_config(&FS, &manifest, &[]).expect("config");
    let mut login = Request::new(HttpMethod::Post, format!("{base}/login"));
    login.name = "Log in".into();
    login
        .headers
        .push(tortuga_core::KeyValue::new("X-Api-Key", "{{api_key}}"));
    login.capture.insert("token".into(), "$.token".into());
    Collection::open(&FS, dir)
        .expect("open")
        .new_request(&FS, Path::new(""), &login)
        .expect("login");
    let mut get = Request::new(HttpMethod::Get, format!("{base}/users/{{{{user}}}}"));
    get.name = "Get user".into();
    get.auth = Some(Auth::Bearer {
        token: "{{token}}".into(),
    });
    get.scripts.tests = r#"test("found", () => expect(res.status).toBe(200));"#.into();
    Collection::open(&FS, dir)
        .expect("open")
        .new_request(&FS, Path::new(""), &get)
        .expect("get");
    let col = Collection::open(&FS, dir).expect("open");
    let id = col.new_workflow(&FS, "Lifecycle").expect("workflow");
    let col = Collection::open(&FS, dir).expect("open");
    let mut wf = col.workflow(&id).expect("loaded").clone();
    wf.steps = vec![
        Step::new(Path::new("log-in.toml")),
        Step::new(Path::new("get-user.toml")),
    ];
    col.save_workflow(&FS, &wf).expect("save");
}

fn cli(args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"));
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run tortuga-cli");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_a_workflow_with_secrets_data_and_reports() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let project_dir = dir.path().join("shop");
    project(&project_dir, &server.uri());
    let p = project_dir.to_str().expect("utf-8");

    // R-CLI-2: the secret comes from TORTUGA_VAR_api_key, the user from --var.
    let (code, stdout, stderr) = cli(
        &["run", p, "Lifecycle", "--var", "user=ann"],
        &[("TORTUGA_VAR_api_key", "k-1")],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("ok   Log in (200 OK"), "{stdout}");
    assert!(stdout.contains("ok   found"), "{stdout}");
    assert!(stdout.contains("PASSED: Lifecycle"), "{stdout}");

    // R-FLOW-4, R-CLI-3: a data file with a user that doesn't exist; a JUnit report.
    let data = dir.path().join("users.csv");
    std::fs::write(&data, "user\nann\nbob\n").expect("data");
    let report = dir.path().join("report.xml");
    let (code, stdout, _) = cli(
        &[
            "run",
            p,
            "lifecycle",
            "--data",
            data.to_str().expect("utf-8"),
            "--report",
            "junit",
            "--out",
            report.to_str().expect("utf-8"),
        ],
        &[("TORTUGA_VAR_api_key", "k-1")],
    );
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("Row 2: user=bob"), "{stdout}");
    assert!(
        stdout.contains("FAIL found: Expected 404 to be 200"),
        "{stdout}"
    );
    let xml = std::fs::read_to_string(&report).expect("report");
    assert!(
        xml.contains(r#"<testsuite name="Lifecycle (row 2: user=bob)" tests="2" failures="1""#),
        "{xml}"
    );

    // Without the secret the login is refused: a failed step, not a setup error.
    let (code, stdout, _) = cli(&["run", p, "Lifecycle", "--var", "user=ann"], &[]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("api_key"), "{stdout}");

    // A single request; the whole project.
    let (code, stdout, _) = cli(
        &[
            "run",
            p,
            "log-in",
            "--report",
            "json",
            "--out",
            report.to_str().expect("utf-8"),
        ],
        &[("TORTUGA_VAR_api_key", "k-1")],
    );
    assert_eq!(code, 0, "{stdout}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).expect("json")).expect("parse");
    assert_eq!(json["passed"], true);
    let (code, stdout, _) = cli(
        &["run", p],
        &[("TORTUGA_VAR_api_key", "k-1"), ("TORTUGA_VAR_user", "ann")],
    );
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("Shop — Shop"), "{stdout}");
}

/// R-CLI-1: usage and setup errors exit with 2 and say what is wrong.
#[test]
fn errors_exit_with_2() {
    let dir = tempfile::tempdir().expect("temp dir");
    let p = dir.path().to_str().expect("utf-8");
    let (code, _, stderr) = cli(&["run", p], &[]);
    assert_eq!(code, 2);
    assert!(stderr.contains("not a Tortuga project"), "{stderr}");

    Collection::create(&FS, dir.path(), "Empty").expect("create");
    let (code, _, stderr) = cli(&["run", p, "nothing"], &[]);
    assert_eq!(code, 2);
    assert!(
        stderr.contains("'nothing' is not a workflow, folder or request"),
        "{stderr}"
    );
    let (code, _, stderr) = cli(&["run", p, "--env", "Prod"], &[]);
    assert_eq!(code, 2);
    assert!(stderr.contains("no environment 'Prod'"), "{stderr}");
    let (code, _, _) = cli(&["frobnicate"], &[]);
    assert_eq!(code, 2);
    let (code, stdout, _) = cli(&["--version"], &[]);
    assert_eq!((code, stdout.starts_with("tortuga-cli ")), (0, true));
}
