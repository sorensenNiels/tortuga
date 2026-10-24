//! Importing a Postman collection as a project on disk (R-COL-16), and running it as
//! Postman's "Run collection" does (R-COL-39, R-COL-41).

use std::path::Path;

use tortuga_core::import::{self, ImportKind};
use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::{self, RunOptions, Workflow};
use tortuga_core::{Auth, Client, Collection, Item, KeyValue};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

const XML_SUITE: &str = include_str!("fixtures/xml-suite.postman_collection.json");

/// R-COL-39: a real XML test suite, written on Windows and reading XML with `xml2js`,
/// is imported with every script translated, and no `\r` in its files.
#[test]
fn xml_suite_scripts_are_all_translated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("xml-suite");
    let import = import::parse(XML_SUITE, "xml-suite.json", ImportKind::Postman).expect("parse");
    assert!(
        import
            .warnings
            .iter()
            .any(|w| w.starts_with("4 scripts were translated")),
        "{:?}",
        import.warnings
    );
    assert!(
        !import
            .warnings
            .iter()
            .any(|w| w.contains("kept as comments")),
        "{:?}",
        import.warnings
    );
    import::create_project(&FS, &root, &import).expect("create");
    let files = all_files(&root);
    assert!(!files.contains("\\r") && !files.contains('\r'));
    assert!(!files.contains("require(") && files.contains("xml2Json(res.body)"));
}

/// Answers `request` (a text in the XML body) with `xml`.
async fn answer(server: &MockServer, request: &str, xml: &str) {
    Mock::given(method("POST"))
        .and(path("/apixml"))
        .and(header("content-type", "application/xml"))
        .and(body_string_contains(request))
        .respond_with(ResponseTemplate::new(200).set_body_string(xml))
        .mount(server)
        .await;
}

/// R-COL-41, R-FLOW-7: run as the project's Run… runs it (every request in order, as
/// Postman's "Run collection"), the translated scripts jump with `setNextRequest` by
/// request name and hand values on through variables.
#[tokio::test]
async fn an_imported_xml_suite_runs_as_its_workflow() {
    let server = MockServer::start().await;
    answer(
        &server,
        "destinationGet",
        r#"<destinationGetResponse status="success"/>"#,
    )
    .await;
    answer(
        &server,
        "registrationCreate",
        r#"<registrationCreateResponse status="success"><registration registrationSequence="4711"/></registrationCreateResponse>"#,
    )
    .await;
    answer(
        &server,
        r#"registrationGet registrationSequence="4711""#,
        r#"<registrationGetResponse status="success"><registration><itemValue item="X" value="ü😀"/><itemValue item="BESKED" value="ABC ? ABC"/></registration></registrationGetResponse>"#,
    )
    .await;

    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("xml-suite");
    let import = import::parse(XML_SUITE, "xml-suite.json", ImportKind::Postman).expect("parse");
    let project = import::create_project(&FS, &root, &import).expect("create");
    assert_eq!(project.workflows, [], "the import writes no workflow (D65)");
    let suite = Workflow::from_folder(&project, Path::new(""));
    assert_eq!(suite.name, "XML suite");
    assert!(suite.continue_on_failure);
    let steps: Vec<&str> = suite.steps.iter().map(|s| s.request.as_str()).collect();
    assert_eq!(
        steps,
        [
            "destinationget.toml",
            "destinationcreate.toml",
            "registrationcreate.toml",
            "registrationget.toml"
        ]
    );

    let mut environments = project.environments.clone();
    environments[0]
        .servers
        .insert("servicebase".into(), server.uri());
    project
        .save_config(&FS, &project.manifest, &environments)
        .expect("save");
    let project = Collection::open(&FS, &root).expect("open");
    let options = RunOptions {
        environment: Some(environments[0].id.clone()),
        ..RunOptions::default()
    };
    let result = workflow::run(&Client::default(), &project, &suite, &options, |_| {}).await;

    let iteration = &result.iterations[0];
    let ran: Vec<(usize, &str)> = iteration
        .steps
        .iter()
        .map(|s| (s.index, s.name.as_str()))
        .collect();
    assert_eq!(
        ran,
        [
            (0, "destinationGet"),
            (2, "registrationCreate"),
            (3, "registrationGet")
        ]
    );
    for step in &iteration.steps {
        assert!(
            step.outcome.errors.is_empty(),
            "{}: {:?}",
            step.name,
            step.outcome.errors
        );
        assert!(!step.outcome.tests.is_empty(), "{} has tests", step.name);
        assert!(
            step.outcome.tests.iter().all(|t| t.passed),
            "{}: {:?}",
            step.name,
            step.outcome.tests
        );
    }
    assert!(result.passed());
}
