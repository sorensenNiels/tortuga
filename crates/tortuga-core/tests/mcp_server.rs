//! The MCP server through a real MCP client (R-MCP-1 to R-MCP-4): tools listed, a request
//! sent against wiremock with this machine's secret hidden, an edit written to disk.

use std::collections::BTreeMap;
use std::path::Path;

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
        let server = TortugaMcp::new(project, Client::default())
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
