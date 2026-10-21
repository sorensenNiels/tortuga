use super::*;
use crate::collection::Collection;
use crate::import::{create_project, requests};
use crate::model::Auth;
use crate::storage::FsStorage;

/// A Bruno collection with what teams typically have in one, as (path, text).
const FILES: [(&str, &str); 17] = [
    (
        "bruno.json",
        r#"{ "version": "1", "name": "Shop API", "type": "collection", "ignore": ["node_modules", ".git"] }"#,
    ),
    (
        "collection.bru",
        "headers {\n  X-Client: tortuga\n}\n\nauth {\n  mode: bearer\n}\n\nauth:bearer {\n  token: {{token}}\n}\n\nvars:pre-request {\n  userId: 42\n}\n\nscript:pre-request {\n  console.log(1);\n}\n\ndocs {\n  The shop's API.\n}\n",
    ),
    (
        ".env",
        "# local secrets\nAPI_KEY=\"k3y ü😀\"\nexport OTHER=1\n",
    ),
    (
        "environments/Local.bru",
        "vars {\n  baseUrl: http://localhost:3000\n  ~old: x\n}\nvars:secret [\n  token\n]\n",
    ),
    (
        "environments/Production.bru",
        "vars {\n  baseUrl: https://shop.test/api\n}\nvars:secret [\n  token\n]\n",
    ),
    (
        "users/folder.bru",
        "meta {\n  name: Users\n  seq: 1\n}\n\nheaders {\n  Accept: application/json\n}\n\nvars:pre-request {\n  page: 1\n}\n\ndocs {\n  Everything about users.\n}\n",
    ),
    (
        "users/list.bru",
        "meta {\n  name: List users\n  type: http\n  seq: 2\n}\n\nget {\n  url: {{baseUrl}}/users?page={{page}}&debug=1\n  body: none\n  auth: inherit\n}\n\nparams:query {\n  page: {{page}}\n  ~debug: 1\n}\n\nassert {\n  res.status: eq 200\n  res.body.items: isArray\n  res('items'): isDefined\n}\n\nvars:post-response {\n  firstId: res.body.items[0].id\n  ids: res.body.items.map(i => i.id)\n}\n",
    ),
    (
        "users/get.bru",
        "meta {\n  name: Get user\n  type: http\n  seq: 1\n}\n\nget {\n  url: {{baseUrl}}/users/:id\n  body: none\n  auth: inherit\n}\n\nparams:path {\n  id: {{userId}}\n}\n\nscript:pre-request {\n  bru.setVar(\"n\", 1);\n}\n\ntests {\n  test(\"ok\", function() {\n    expect(res.getStatus()).to.equal(200);\n  });\n}\n",
    ),
    (
        "users/create.bru",
        "meta {\n  name: Create user\n  type: http\n}\n\npost {\n  url: {{baseUrl}}/users\n  body: json\n  auth: basic\n}\n\nauth:basic {\n  username: ann\n  password: hunter2\n}\n\nbody:json {\n  {\n    \"name\": \"Ann ü😀\"\n  }\n}\n\nbody:text {\n  not the chosen body\n}\n\nvars:pre-request {\n  stamp: {{$timestamp}}\n  plain: x\"y\n}\n\nsettings {\n  timeout: 5000\n  followRedirects: false\n}\n\ndocs {\n  Creates a user.\n}\n",
    ),
    (
        "partners/folder.bru",
        "meta {\n  name: Partners\n  seq: 2\n}\n\nauth {\n  mode: apikey\n}\n\nauth:apikey {\n  key: X-Partner-Key\n  value: {{process.env.API_KEY}}\n  placement: header\n}\n",
    ),
    (
        "partners/upload.bru",
        "meta {\n  name: Upload avatar\n  seq: 1\n}\n\nput {\n  url: {{baseUrl}}/avatar\n  body: multipartForm\n  auth: inherit\n}\n\nbody:multipart-form {\n  title: me\n  ~draft: yes\n  file: @file(avatars/me.png) @contentType(image/png)\n}\n",
    ),
    (
        "partners/search.bru",
        "meta {\n  name: Search\n  type: graphql\n  seq: 2\n}\n\npost {\n  url: {{baseUrl}}/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  { users { id } }\n}\n\nbody:graphql:vars {\n  {\"n\": 1}\n}\n",
    ),
    (
        "public/folder.bru",
        "meta {\n  name: Public\n}\n\nauth {\n  mode: none\n}\n",
    ),
    (
        "public/health.bru",
        "meta {\n  name: Health\n}\n\nget {\n  url: https://status.shop.test/health\n  auth: inherit\n}\n",
    ),
    (
        "socket.bru",
        "meta {\n  name: Socket\n  type: ws\n}\n\nws {\n  url: wss://shop.test\n}\n",
    ),
    ("broken.bru", "meta {\n  name: Broken\n"),
    (
        "digest.bru",
        "meta {\n  name: Digest\n}\n\nget {\n  url: https://shop.test/digest\n  auth: digest\n}\n\nauth:digest {\n  username: a\n  password: b\n}\n",
    ),
];

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
}

fn import() -> (tempfile::TempDir, Import) {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), &FILES);
    // Left alone: ignored and hidden folders, and files that are no requests.
    write(
        dir.path(),
        &[
            ("node_modules/pkg/x.bru", "get {\n  url: x\n}\n"),
            (".git/y.bru", "get {\n  url: y\n}\n"),
            ("notes.txt", "hello"),
        ],
    );
    let import = read(&FsStorage, dir.path()).expect("read");
    (dir, import)
}

fn find<'a>(import: &'a Import, name: &str) -> &'a Request {
    requests(&import.items)
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no request {name}"))
}

fn folder<'a>(import: &'a Import, name: &str) -> (&'a FolderDefaults, &'a [NewItem]) {
    import
        .items
        .iter()
        .find_map(|i| match i {
            NewItem::Folder {
                name: n,
                defaults,
                items,
            } if n == name => Some((defaults, items.as_slice())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no folder {name}"))
}

fn names(items: &[NewItem]) -> Vec<&str> {
    items
        .iter()
        .map(|i| match i {
            NewItem::Request(r) => r.name.as_str(),
            NewItem::Folder { name, .. } => name.as_str(),
        })
        .collect()
}

/// R-COL-31: folders and requests in `seq` order (without one: after, by name); the
/// collection's and folders' headers, variables, docs; `{{baseUrl}}` as a server whose
/// base URLs come from the environments.
#[test]
fn structure_defaults_and_environments() {
    let (_dir, import) = import();
    assert_eq!(import.kind, ImportKind::Bruno);
    assert_eq!(import.manifest.name, "Shop API");
    assert_eq!(import.manifest.description, "The shop's API.");
    assert_eq!(
        names(&import.items),
        ["Users", "Partners", "Public", "Digest"]
    );
    let (users, items) = folder(&import, "Users");
    assert_eq!(names(items), ["Get user", "List users", "Create user"]);
    assert_eq!(users.description, "Everything about users.");
    assert_eq!(users.headers, [KeyValue::new("Accept", "application/json")]);
    assert_eq!(users.variables.get("page").map(String::as_str), Some("1"));
    assert_eq!(
        import.manifest.headers,
        [KeyValue::new("X-Client", "tortuga")]
    );
    assert_eq!(
        import.manifest.variables.get("userId").map(String::as_str),
        Some("42")
    );

    let server = &import.manifest.servers[0];
    assert_eq!(server.name, "baseUrl");
    let envs: Vec<_> = import
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(envs, ["Local", "Production"]);
    assert_eq!(
        import.environments[1]
            .servers
            .get(&server.id)
            .map(String::as_str),
        Some("https://shop.test/api")
    );
    assert!(!import.environments[0].variables.contains_key("old"));
    let list = find(&import, "List users");
    assert_eq!(list.server.as_deref(), Some(server.id.as_str()));
    assert_eq!(list.url, "/users");
    assert_eq!(list.params.len(), 2);
    assert!(!list.params[1].enabled);
    let get = find(&import, "Get user");
    assert_eq!(get.path_vars, [KeyValue::new("id", "{{userId}}")]);
}

/// R-COL-31, D35: auth on three levels; credentials, `vars:secret` and
/// `{{process.env.…}}` become secret variables, with values only for this machine.
#[test]
fn auth_and_secrets() {
    let (_dir, import) = import();
    assert_eq!(
        import.manifest.auth,
        Some(Auth::Bearer {
            token: "{{token}}".into()
        })
    );
    assert_eq!(find(&import, "List users").auth, None);
    assert_eq!(
        find(&import, "Create user").auth,
        Some(Auth::Basic {
            username: "{{username}}".into(),
            password: "{{password}}".into()
        })
    );
    // A folder without auth turns the project's off for its requests.
    assert_eq!(find(&import, "Health").auth, Some(Auth::None));
    assert_eq!(find(&import, "Search").auth, Some(Auth::None));
    let (partners, _) = folder(&import, "Partners");
    assert!(
        partners
            .headers
            .contains(&KeyValue::new("X-Partner-Key", "{{API_KEY}}"))
    );

    for name in ["token", "API_KEY", "username", "password"] {
        assert!(import.manifest.secrets.contains(&name.to_owned()), "{name}");
    }
    assert!(!import.manifest.secrets.contains(&"OTHER".to_owned()));
    assert_eq!(
        import.secret_values.get("API_KEY").map(String::as_str),
        Some("k3y ü😀")
    );
    assert_eq!(
        import.secret_values.get("password").map(String::as_str),
        Some("hunter2")
    );
    // Bruno keeps `token` on its user's computer.
    assert!(!import.has_secret_value("token"));
    let files: String = requests(&import.items)
        .into_iter()
        .map(|r| crate::format::write_request(r, None))
        .collect();
    assert!(!files.contains("hunter2") && !files.contains("k3y"));
    assert!(!files.contains("process.env"));
}

/// R-COL-31: bodies of the chosen mode, form parts with files, GraphQL as JSON,
/// settings, docs.
#[test]
fn bodies_and_settings() {
    let (_dir, import) = import();
    let create = find(&import, "Create user");
    assert_eq!(
        create.body,
        RequestBody::Json("{\n  \"name\": \"Ann ü😀\"\n}".into())
    );
    assert_eq!(create.timeout_ms, Some(5000));
    assert_eq!(create.description, "Creates a user.");
    let RequestBody::Multipart(parts) = &find(&import, "Upload avatar").body else {
        panic!("multipart");
    };
    assert_eq!(parts.len(), 3);
    assert!(!parts[1].enabled);
    assert_eq!(parts[2].value, "avatars/me.png");
    assert!(parts[2].file);
    assert_eq!(parts[2].content_type.as_deref(), Some("image/png"));
    let RequestBody::Json(graphql) = &find(&import, "Search").body else {
        panic!("graphql");
    };
    let json: Value = serde_json::from_str(graphql).expect("json");
    assert_eq!(json["query"], "{ users { id } }");
    assert_eq!(json["variables"]["n"], 1);
}

/// R-COL-31: assertions become tests, response reads become captures, request
/// variables `vars.set(…)`, and scripts are kept as comments.
#[test]
fn checks_and_scripts() {
    let (_dir, import) = import();
    let list = find(&import, "List users");
    assert!(
        list.scripts
            .tests
            .contains("test(\"res.status: eq 200\", () => expect(res.status).toEqual(200));")
    );
    assert!(list.scripts.tests.contains("// res('items'): isDefined"));
    assert_eq!(
        list.capture.get("firstId").map(String::as_str),
        Some("$.items[0].id")
    );
    assert!(!list.capture.contains_key("ids"));

    let create = find(&import, "Create user");
    assert_eq!(
        create.scripts.pre_request,
        "vars.set(\"stamp\", vars.replaceIn(\"{{$timestamp}}\"));\nvars.set(\"plain\", \"x\\\"y\");\n"
    );
    assert!(crate::script::parses(&create.scripts.pre_request));

    let get = find(&import, "Get user");
    assert!(
        get.scripts
            .pre_request
            .starts_with(&crate::import::script_note(ImportKind::Bruno))
    );
    assert!(get.scripts.pre_request.contains("// bru.setVar(\"n\", 1);"));
    assert!(
        get.scripts
            .tests
            .contains("//   expect(res.getStatus()).to.equal(200);")
    );
    assert!(crate::script::parses(&get.scripts.tests));
}

/// R-COL-31: what can't be imported as written is listed.
#[test]
fn warnings() {
    let (_dir, import) = import();
    let expect = [
        "The scripts of the collection were left out",
        "“Socket” is a ws request",
        "broken.bru could not be read (line 1: block “meta” is not closed)",
        "“Digest” uses digest auth",
        "“Upload avatar” uploads files",
        "“Create user” doesn't follow redirects in Bruno",
        "“List users” sets ids to res.body.items.map(i => i.id)",
        "“List users” has assertions the import could not translate",
        "1 script was translated from Bruno's bru API",
        "2 scripts were kept as comments",
    ];
    for text in expect {
        assert!(
            import.warnings.iter().any(|w| w.contains(text)),
            "missing {text:?} in {:#?}",
            import.warnings
        );
    }
    assert_eq!(
        import.warnings.len(),
        expect.len(),
        "{:#?}",
        import.warnings
    );
}

/// R-COL-31: the import is written as a project that opens with every request.
#[test]
fn becomes_a_project() {
    let (dir, import) = import();
    let root = dir.path().join("tortuga");
    let project = create_project(&FsStorage, &root, &import).expect("create");
    assert_eq!(project.name(), "Shop API");
    let opened = Collection::open(&FsStorage, &root).expect("open");
    assert_eq!(opened.children(Path::new("")).len(), 4);
    assert_eq!(opened.folders().len(), 3);
    assert_eq!(opened.environments.len(), 2);
    assert_eq!(import.requests, 7);
}

#[test]
fn what_is_not_a_collection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = crate::error::message(&read(&FsStorage, dir.path()).expect_err("no bruno.json"));
    assert!(err.contains("has no bruno.json"), "{err}");
    write(dir.path(), &[("bruno.json", "{ nope")]);
    let err = crate::error::message(&read(&FsStorage, dir.path()).expect_err("bad json"));
    assert!(err.contains("not valid JSON"), "{err}");
    // Only bruno.json: an empty project named after the folder's config or folder.
    write(dir.path(), &[("bruno.json", "{}")]);
    let import = read(&FsStorage, dir.path()).expect("empty");
    assert_eq!(import.requests, 0);
    assert_ne!(import.manifest.name, "");
}

#[test]
fn environment_files() {
    let env = parse_environment(
        "vars {\n  host: {{process.env.HOST}}\n  key: abc\n}\nvars:secret [\n  key,\n  ~gone,\n  pin\n]\n",
        "Stage ü",
    )
    .expect("parse");
    assert_eq!(env.name, "Stage ü");
    let got: Vec<_> = env
        .values
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str(), v.secret))
        .collect();
    assert_eq!(
        got,
        [
            ("host", "{{HOST}}", false),
            ("key", "abc", true),
            ("pin", "", true),
            ("HOST", "", true)
        ]
    );
    let err = parse_environment("meta {\n}\n", "x").expect_err("no vars");
    assert!(crate::error::message(&err).contains("no vars"));
}

#[test]
fn process_env_and_dotenv() {
    let mut names = BTreeSet::new();
    assert_eq!(
        without_process_env(
            "a {{process.env.X}} b {{ process.env.Y }} {{process.env.}} {{process.env.Z",
            &mut names
        ),
        "a {{X}} b {{ process.env.Y }} {{process.env.}} {{process.env.Z"
    );
    assert_eq!(names.into_iter().collect::<Vec<_>>(), ["X"]);
    let values = dotenv("A=1\n# c\nexport B = 'two words'\nC=\"ü😀\"\nbad line\n\nD=\"unclosed\n");
    assert_eq!(values.get("A").map(String::as_str), Some("1"));
    assert_eq!(values.get("B").map(String::as_str), Some("two words"));
    assert_eq!(values.get("C").map(String::as_str), Some("ü😀"));
    assert_eq!(values.get("D").map(String::as_str), Some("\"unclosed"));
    assert_eq!(values.len(), 4);
}
