use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::*;
use crate::storage::FsStorage;
use crate::{HttpMethod, Request};

const FS: FsStorage = FsStorage;

/// A project "Shop ü😀" with a folder `users` holding "Get user ü😀" (GET) and
/// "Create user" (POST), a workflow using the GET, a secret variable `token`, and the
/// environments Dev and Prod (protected).
fn project(dir: &Path) -> McpProject {
    let col = Collection::create(&FS, dir, "Shop ü😀").expect("create");
    let users = col.new_folder(&FS, Path::new(""), "Users").expect("folder");
    let mut get = Request::new(HttpMethod::Get, "{{base}}/users/1");
    get.name = "Get user ü😀".into();
    let get_path = path_text(&col.new_request(&FS, &users, &get).expect("get"));
    let mut post = Request::new(HttpMethod::Post, "{{base}}/users");
    post.name = "Create user".into();
    col.new_request(&FS, &users, &post).expect("post");
    let manifest = dir.join("tortuga.toml");
    let text = std::fs::read_to_string(&manifest).expect("manifest");
    std::fs::write(
        &manifest,
        format!("secrets = [\"token\"]\n{text}\n[variables]\ntoken = \"\"\nretries = \"3\"\n"),
    )
    .expect("manifest");
    std::fs::create_dir_all(dir.join("environments")).expect("dir");
    std::fs::write(
        dir.join("environments/dev.toml"),
        "name = \"Dev\"\n[variables]\nbase = \"http://dev.test\"\n",
    )
    .expect("dev");
    std::fs::write(
        dir.join("environments/prod.toml"),
        "name = \"Prod\"\nprotected = true\n[variables]\nbase = \"http://prod.test\"\n",
    )
    .expect("prod");
    std::fs::create_dir_all(dir.join("workflows")).expect("dir");
    std::fs::write(
        dir.join("workflows/smoke.toml"),
        format!("name = \"Smoke\"\n\n[[steps]]\nrequest = \"{get_path}\"\n"),
    )
    .expect("workflow");
    McpProject {
        root: dir.to_owned(),
        ..McpProject::default()
    }
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

/// The path of "Get user ü😀", as `list_project` gives it.
fn first_request(project: &McpProject) -> String {
    let tree = json(&project.list_project(&FS).expect("list"));
    let users = tree["items"][0]["items"].as_array().expect("users");
    let get = users.iter().find(|r| r["method"] == "GET").expect("GET");
    get["path"].as_str().expect("path").to_owned()
}

/// R-MCP-2: paths stay inside the project, whatever an agent sends.
#[test]
fn paths_outside_the_project_are_refused() {
    assert_eq!(
        relative("./users\\get.toml/").expect("ok"),
        Path::new("users/get.toml")
    );
    assert_eq!(relative("  ").expect("top"), Path::new(""));
    assert_eq!(relative("ü😀/a").expect("ok"), Path::new("ü😀/a"));
    for bad in ["../x", "users/../../x", "/etc/passwd", "C:\\x"] {
        if cfg!(windows) || !bad.starts_with("C:") {
            assert!(relative(bad).is_err(), "{bad}");
        }
    }
}

/// R-MCP-2: the tree, and a request as saved with or without `.toml`.
#[test]
fn lists_the_project_and_reads_a_request() {
    let dir = tempfile::tempdir().expect("dir");
    let project = project(dir.path());

    let tree = json(&project.list_project(&FS).expect("list"));
    assert_eq!(tree["name"], "Shop ü😀");
    assert_eq!(tree["workflows"][0]["name"], "Smoke");

    let path = first_request(&project);
    let request = json(&project.get_request(&FS, &path).expect("get"));
    assert_eq!(request["request"]["name"], "Get user ü😀");
    assert_eq!(request["request"]["method"], "GET");
    let without = path.trim_end_matches(".toml");
    assert_eq!(
        json(&project.get_request(&FS, without).expect("get"))["path"],
        path
    );
    assert!(project.get_request(&FS, "users/nope").is_err());
    assert!(project.get_request(&FS, "../secret.toml").is_err());
}

/// R-MCP-2: secret variables are listed by name, never with a value.
#[test]
fn environments_list_secrets_by_name_only() {
    let dir = tempfile::tempdir().expect("dir");
    let project = McpProject {
        environment: Some("Dev".into()),
        values: BTreeMap::from([("token".into(), "s3cret-token".into())]),
        ..project(dir.path())
    };

    let text = project.list_environments(&FS).expect("environments");
    let envs = json(&text);

    assert_eq!(envs["default"], "Dev");
    assert_eq!(envs["environments"][0]["name"], "Dev");
    assert_eq!(
        envs["environments"][0]["variables"]["base"],
        "http://dev.test"
    );
    assert_eq!(envs["environments"][1]["protected"], true);
    assert_eq!(envs["secret_variables"], json!(["token"]));
    assert_eq!(envs["project_variables"], json!({ "retries": "3" }));
    assert!(!text.contains("s3cret"), "{text}");
}

/// R-MCP-3, R-SRV-9: only safe methods go to a protected environment.
#[test]
fn a_protected_environment_only_takes_safe_methods() {
    let dir = tempfile::tempdir().expect("dir");
    let project = project(dir.path());
    let get = first_request(&project);
    let no_values = BTreeMap::new();

    let prepared = project
        .prepare_send(&FS, &get, Some("prod"), &no_values)
        .expect("GET is fine");
    assert!(prepared.options.environment.is_some());

    let refused = project
        .prepare_send(&FS, "users/create-user", Some("Prod"), &no_values)
        .expect_err("POST is refused");
    assert!(refused.to_string().contains("protected"), "{refused}");
    assert!(
        project
            .prepare_send(&FS, "users/create-user", Some("Dev"), &no_values)
            .is_ok()
    );
    let unknown = project
        .prepare_send(&FS, &get, Some("Staging"), &no_values)
        .expect_err("no such environment");
    assert!(unknown.to_string().contains("Dev"), "{unknown}");
}

/// R-MCP-3: values given for a send win over this machine's.
#[test]
fn values_for_a_send_win() {
    let dir = tempfile::tempdir().expect("dir");
    let project = McpProject {
        values: BTreeMap::from([("user".into(), "ann".into()), ("token".into(), "t".into())]),
        ..project(dir.path())
    };
    let given = BTreeMap::from([("user".to_owned(), "bob".to_owned())]);

    let prepared = project
        .prepare_send(&FS, &first_request(&project), None, &given)
        .expect("prepared");

    assert_eq!(prepared.options.overrides["user"], "bob");
    assert_eq!(prepared.options.overrides["token"], "t");
}

/// R-MCP-4: create, change, rename, move and delete, with workflows following.
#[test]
fn edits_change_the_files_and_workflows_follow() {
    let dir = tempfile::tempdir().expect("dir");
    let project = project(dir.path());
    let get = first_request(&project);

    let created = project
        .create_folder(&FS, "", "Orders ü😀")
        .expect("folder");
    let orders_path = created.strip_prefix("Created ").expect("path").to_owned();
    project
        .create_request(
            &FS,
            &orders_path,
            &json!({ "name": "List orders", "method": "GET", "url": "{{base}}/orders/:id" }),
        )
        .expect("request");
    let col = Collection::open(&FS, dir.path()).expect("open");
    let orders = col
        .folders()
        .into_iter()
        .find(|f| f.name == "Orders ü😀")
        .expect("folder");
    let new_request = orders.items[0].path().to_path_buf();
    let saved = &col.request(&new_request).expect("saved").request;
    assert_eq!(
        saved.path_vars[0].name, "id",
        "path variables follow the URL"
    );

    let mut request = json(&project.get_request(&FS, &get).expect("get"))["request"].clone();
    request["name"] = json!("Fetch user");
    let saved = project.save_request(&FS, &get, &request).expect("save");
    assert_eq!(saved, "Saved users/fetch-user.toml");
    let smoke = std::fs::read_to_string(dir.path().join("workflows/smoke.toml")).expect("wf");
    assert!(smoke.contains("users/fetch-user.toml"), "{smoke}");

    let moved = project
        .move_item(&FS, "users/fetch-user", &orders_path)
        .expect("move");
    assert_eq!(moved, format!("Moved to {orders_path}/fetch-user.toml"));
    let smoke = std::fs::read_to_string(dir.path().join("workflows/smoke.toml")).expect("wf");
    assert!(!smoke.contains("users/fetch-user.toml"), "{smoke}");

    let renamed = project
        .rename_folder(&FS, "users", "People")
        .expect("rename");
    assert_eq!(renamed, "Renamed to people");
    assert!(project.rename_folder(&FS, "", "Top").is_err());

    project.delete_item(&FS, "people").expect("delete");
    assert!(!dir.path().join("people").exists());
    assert!(
        project.delete_item(&FS, "").is_err(),
        "the project itself is not deleted"
    );
    assert!(project.delete_item(&FS, "people").is_err(), "already gone");
}

/// R-MCP-4: a request an agent sends must at least have a name, method and URL.
#[test]
fn a_malformed_request_is_refused_with_a_hint() {
    let dir = tempfile::tempdir().expect("dir");
    let project = project(dir.path());

    let error = project
        .create_request(&FS, "", &json!({ "title": "x" }))
        .expect_err("no name");
    assert!(error.to_string().contains("get_request"), "{error}");
    let blank = project
        .create_request(&FS, "", &json!({ "name": " ", "method": "GET", "url": "" }))
        .expect_err("blank name");
    assert!(blank.to_string().contains("empty"), "{blank}");
    assert!(
        project
            .create_request(
                &FS,
                "missing",
                &json!({ "name": "a", "method": "GET", "url": "" })
            )
            .is_err()
    );
}

/// R-MCP-4: a project in a newer format is read-only for agents too.
#[test]
fn a_read_only_project_refuses_edits() {
    let dir = tempfile::tempdir().expect("dir");
    let project = project(dir.path());
    let manifest = dir.path().join("tortuga.toml");
    let text = std::fs::read_to_string(&manifest).expect("manifest");
    let newer = text
        .lines()
        .map(|l| {
            if l.starts_with("format") {
                "format = 999"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&manifest, newer).expect("write");

    assert!(project.list_project(&FS).is_ok());
    assert!(project.create_folder(&FS, "", "New").is_err());
    assert!(project.delete_item(&FS, "users").is_err());
    assert!(dir.path().join("users").exists());
}

/// P6: this machine's values never reach the agent, however they are embedded.
#[test]
fn hidden_values_are_replaced_longest_first() {
    let hidden = [
        ("part".to_owned(), "abc-ü".to_owned()),
        ("token".to_owned(), "abc-ü😀-123".to_owned()),
        ("short".to_owned(), "ab".to_owned()),
    ];
    let value = json!({
        "body": "{\"token\":\"abc-ü😀-123\"} abc-ü",
        "list": ["Bearer abc-ü😀-123", 3, null],
        "ab": "ab stays",
    });

    let shown = hide(value, &hidden);

    assert_eq!(shown["body"], "{\"token\":\"{{token}}\"} {{part}}");
    assert_eq!(shown["list"][0], "Bearer {{token}}");
    assert_eq!(shown["ab"], "ab stays", "too short to hide");
}

/// R-MCP-3: bodies are cut at a character boundary.
#[test]
fn cutting_never_splits_a_character() {
    assert_eq!(cut("ü😀", 3), "ü");
    assert_eq!(cut("abc", 10), "abc");
    assert_eq!(cut("", 0), "");
}

/// R-MCP-3: a big body is cut and says how big it is.
#[test]
fn a_big_body_is_cut_and_says_so() {
    let response = Response {
        status: 200,
        status_text: "OK".into(),
        headers: vec![],
        body: "ü".repeat(BODY_LIMIT).into_bytes(),
        elapsed: std::time::Duration::from_millis(5),
        timing: None,
    };

    let value = response_json(&response);

    let body = value["body"].as_str().expect("text");
    assert!(body.len() <= BODY_LIMIT);
    assert!(
        value["body_truncated"]
            .as_str()
            .expect("note")
            .contains("bytes")
    );
    let binary = Response {
        body: vec![0xff, 0xfe, 0x00],
        ..response
    };
    assert!(
        response_json(&binary)["body"]
            .as_str()
            .expect("text")
            .starts_with("(binary, 3 bytes")
    );
}

/// R-MCP-5: the app's secret values go by environment, and none is shown.
#[test]
fn secrets_follow_the_environment_and_are_all_hidden() {
    let dir = tempfile::tempdir().expect("dir");
    let project = McpProject {
        secrets: BTreeMap::from([
            (
                String::new(),
                BTreeMap::from([("token".into(), "every-env".into())]),
            ),
            (
                "prod".into(),
                BTreeMap::from([("token".into(), "prod-only".into())]),
            ),
        ]),
        ..project(dir.path())
    };
    let get = first_request(&project);
    let none = BTreeMap::new();

    let dev = project
        .prepare_send(&FS, &get, Some("Dev"), &none)
        .expect("dev");
    let prod = project
        .prepare_send(&FS, &get, Some("Prod"), &none)
        .expect("prod");

    assert_eq!(dev.options.secrets["token"], "every-env");
    assert_eq!(
        prod.options.secrets["token"], "prod-only",
        "names match ignoring case"
    );
    let hidden: Vec<String> = project.hidden().into_iter().map(|(_, v)| v).collect();
    assert!(hidden.contains(&"every-env".to_owned()), "{hidden:?}");
    assert!(hidden.contains(&"prod-only".to_owned()), "{hidden:?}");
    assert!(
        !format!("{project:?}").contains("prod-only"),
        "Debug hides values"
    );
}

/// R-MCP-5: "Copy agent config" gives each agent the URL and the token.
#[test]
fn agent_configs_carry_the_url_and_token() {
    let url = "http://127.0.0.1:4040/mcp";
    for agent in Agent::ALL {
        let config = agent.config(url, "abc123");
        assert!(config.contains(url), "{config}");
        assert!(config.contains("Bearer abc123"), "{config}");
    }
    let cursor = json(&Agent::Cursor.config(url, "t"));
    assert_eq!(cursor["mcpServers"]["tortuga"]["url"], url);
    let vscode = json(&Agent::VsCode.config(url, "t"));
    assert_eq!(vscode["servers"]["tortuga"]["type"], "http");
    assert!(
        Agent::ClaudeCode
            .config(url, "t")
            .starts_with("claude mcp add --transport http")
    );
}
