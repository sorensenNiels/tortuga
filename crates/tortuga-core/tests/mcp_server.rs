//! The MCP server through a real MCP client (R-MCP-1 to R-MCP-4): tools listed, a request
//! sent against wiremock with this machine's secret hidden, an edit written to disk.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RoleClient, RunningService};
use serde_json::{Value, json};
use tortuga_core::mcp::McpProject;
use tortuga_core::mcp::server::TortugaMcp;
use tortuga_core::storage::FsStorage;
use tortuga_core::{Client, Collection, HttpMethod, KeyValue, Request};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// ASCII: header values travel as bytes and wiremock only matches text. Non-ASCII values
/// are hidden too (the `mcp` unit tests).
const SECRET: &str = "tok-7f3a9c2e";

/// A project whose "Get user" sends `Authorization: Bearer {{token}}` to `{{base}}/users/1`
/// and tests the status; Dev points `base` at `uri`.
fn project(dir: &Path, uri: &str) {
    let col = Collection::create(&FsStorage, dir, "Shop").expect("create");
    let mut get = Request::new(HttpMethod::Get, "{{base}}/users/1");
    get.name = "Get user".into();
    get.headers
        .push(KeyValue::new("Authorization", "Bearer {{token}}"));
    get.scripts.tests = r#"
        test("ok", () => expect(res.status).toBe(200));
        console.log("got " + res.json().echo);
    "#
    .into();
    col.new_request(&FsStorage, Path::new(""), &get)
        .expect("request");
    std::fs::create_dir_all(dir.join("environments")).expect("dir");
    std::fs::write(
        dir.join("environments/dev.toml"),
        format!("name = \"Dev\"\n[variables]\nbase = \"{uri}\"\n"),
    )
    .expect("env");
}

async fn connect(project: McpProject) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let projects = Arc::new(RwLock::new(vec![project]));
        let server = TortugaMcp::new(projects, Client::default(), Arc::new(|_| {}))
            .serve(server_side)
            .await
            .expect("server starts");
        // The test ends by cancelling the client, which ends this.
        let _ = server.waiting().await;
    });
    ().serve(client_side).await.expect("client connects")
}

async fn call(client: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments are an object")
    };
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
        .await
        .expect("tool call")
}

fn text(result: &CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map_or("", |t| t.text.as_str())
}

/// R-MCP-1, R-MCP-3: a send fills in this machine's secret, runs the tests, and never
/// shows the secret, not even where the server echoes it.
#[tokio::test]
async fn sends_a_request_and_hides_the_secret() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/1"))
        .and(header("authorization", format!("Bearer {SECRET}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "echo": SECRET })))
        .mount(&api)
        .await;
    let dir = tempfile::tempdir().expect("dir");
    project(dir.path(), &api.uri());
    let client = connect(McpProject {
        root: dir.path().to_owned(),
        environment: Some("Dev".into()),
        values: BTreeMap::from([("token".into(), SECRET.into())]),
        ..McpProject::default()
    })
    .await;

    let tools = client.list_all_tools().await.expect("tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for tool in [
        "list_project",
        "get_request",
        "list_environments",
        "send_request",
        "create_request",
        "save_request",
        "create_folder",
        "rename_folder",
        "move_item",
        "delete_item",
    ] {
        assert!(names.contains(&tool), "{tool} in {names:?}");
    }

    let sent = call(&client, "send_request", json!({ "path": "get-user" })).await;
    let shown = text(&sent);
    assert_ne!(sent.is_error, Some(true), "{shown}");
    let outcome: Value = serde_json::from_str(shown).expect("json");
    assert_eq!(outcome["response"]["status"], 200, "{shown}");
    assert_eq!(outcome["tests"][0]["passed"], true, "{shown}");
    assert_eq!(outcome["console"][0], "got {{token}}", "{shown}");
    assert!(!shown.contains(SECRET), "{shown}");

    let missing = call(&client, "send_request", json!({ "path": "nope" })).await;
    assert_eq!(missing.is_error, Some(true));
    assert!(
        text(&missing).contains("list_project"),
        "{}",
        text(&missing)
    );

    client.cancel().await.expect("stop");
}

/// R-MCP-4: an edit through the protocol is a file on disk.
#[tokio::test]
async fn an_edit_writes_the_project_files() {
    let dir = tempfile::tempdir().expect("dir");
    project(dir.path(), "http://localhost:1");
    let client = connect(McpProject {
        root: dir.path().to_owned(),
        ..McpProject::default()
    })
    .await;

    let created = call(
        &client,
        "create_request",
        json!({ "request": { "name": "Delete user ü😀", "method": "DELETE", "url": "{{base}}/users/1" } }),
    )
    .await;

    assert_ne!(created.is_error, Some(true), "{}", text(&created));
    let col = Collection::open(&FsStorage, dir.path()).expect("open");
    assert!(col.items.iter().any(|i| i.name() == "Delete user ü😀"));
    client.cancel().await.expect("stop");
}

/// R-MCP-5: with several projects an agent names one; edits and sends are logged.
#[tokio::test]
async fn several_projects_are_picked_by_name_and_actions_logged() {
    let (a, b) = (
        tempfile::tempdir().expect("a"),
        tempfile::tempdir().expect("b"),
    );
    project(a.path(), "http://localhost:1");
    project(b.path(), "http://localhost:1");
    let projects = Arc::new(RwLock::new(vec![
        McpProject {
            name: "Shop".into(),
            root: a.path().to_owned(),
            ..McpProject::default()
        },
        McpProject {
            name: "Billing ü😀".into(),
            root: b.path().to_owned(),
            ..McpProject::default()
        },
    ]));
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&log);
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let shared = Arc::clone(&projects);
    tokio::spawn(async move {
        let on_action = Arc::new(move |a| seen.lock().expect("log").push(a));
        let server = TortugaMcp::new(shared, Client::default(), on_action)
            .serve(server_side)
            .await
            .expect("server starts");
        // The test ends by cancelling the client, which ends this.
        let _ = server.waiting().await;
    });
    let client = ().serve(client_side).await.expect("client connects");

    let all: Value =
        serde_json::from_str(text(&call(&client, "list_project", json!({})).await)).expect("json");
    assert_eq!(all.as_array().map(Vec::len), Some(2));
    let unnamed = call(&client, "get_request", json!({ "path": "get-user" })).await;
    assert_eq!(unnamed.is_error, Some(true));
    assert!(text(&unnamed).contains("Billing ü😀"), "{}", text(&unnamed));
    let named = call(
        &client,
        "create_folder",
        json!({ "project": "billing ü😀", "name": "Invoices" }),
    )
    .await;
    assert_ne!(named.is_error, Some(true), "{}", text(&named));
    assert!(b.path().join("invoices").exists());
    assert!(!a.path().join("invoices").exists());
    let refused = call(
        &client,
        "send_request",
        json!({ "project": "Shop", "path": "nope" }),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));

    let log = log.lock().expect("log").clone();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0].project, "Billing ü😀");
    assert_eq!(log[0].summary, "Create folder Invoices");
    assert!(!log[0].failed);
    assert!(
        log[1].failed && log[1].summary.starts_with("Sent"),
        "{log:?}"
    );
    client.cancel().await.expect("stop");
}

/// R-MCP-5, D75: the app's HTTP server only answers at /mcp, with the token.
#[tokio::test]
async fn the_http_server_wants_its_token() {
    let dir = tempfile::tempdir().expect("dir");
    project(dir.path(), "http://localhost:1");
    let projects = Arc::new(RwLock::new(vec![McpProject {
        name: "Shop".into(),
        root: dir.path().to_owned(),
        ..McpProject::default()
    }]));
    let running =
        tortuga_core::mcp::server::http::start(0, projects, Client::default(), Arc::new(|_| {}))
            .await
            .expect("starts");
    let http = reqwest::Client::new();
    let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": { "name": "test", "version": "1" } } });
    let post = |token: Option<&str>, url: String| {
        let mut request = http
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(initialize.to_string());
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send()
    };

    let without = post(None, running.url()).await.expect("answer");
    let wrong = post(Some("nope"), running.url()).await.expect("answer");
    let elsewhere = post(Some(running.token()), running.url().replace("/mcp", "/x"))
        .await
        .expect("answer");
    let with = post(Some(running.token()), running.url())
        .await
        .expect("answer");

    assert_eq!(without.status(), 401);
    assert_eq!(wrong.status(), 401);
    assert_eq!(elsewhere.status(), 404);
    assert_eq!(with.status(), 200);
    assert!(with.text().await.expect("body").contains("tortuga"));
    assert!(running.url().starts_with("http://127.0.0.1:"));

    let stopped_port = running.port();
    drop(running);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", stopped_port)).is_err(),
        "stopped when dropped"
    );
}
