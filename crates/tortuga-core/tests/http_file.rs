//! Importing a `.http` file as a project on disk (R-COL-17).

use tortuga_core::import::{self, ImportKind};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Auth, Collection, Item};

const FS: FsStorage = FsStorage;

const FILE: &str = r#"@baseUrl = http://localhost:3000

### Log in
# @name login
POST {{baseUrl}}/login
Content-Type: application/json

{"user": "ann"}

### Me
GET {{baseUrl}}/me
Authorization: Bearer {{login.response.body.$.token}}
"#;

/// R-COL-17: the file is written as an ordinary project: a server for `{{baseUrl}}`,
/// and the request variable as a capture in "Log in" that "Me" uses (R-TEST-4).
#[test]
fn import_writes_a_project_with_captures() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("auth");
    let import = import::parse(FILE, "auth.http", ImportKind::Http).expect("parse");
    import::create_project(&FS, &root, &import).expect("create");

    let project = Collection::open(&FS, &root).expect("open");
    assert_eq!(project.name(), "Auth");
    assert_eq!(
        project.environments[0].servers["baseurl"],
        "http://localhost:3000"
    );
    let [Item::Request(login), Item::Request(me)] = project.items.as_slice() else {
        panic!("two requests: {:?}", project.items);
    };
    assert_eq!(login.request.name, "Log in");
    assert_eq!(login.request.capture["login_token"], "$.token");
    assert_eq!(me.request.server.as_deref(), Some("baseurl"));
    assert_eq!(
        me.request.auth,
        Some(Auth::Bearer {
            token: "{{login_token}}".into()
        })
    );
    assert_eq!(project.manifest.secrets, Vec::<String>::new());
}
