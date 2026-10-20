use std::path::PathBuf;

use super::*;
use crate::collection::{Folder, RequestItem};
use crate::format::{FolderDefaults, Manifest, Server};

fn request(method: HttpMethod, url: &str, name: &str, examples: Vec<Example>) -> Request {
    let mut r = Request::new(method, url);
    r.name = name.into();
    r.examples = examples;
    r
}

fn json_example(name: &str, status: u16, body: &str) -> Example {
    Example {
        status,
        body: body.into(),
        ..Example::new(name)
    }
}

fn project(requests: Vec<Request>) -> Collection {
    let mut manifest = Manifest::new("Shop ü😀");
    manifest.variables.insert("greeting".into(), "hej".into());
    Collection {
        root: PathBuf::from("/nowhere"),
        manifest,
        read_only: false,
        items: requests
            .into_iter()
            .map(|request| {
                Item::Request(RequestItem {
                    path: PathBuf::from(format!("{}.toml", crate::slug::slugify(&request.name))),
                    seq: None,
                    request,
                })
            })
            .collect(),
        environments: Vec::new(),
        broken_environments: Vec::new(),
        workflows: Vec::new(),
        broken_workflows: Vec::new(),
    }
}

fn answer(api: &MockApi, method: &str, target: &str) -> Answer {
    api.answer(&MockRequest::new(method, target))
}

fn example_name(answer: &Answer) -> &str {
    match &answer.outcome {
        Outcome::Example { example, .. } => example,
        Outcome::NoMatch(reason) => panic!("no match: {reason}"),
    }
}

fn body(answer: &Answer) -> String {
    String::from_utf8(answer.body.clone()).expect("utf-8")
}

/// R-MOCK-1: examples round-trip through the request file, readable and in a fixed order.
#[test]
fn examples_are_saved_in_the_request_file() {
    let mut example = json_example("found", 200, "{\"id\": 7, \"name\": \"ü😀\"}\n");
    example.headers.push(KeyValue::new("X-Rate", "10"));
    example.delay_ms = Some(250);
    example.conditions.query.insert("full".into(), "1".into());
    example
        .conditions
        .body
        .insert("$.kind".into(), "user".into());
    let r = request(
        HttpMethod::Get,
        "/users/:id",
        "Get user",
        vec![example, json_example("missing", 404, "")],
    );
    let text = crate::format::write_request(&r, None);
    assert!(
        text.ends_with(
            r#"[[examples]]
name = "found"
status = 200
delay_ms = 250
body = """
{"id": 7, "name": "ü😀"}
"""

[[examples.headers]]
name = "X-Rate"
value = "10"

[examples.match.query]
full = "1"

[examples.match.body]
"$.kind" = "user"

[[examples]]
name = "missing"
status = 404
"#
        ),
        "{text}"
    );
    let (back, _) = crate::format::parse_request(&text, "x").expect("parse");
    assert_eq!(back, r);
}

/// R-MOCK-1: a request file without examples still loads, and one with only a name
/// gets status 200.
#[test]
fn examples_default_to_none_and_status_200() {
    let (r, _) = crate::format::parse_request("url = \"/a\"\n", "a").expect("parse");
    assert_eq!(r.examples, Vec::<Example>::new());
    let (r, _) = crate::format::parse_request("url = \"/a\"\n[[examples]]\nname = \"ok\"\n", "a")
        .expect("parse");
    assert_eq!(r.examples, [Example::new("ok")]);
}

/// R-MOCK-1: saving a response as an example leaves out credentials and transfer headers.
#[test]
fn example_from_a_response_leaves_out_credentials() {
    let response = Response {
        status: 201,
        status_text: "Created".into(),
        headers: vec![
            KeyValue::new("Content-Type", "application/json"),
            KeyValue::new("Set-Cookie", "session=secret"),
            KeyValue::new("content-length", "2"),
            KeyValue::new("Location", "/users/7"),
        ],
        body: b"{}".to_vec(),
        elapsed: Duration::ZERO,
    };
    let example = Example::from_response("created", &response);
    assert_eq!(example.status, 201);
    assert_eq!(example.body, "{}");
    assert_eq!(
        example.headers,
        [
            KeyValue::new("Content-Type", "application/json"),
            KeyValue::new("Location", "/users/7")
        ]
    );
}

/// R-MOCK-2: method and path pick the request; `:id` and `{{id}}` match any segment,
/// and literal segments win over variable ones.
#[test]
fn method_and_path_pick_the_request() {
    let api = MockApi::new(
        &project(vec![
            request(
                HttpMethod::Get,
                "/users/:id",
                "Get user",
                vec![json_example("one", 200, "{}")],
            ),
            request(
                HttpMethod::Get,
                "/users/me",
                "Me",
                vec![json_example("me", 200, "{}")],
            ),
            request(
                HttpMethod::Delete,
                "/users/{{id}}",
                "Delete user",
                vec![json_example("gone", 204, "")],
            ),
            request(
                HttpMethod::Get,
                "{{baseUrl}}/orders/",
                "Orders",
                vec![json_example("orders", 200, "[]")],
            ),
            request(
                HttpMethod::Get,
                "https://api.test/v1/items",
                "Items",
                vec![json_example("items", 200, "[]")],
            ),
        ]),
        None,
    );
    assert_eq!(example_name(&answer(&api, "GET", "/users/42")), "one");
    assert_eq!(example_name(&answer(&api, "GET", "/users/me")), "me");
    assert_eq!(
        example_name(&answer(&api, "get", "/users/%C3%BC%F0%9F%98%80")),
        "one"
    );
    assert_eq!(example_name(&answer(&api, "DELETE", "/users/42")), "gone");
    assert_eq!(
        example_name(&answer(&api, "GET", "/orders?page=2")),
        "orders"
    );
    assert_eq!(example_name(&answer(&api, "GET", "/v1/items/")), "items");
}

/// R-MOCK-2: nothing matching is a 404 that says why and names the closest requests.
#[test]
fn unknown_requests_get_a_404_naming_the_closest() {
    let api = MockApi::new(
        &project(vec![
            request(
                HttpMethod::Get,
                "/users/:id",
                "Get user",
                vec![json_example("one", 200, "{}")],
            ),
            request(HttpMethod::Get, "/users", "List users", vec![]),
        ]),
        None,
    );
    let missing = answer(&api, "GET", "/users/42/orders");
    assert_eq!(missing.status, 404);
    assert!(matches!(&missing.outcome, Outcome::NoMatch(r) if r.contains("GET /users/42/orders")));
    let json: serde_json::Value = serde_json::from_slice(&missing.body).expect("json");
    assert_eq!(json["closest"][0], "GET /users/:id (Get user)");

    let wrong_method = answer(&api, "POST", "/users/42");
    assert!(
        body(&wrong_method).contains("another method"),
        "{}",
        body(&wrong_method)
    );
    let unknown_method = answer(&api, "BREW", "/users/42");
    assert_eq!(unknown_method.status, 404);

    let no_examples = answer(&api, "GET", "/users");
    assert!(
        matches!(&no_examples.outcome, Outcome::NoMatch(r) if r == "“List users” has no examples yet"),
        "{:?}",
        no_examples.outcome
    );
}

/// R-MOCK-3: the first example whose conditions hold answers; one without conditions
/// is the fallback; the header picks one by name.
#[test]
fn conditions_and_the_header_pick_the_example() {
    let mut admin = json_example("admin", 200, "{\"role\": \"admin\"}");
    admin
        .conditions
        .headers
        .insert("x-role".into(), "admin".into());
    let mut paged = json_example("page 2", 200, "[]");
    paged.conditions.query.insert("page".into(), "2".into());
    let mut big = json_example("big order", 201, "{}");
    big.conditions
        .body
        .insert("$.items[0].qty".into(), "100".into());
    let mut soap = json_example("soap", 200, "<ok/>");
    soap.conditions
        .body
        .insert("xpath://Id".into(), "42".into());
    let api = MockApi::new(
        &project(vec![
            request(
                HttpMethod::Get,
                "/users",
                "Users",
                vec![
                    admin,
                    paged,
                    json_example("fallback", 200, "[]"),
                    json_example("Not Found", 404, "{}"),
                ],
            ),
            request(
                HttpMethod::Post,
                "/orders",
                "Order",
                vec![big, soap, json_example("small", 201, "{}")],
            ),
        ]),
        None,
    );
    assert_eq!(example_name(&answer(&api, "GET", "/users")), "fallback");
    assert_eq!(
        example_name(&answer(&api, "GET", "/users?page=2")),
        "page 2"
    );
    let mut with_header = MockRequest::new("GET", "/users");
    with_header.headers.push(KeyValue::new("X-ROLE", " admin "));
    assert_eq!(example_name(&api.answer(&with_header)), "admin");

    let mut pick = MockRequest::new("GET", "/users?page=2");
    pick.headers
        .push(KeyValue::new(EXAMPLE_HEADER, "not found"));
    let picked = api.answer(&pick);
    assert_eq!((picked.status, example_name(&picked)), (404, "Not Found"));
    pick.headers[0].value = "nope".into();
    assert!(body(&api.answer(&pick)).contains("no example named “nope”"));

    let mut order = MockRequest::new("POST", "/orders");
    order
        .headers
        .push(KeyValue::new("Content-Type", "application/json"));
    order.body = br#"{"items": [{"qty": 100}]}"#.to_vec();
    assert_eq!(example_name(&api.answer(&order)), "big order");
    order.body = br#"{"items": [{"qty": 1}]}"#.to_vec();
    assert_eq!(example_name(&api.answer(&order)), "small");
    order.body = b"not json".to_vec();
    assert_eq!(
        example_name(&api.answer(&order)),
        "small",
        "a body that can't be read doesn't match"
    );
    order.body = b"<Envelope><Body><Id>42</Id></Body></Envelope>".to_vec();
    assert_eq!(example_name(&api.answer(&order)), "soap");
}

/// R-MOCK-2: variables are filled from the project and environment; secrets and unknown
/// names stay as written; the Content-Type is guessed when the example has none.
#[test]
fn answers_fill_variables_and_content_type() {
    let mut example = json_example(
        "hello",
        200,
        "{\"msg\": \"{{greeting}} {{token}} {{nope}}\"}",
    );
    example
        .headers
        .push(KeyValue::new("X-Greeting", "{{greeting}}"));
    example.headers.push(KeyValue {
        enabled: false,
        ..KeyValue::new("X-Off", "1")
    });
    example.delay_ms = Some(30);
    let mut collection = project(vec![
        request(HttpMethod::Get, "/hello", "Hello", vec![example]),
        request(
            HttpMethod::Get,
            "/xml",
            "Xml",
            vec![json_example("x", 200, "<a/>")],
        ),
        request(
            HttpMethod::Get,
            "/text",
            "Text",
            vec![json_example("t", 200, "hi ü😀")],
        ),
        request(
            HttpMethod::Get,
            "/typed",
            "Typed",
            vec![Example {
                headers: vec![KeyValue::new("content-type", "text/csv")],
                ..json_example("csv", 200, "a,b")
            }],
        ),
    ]);
    collection.manifest.secrets.push("token".into());
    collection.environments.push(Environment {
        id: "dev".into(),
        name: "Dev".into(),
        variables: [("greeting".to_owned(), "hello from dev".to_owned())].into(),
        ..Environment::default()
    });
    let api = MockApi::new(&collection, Some("dev"));
    let hello = answer(&api, "GET", "/hello");
    assert_eq!(
        body(&hello),
        "{\"msg\": \"hello from dev {{token}} {{nope}}\"}"
    );
    assert_eq!(
        hello.headers,
        [
            KeyValue::new("X-Greeting", "hello from dev"),
            KeyValue::new("Content-Type", "application/json")
        ]
    );
    assert_eq!(hello.delay, Some(Duration::from_millis(30)));
    let content_type = |path| answer(&api, "GET", path).headers[0].value.clone();
    assert_eq!(content_type("/xml"), "application/xml");
    assert_eq!(content_type("/text"), "text/plain; charset=utf-8");
    assert_eq!(content_type("/typed"), "text/csv");
}

/// R-MOCK-2, R-MOCK-4: requests of folders are served too, and with several servers each
/// is reached under `/<server id>`; a path without the prefix still matches.
#[test]
fn several_servers_are_told_apart_by_prefix() {
    let mut users = request(
        HttpMethod::Get,
        "/status",
        "Users status",
        vec![json_example("users", 200, "{}")],
    );
    users.server = Some("users".into());
    let mut orders = request(
        HttpMethod::Get,
        "/status",
        "Orders status",
        vec![json_example("orders", 200, "{}")],
    );
    orders.server = Some("orders".into());
    let mut collection = project(vec![]);
    collection.items = vec![Item::Folder(Folder {
        path: PathBuf::from("api"),
        name: "API".into(),
        seq: None,
        items: project(vec![users, orders]).items,
        defaults: FolderDefaults::default(),
        error: None,
    })];
    collection.manifest.servers = vec![
        Server {
            id: "users".into(),
            name: "Users".into(),
        },
        Server {
            id: "orders".into(),
            name: "Orders".into(),
        },
    ];
    let api = MockApi::new(&collection, None);
    assert_eq!(api.example_count(), 2);
    assert_eq!(
        example_name(&answer(&api, "GET", "/orders/status")),
        "orders"
    );
    assert_eq!(example_name(&answer(&api, "GET", "/users/status")), "users");
    assert_eq!(
        example_name(&answer(&api, "GET", "/status")),
        "users",
        "first in the tree"
    );

    let env = environment(&collection, "http://127.0.0.1:4010/");
    assert_eq!(env.id, ENVIRONMENT_ID);
    assert_eq!(env.servers["orders"], "http://127.0.0.1:4010/orders");
    collection.manifest.servers.pop();
    let env = environment(&collection, "http://127.0.0.1:4010");
    assert_eq!(
        env.servers["users"], "http://127.0.0.1:4010",
        "one server: no prefix"
    );
}

/// R-MOCK-2: an empty project answers everything with a 404 and no closest requests.
#[test]
fn an_empty_project_answers_404() {
    let api = MockApi::new(&project(vec![]), None);
    let a = answer(&api, "GET", "/");
    assert_eq!(a.status, 404);
    let json: serde_json::Value = serde_json::from_slice(&a.body).expect("json");
    assert_eq!(json["closest"], serde_json::json!([]));
}
