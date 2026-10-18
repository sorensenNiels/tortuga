//! Importing an OpenAPI description as a project on disk (R-COL-15).

use std::path::Path;

use tortuga_core::storage::FsStorage;
use tortuga_core::{Collection, Error, Item, import, openapi};

const FS: FsStorage = FsStorage;

const SPEC: &str = r#"{
  "openapi": "3.0.3",
  "info": { "title": "Users API", "version": "1" },
  "servers": [
    { "url": "https://api.users.test", "description": "Production" },
    { "url": "https://staging.users.test", "description": "Staging" }
  ],
  "security": [{ "bearer": [] }],
  "paths": {
    "/users": {
      "get": { "tags": ["Users"], "summary": "List users" },
      "post": {
        "tags": ["Users"],
        "summary": "Create user",
        "requestBody": {
          "content": {
            "application/json": { "example": { "name": "Ann" } }
          }
        }
      }
    },
    "/users/{id}": {
      "get": {
        "tags": ["Users"],
        "summary": "Get user",
        "parameters": [{ "name": "id", "in": "path", "required": true }]
      }
    },
    "/health": { "get": { "summary": "Health" } }
  },
  "components": {
    "securitySchemes": { "bearer": { "type": "http", "scheme": "bearer" } }
  }
}"#;

fn names(items: &[Item]) -> Vec<String> {
    items.iter().map(Item::name).collect()
}

/// R-COL-15: the import is written as an ordinary project: manifest, environments,
/// a folder per tag and a file per request, in the description's order.
#[test]
fn import_writes_a_project() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("users-api");
    let import = openapi::parse(SPEC).expect("parse");
    let project = import::create_project(&FS, &root, &import).expect("create");

    assert_eq!(project.name(), "Users API");
    assert_eq!(names(&project.items), ["Users", "Health"]);
    let Item::Folder(users) = &project.items[0] else {
        panic!("folder");
    };
    assert_eq!(
        names(&users.items),
        ["List users", "Create user", "Get user"]
    );
    let environments: Vec<&str> = project
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(environments, ["Production", "Staging"]);
    assert_eq!(project.manifest.secrets, ["token"]);

    let read = |path: &str| std::fs::read_to_string(root.join(path)).expect(path);
    let manifest = read("tortuga.toml");
    assert!(manifest.contains("secrets = [\"token\"]"), "{manifest}");
    assert!(manifest.contains("token = \"{{token}}\""), "{manifest}");
    assert!(
        read("environments/staging.toml").contains("users-api = \"https://staging.users.test\"")
    );
    let get = read("users/get-user.toml");
    assert!(get.contains("url = \"/users/:id\""), "{get}");
    assert!(get.contains("server = \"users-api\""), "{get}");
    assert!(read("users/create-user.toml").contains("\"name\": \"Ann\""));
    assert!(root.join("users/_folder.toml").exists());
    assert!(root.join("health.toml").exists());
}

/// R-COL-15, D34: an import never writes into an existing project.
#[test]
fn import_needs_a_folder_that_is_not_a_project() {
    let dir = tempfile::tempdir().expect("temp dir");
    Collection::create(&FS, dir.path(), "Existing").expect("create");
    let import = openapi::parse(SPEC).expect("parse");
    let result = import::create_project(&FS, dir.path(), &import);
    assert!(
        matches!(result, Err(Error::AlreadyACollection(_))),
        "{result:?}"
    );
    let project = Collection::open(&FS, dir.path()).expect("open");
    assert_eq!(project.name(), "Existing");
    assert_eq!(project.items.len(), 0);
    assert!(!Path::new(&dir.path().join("users")).exists());
}
