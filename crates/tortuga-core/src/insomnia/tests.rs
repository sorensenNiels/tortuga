use super::*;
use crate::import::{requests, script_note};
use crate::model::Auth;

/// A v4 export (Insomnia ≤ 10) with what teams typically have in a collection, and a
/// second collection that is not imported.
const V4: &str = r#"{
  "_type": "export",
  "__export_format": 4,
  "__export_source": "insomnia.desktop.app:v10.3.0",
  "resources": [
    { "_id": "wrk_1", "_type": "workspace", "parentId": null, "name": "Shop API", "description": "The shop's API.", "scope": "collection" },
    { "_id": "wrk_2", "_type": "workspace", "parentId": null, "name": "Other", "scope": "collection" },
    { "_id": "env_base", "_type": "environment", "parentId": "wrk_1", "name": "Base Environment", "data": { "base_url": "https://shop.test/api", "user": { "id": 42 } } },
    { "_id": "env_prod", "_type": "environment", "parentId": "env_base", "name": "Production ü", "metaSortKey": 2, "isPrivate": true, "data": { "base_url": "https://shop.test/api", "token": "prod-ü😀" } },
    { "_id": "env_dev", "_type": "environment", "parentId": "env_base", "name": "Dev", "metaSortKey": 1, "data": { "base_url": "http://localhost:3000", "token": "dev-token" } },
    { "_id": "jar_1", "_type": "cookie_jar", "parentId": "wrk_1", "cookies": [{ "key": "s", "value": "1" }] },
    { "_id": "spc_1", "_type": "api_spec", "parentId": "wrk_1", "contents": "openapi: 3.0.0" },
    { "_id": "uts_1", "_type": "unit_test_suite", "parentId": "wrk_1", "name": "Smoke" },
    { "_id": "fld_users", "_type": "request_group", "parentId": "wrk_1", "name": "Users", "metaSortKey": -10, "description": "Everything about users.", "environment": { "page": 1 }, "headers": [{ "name": "Accept", "value": "application/json" }], "authentication": { "type": "bearer", "token": "{{ _.token }}" } },
    { "_id": "req_create", "_type": "request", "parentId": "fld_users", "name": "Create user", "metaSortKey": 3, "method": "POST", "url": "{{ _.base_url }}/users",
      "body": { "mimeType": "application/json", "text": "{\"id\": \"{% uuid 'v4' %}\", \"at\": \"{% now 'iso-8601' %}\", \"name\": \"Ann ü😀\"}" },
      "headers": [{ "name": "Content-Type", "value": "application/json" }, { "name": "X-Prev", "value": "{% response 'body', 'req_list', 'b64::JC5pZA==::46b', 'never', 60 %}" }],
      "authentication": { "type": "basic", "username": "ann", "password": "hunter2" } },
    { "_id": "req_list", "_type": "request", "parentId": "fld_users", "name": "List users", "metaSortKey": 2, "method": "GET", "url": "{{ _.base_url }}/users?page={{ _.page }}",
      "parameters": [{ "name": "debug", "value": "1", "disabled": true }], "headers": [], "authentication": {}, "body": {},
      "afterResponseScript": "insomnia.test(\"ok\", () => {\n  insomnia.expect(insomnia.response.code).to.equal(200);\n});\ninsomnia.environment.set(\"first\", insomnia.response.json()[0].id);" },
    { "_id": "req_get", "_type": "request", "parentId": "fld_users", "name": "Get user", "metaSortKey": 1, "method": "GET", "url": "{{ _.base_url }}/users/:id",
      "pathParameters": [{ "name": "id", "value": "{{ _.user.id }}" }], "authentication": {},
      "preRequestScript": "insomnia.sendRequest('https://x');", "settingSendCookies": false, "settingFollowRedirects": "off" },
    { "_id": "fld_partners", "_type": "request_group", "parentId": "wrk_1", "name": "Partners", "metaSortKey": 0, "preRequestScript": "console.log(1)",
      "authentication": { "type": "apikey", "key": "X-Partner-Key", "value": "s3cret", "addTo": "header" } },
    { "_id": "req_upload", "_type": "request", "parentId": "fld_partners", "name": "Upload avatar", "metaSortKey": 1, "method": "PUT", "url": "{{ _.base_url }}/avatar", "authentication": {},
      "body": { "mimeType": "multipart/form-data", "params": [{ "name": "title", "value": "me" }, { "name": "file", "type": "file", "fileName": "/home/ann/me.png" }, { "name": "draft", "value": "x", "disabled": true }] } },
    { "_id": "req_search", "_type": "request", "parentId": "fld_partners", "name": "Search", "metaSortKey": 2, "method": "POST", "url": "{{ _.base_url }}/graphql", "authentication": { "type": "none" },
      "body": { "mimeType": "application/graphql", "text": "{\"query\":\"{ users { id } }\",\"variables\":{\"n\":1}}" } },
    { "_id": "req_login", "_type": "request", "parentId": "fld_partners", "name": "Login", "metaSortKey": 3, "method": "POST", "url": "{{ _.base_url }}/login",
      "authentication": { "type": "digest", "username": "a", "password": "b" },
      "body": { "mimeType": "application/x-www-form-urlencoded", "params": [{ "name": "remember", "value": "true" }] } },
    { "_id": "req_health", "_type": "request", "parentId": "wrk_1", "name": "Health", "metaSortKey": 5, "method": "GET", "url": "https://status.shop.test/health", "body": { "mimeType": "text/csv", "text": "a,b" } },
    { "_id": "req_trace", "_type": "request", "parentId": "wrk_1", "name": "Trace", "metaSortKey": 6, "method": "TRACE", "url": "{{ _.base_url }}" },
    { "_id": "ws_1", "_type": "websocket_request", "parentId": "wrk_1", "name": "Live", "metaSortKey": 7, "url": "wss://shop.test/live" },
    { "_id": "req_other", "_type": "request", "parentId": "wrk_2", "name": "Not this one", "method": "GET", "url": "https://other.test" }
  ]
}"#;

/// The same collection as a v5 export (Insomnia 11+).
const V5: &str = r#"type: collection.insomnia.rest/5.0
name: Shop API
meta:
  id: wrk_1
  description: The shop's API.
collection:
  - name: Partners
    meta:
      sortKey: 0
    authentication:
      type: apikey
      key: X-Partner-Key
      value: s3cret
      addTo: header
    scripts:
      preRequest: console.log(1)
    children:
      - url: "{{ _.base_url }}/avatar"
        name: Upload avatar
        meta:
          sortKey: 1
        method: PUT
        body:
          mimeType: multipart/form-data
          params:
            - name: title
              value: me
            - name: file
              type: file
              fileName: /home/ann/me.png
            - name: draft
              value: x
              disabled: true
      - url: "{{ _.base_url }}/graphql"
        name: Search
        meta:
          sortKey: 2
        method: POST
        authentication:
          type: none
        body:
          mimeType: application/graphql
          text: '{"query":"{ users { id } }","variables":{"n":1}}'
      - url: "{{ _.base_url }}/login"
        name: Login
        meta:
          sortKey: 3
        method: POST
        authentication:
          type: digest
          username: a
          password: b
        body:
          mimeType: application/x-www-form-urlencoded
          params:
            - name: remember
              value: "true"
  - name: Users
    meta:
      sortKey: -10
      description: Everything about users.
    environment:
      page: 1
    headers:
      - name: Accept
        value: application/json
    authentication:
      type: bearer
      token: "{{ _.token }}"
    children:
      - url: "{{ _.base_url }}/users?page={{ _.page }}"
        name: List users
        meta:
          sortKey: 2
        method: GET
        parameters:
          - name: debug
            value: "1"
            disabled: true
        authentication: {}
        scripts:
          afterResponse: |-
            insomnia.test("ok", () => {
              insomnia.expect(insomnia.response.code).to.equal(200);
            });
            insomnia.environment.set("first", insomnia.response.json()[0].id);
      - url: "{{ _.base_url }}/users/:id"
        name: Get user
        meta:
          sortKey: 1
        method: GET
        pathParameters:
          - name: id
            value: "{{ _.user.id }}"
        scripts:
          preRequest: insomnia.sendRequest('https://x');
        settings:
          followRedirects: "off"
          cookies:
            send: false
            store: true
      - url: "{{ _.base_url }}/users"
        name: Create user
        meta:
          sortKey: 3
        method: POST
        body:
          mimeType: application/json
          text: "{\"id\": \"{% uuid 'v4' %}\", \"at\": \"{% now 'iso-8601' %}\", \"name\": \"Ann ü😀\"}"
        headers:
          - name: Content-Type
            value: application/json
          - name: X-Prev
            value: "{% response 'body', 'req_list', 'b64::JC5pZA==::46b', 'never', 60 %}"
        authentication:
          type: basic
          username: ann
          password: hunter2
  - url: https://status.shop.test/health
    name: Health
    meta:
      sortKey: 5
    method: GET
    body:
      mimeType: text/csv
      text: a,b
  - url: "{{ _.base_url }}"
    name: Trace
    meta:
      sortKey: 6
    method: TRACE
  - url: wss://shop.test/live
    name: Live
    meta:
      sortKey: 7
cookieJar:
  name: Default Jar
  cookies:
    - key: s
      value: "1"
environments:
  name: Base Environment
  data:
    base_url: https://shop.test/api
    user:
      id: 42
  subEnvironments:
    - name: Production ü
      meta:
        sortKey: 2
      isPrivate: true
      data:
        base_url: https://shop.test/api
        token: prod-ü😀
    - name: Dev
      meta:
        sortKey: 1
      data:
        base_url: http://localhost:3000
        token: dev-token
spec:
  contents: "openapi: 3.0.0"
"#;

fn find<'a>(import: &'a Import, name: &str) -> &'a Request {
    requests(&import.items)
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no request {name}"))
}

fn folder<'a>(import: &'a Import, name: &str) -> (&'a FolderDefaults, Vec<&'a str>) {
    import
        .items
        .iter()
        .find_map(|i| match i {
            NewItem::Folder {
                name: n,
                defaults,
                items,
            } if n == name => Some((defaults, names(items))),
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

/// Both formats are read through the parser that tells the formats apart.
fn both() -> [Import; 2] {
    [V4, V5].map(|text| {
        let import = crate::import::parse(text, "export", ImportKind::OpenApi).expect("parse");
        assert_eq!(import.kind, ImportKind::Insomnia);
        import
    })
}

/// R-COL-32: folders and requests in Insomnia's order; folder headers, variables and
/// docs; the base environment as project variables, sub-environments as environments,
/// `{{ _.base_url }}` as a server.
#[test]
fn structure_environments_and_servers() {
    for import in both() {
        assert_eq!(import.manifest.name, "Shop API");
        assert_eq!(import.manifest.description, "The shop's API.");
        assert_eq!(names(&import.items), ["Users", "Partners", "Health"]);
        let (users, children) = folder(&import, "Users");
        assert_eq!(children, ["Get user", "List users", "Create user"]);
        assert_eq!(users.description, "Everything about users.");
        assert_eq!(users.headers, [KeyValue::new("Accept", "application/json")]);
        assert_eq!(users.variables.get("page").map(String::as_str), Some("1"));
        assert_eq!(
            import.manifest.variables.get("user.id").map(String::as_str),
            Some("42")
        );
        assert!(!import.manifest.variables.contains_key("base_url"));

        let server = &import.manifest.servers[0];
        assert_eq!(server.name, "base_url");
        let envs: Vec<_> = import
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(envs, ["Dev", "Production ü"]);
        assert_eq!(
            import.environments[0]
                .servers
                .get(&server.id)
                .map(String::as_str),
            Some("http://localhost:3000")
        );

        let list = find(&import, "List users");
        assert_eq!(list.server.as_deref(), Some(server.id.as_str()));
        assert_eq!(list.url, "/users");
        assert_eq!(list.params.len(), 2);
        assert_eq!(list.params[0], KeyValue::new("page", "{{page}}"));
        assert!(!list.params[1].enabled);
        assert_eq!(
            find(&import, "Get user").path_vars,
            [KeyValue::new("id", "{{user.id}}")]
        );
    }
}

/// R-COL-32, D35: auth on folders and requests ("inherit" when empty), credentials and
/// private environments' values as secrets.
#[test]
fn auth_and_secrets() {
    for import in both() {
        let (users, _) = folder(&import, "Users");
        assert_eq!(
            users.auth,
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
        assert_eq!(find(&import, "Search").auth, Some(Auth::None));
        assert_eq!(find(&import, "Login").auth, Some(Auth::None));
        assert_eq!(find(&import, "Health").auth, None);
        let (partners, _) = folder(&import, "Partners");
        let key = partners
            .headers
            .iter()
            .find(|h| h.name == "X-Partner-Key")
            .expect("API key header");
        assert!(key.value.starts_with("{{"), "{}", key.value);

        assert!(import.manifest.secrets.contains(&"token".to_owned()));
        assert_eq!(
            import.secret_values.get("password").map(String::as_str),
            Some("hunter2")
        );
        let prod = import.environments[1].id.clone();
        assert_eq!(
            import
                .environment_secret_values
                .get(&(prod, "token".to_owned()))
                .map(String::as_str),
            Some("prod-ü😀")
        );
        let files: String = requests(&import.items)
            .into_iter()
            .map(|r| crate::format::write_request(r, None))
            .collect();
        assert!(!files.contains("hunter2") && !files.contains("s3cret"));
    }
}

/// R-COL-32: bodies by MIME type, template tags, settings and scripts.
#[test]
fn bodies_tags_settings_and_scripts() {
    for import in both() {
        let create = find(&import, "Create user");
        assert_eq!(
            create.body,
            RequestBody::Json(
                "{\"id\": \"{{$guid}}\", \"at\": \"{{$isoTimestamp}}\", \"name\": \"Ann ü😀\"}"
                    .into()
            )
        );
        assert!(
            create
                .header("x-prev")
                .is_some_and(|v| v.starts_with("{% response"))
        );
        let RequestBody::Multipart(parts) = &find(&import, "Upload avatar").body else {
            panic!("multipart");
        };
        assert_eq!(parts.len(), 3);
        assert!(parts[1].file && parts[1].value == "/home/ann/me.png");
        assert!(!parts[2].enabled);
        let RequestBody::Json(graphql) = &find(&import, "Search").body else {
            panic!("graphql");
        };
        let json: Value = serde_json::from_str(graphql).expect("json");
        assert_eq!(json["query"], "{ users { id } }");
        assert_eq!(json["variables"]["n"], 1);
        assert_eq!(
            find(&import, "Login").body,
            RequestBody::Form(vec![KeyValue::new("remember", "true")])
        );
        let health = find(&import, "Health");
        assert_eq!(health.body, RequestBody::Text("a,b".into()));
        assert_eq!(health.header("content-type"), Some("text/csv"));

        let get = find(&import, "Get user");
        assert!(!get.cookies);
        assert!(find(&import, "List users").cookies);
        assert!(
            get.scripts
                .pre_request
                .starts_with(&script_note(ImportKind::Insomnia))
        );
        assert!(get.scripts.pre_request.contains("insomnia.sendRequest"));
        let tests = &find(&import, "List users").scripts.tests;
        assert!(
            tests.starts_with(&translated_note(ImportKind::Insomnia)),
            "{tests}"
        );
        assert!(tests.contains("expect(res.status).toBe(200)"), "{tests}");
        assert!(
            tests.contains("vars.set(\"first\", res.json()[0].id)"),
            "{tests}"
        );
        assert!(crate::script::parses(tests));
    }
}

/// R-COL-32: what can't be imported as written is listed.
#[test]
fn warnings() {
    let common = [
        "The collection's cookies were not imported",
        "The collection's API description was not imported",
        "The scripts of folder “Partners” were left out",
        "“Get user” doesn't follow redirects in Insomnia",
        "“Login” uses digest auth",
        "“Upload avatar” uploads files",
        "“Trace” was left out: Tortuga does not send TRACE requests.",
        "“Live” is a WebSocket request",
        "Insomnia's {% response %} tag was kept as written in “Create user”",
        "1 script was translated from Insomnia's insomnia API",
        "1 script was kept as a comment",
    ];
    let v4_only = [
        "The export has more collections: “Other” was not imported.",
        "The test suites “Smoke” were left out",
    ];
    let [v4, v5] = both();
    for (import, expect) in [
        (v4, common.iter().chain(&v4_only).collect::<Vec<_>>()),
        (v5, common.iter().collect()),
    ] {
        for text in &expect {
            assert!(
                import.warnings.iter().any(|w| w.contains(*text)),
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
}

#[test]
fn other_insomnia_files_and_shapes() {
    let error = |text: &str| crate::error::message(&parse(text).expect_err("refused"));
    assert!(
        error("type: environment.insomnia.rest/5.0\nname: x\n").contains("export the collection")
    );
    assert!(error("type: mock.insomnia.rest/5.0\n").contains("mock.insomnia.rest/5.0 export"));
    assert!(error(r#"{"_type": "export"}"#).contains("no resources"));

    // An empty export, and one without a workspace: what has no known parent is the top.
    let empty = parse(r#"{"_type": "export", "resources": []}"#).expect("empty");
    assert_eq!(empty.manifest.name, "Insomnia collection");
    assert_eq!(empty.requests, 0);
    let loose = parse(
        r#"{"_type": "export", "resources": [
            {"_id": "req_1", "_type": "request", "parentId": "wrk_gone", "url": "https://a.test/x"}
        ]}"#,
    )
    .expect("loose");
    assert_eq!(loose.requests, 1);
    assert_eq!(requests(&loose.items)[0].name, "GET https://a.test/x");
}

#[test]
fn template_syntax() {
    let mut importer = Importer::new(Format::V4);
    assert_eq!(
        importer.fix(
            "{{ _.a }}/{{_['b c']}}/{{ d }}/{% now 'unix' %}/{% now %}/{% uuid %}",
            "x"
        ),
        "{{a}}/{{b c}}/{{d}}/{{$timestamp}}/{{$isoTimestamp}}/{{$guid}}"
    );
    assert_eq!(
        importer.fix("{% base64 'encode', 'normal', 'ü😀' %} {%", "“R”"),
        "{% base64 'encode', 'normal', 'ü😀' %} {%"
    );
    assert_eq!(importer.fix("{{ unclosed", "x"), "{{ unclosed");
    importer.tag_warnings();
    assert_eq!(importer.c.warnings.len(), 1);
    assert!(importer.c.warnings[0].contains("{% base64 %} tag was kept as written in “R”"));
    assert_eq!(
        quoted_list(&["a".into(), "b".into(), "c".into()]),
        "“a”, “b” and “c”"
    );
}
