//! Importing a Postman collection as a project on disk (R-COL-16).

use std::path::Path;

use tortuga_core::import::{self, ImportKind};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Auth, Collection, Item, KeyValue};

const FS: FsStorage = FsStorage;

const COLLECTION: &str = r#"{
  "info": {
    "name": "Orders",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "variable": [{ "key": "baseUrl", "value": "https://orders.test" }],
  "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "very-secret" }] },
  "item": [
    {
      "name": "Admin",
      "auth": {
        "type": "apikey",
        "apikey": [
          { "key": "key", "value": "X-Admin-Key" },
          { "key": "value", "value": "admin-secret" }
        ]
      },
      "variable": [{ "key": "region", "value": "eu" }],
      "item": [
        { "name": "Stats", "request": { "method": "GET", "url": "{{baseUrl}}/stats?r={{region}}" } }
      ]
    },
    { "name": "List orders", "request": { "method": "GET", "url": "{{baseUrl}}/orders" } }
  ]
}"#;

fn names(items: &[Item]) -> Vec<String> {
    items.iter().map(Item::name).collect()
}

/// Every file under `dir`, as text.
fn all_files(dir: &Path) -> String {
    let mut out = String::new();
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.push_str(&all_files(&path));
        } else {
            out.push_str(&std::fs::read_to_string(&path).expect("read"));
        }
    }
    out
}

/// R-COL-16: the collection is written as an ordinary project: a server for
/// `{{baseUrl}}`, folder defaults for the folder's auth and variables, and no
/// credential in any file (D35).
#[test]
fn import_writes_a_project_without_credentials() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("orders");
    let import = import::parse(COLLECTION, "orders.json", ImportKind::Postman).expect("parse");
    let project = import::create_project(&FS, &root, &import).expect("create");

    assert_eq!(project.name(), "Orders");
    assert_eq!(names(&project.items), ["Admin", "List orders"]);
    assert_eq!(project.manifest.secrets, ["token", "x_admin_key"]);
    assert_eq!(project.manifest.servers[0].id, "baseurl");
    assert_eq!(
        project.environments[0].servers["baseurl"],
        "https://orders.test"
    );

    let Item::Folder(admin) = &project.items[0] else {
        panic!("Admin is a folder");
    };
    assert_eq!(
        admin.defaults.headers,
        [KeyValue::new("X-Admin-Key", "{{x_admin_key}}")]
    );
    assert_eq!(admin.defaults.variables["region"], "eu");
    let Item::Request(stats) = &admin.items[0] else {
        panic!("Stats is a request");
    };
    assert_eq!(stats.request.server.as_deref(), Some("baseurl"));
    assert_eq!(stats.request.url, "/stats");
    assert_eq!(stats.request.auth, Some(Auth::None));

    let files = all_files(&root);
    assert!(!files.contains("very-secret") && !files.contains("admin-secret"));
    assert_eq!(import.secret_values["token"], "very-secret");
    assert_eq!(import.secret_values["x_admin_key"], "admin-secret");

    // Opened again, it is the same project.
    let again = Collection::open(&FS, &root).expect("open");
    assert_eq!(names(&again.items), names(&project.items));
}
