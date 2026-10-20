//! The mock server end to end (R-MOCK-2, R-MOCK-3): a project on disk, served on a
//! free port, called with Tortuga's own client.

use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tortuga_core::mock::server;
use tortuga_core::mock::{Example, Hit, MockApi, Outcome};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Client, Collection, Error, HttpMethod, KeyValue, Request, RequestBody};

fn project(dir: &Path) -> Collection {
    let collection = Collection::create(&FsStorage, dir, "Shop").expect("create");
    let mut get = Request::new(HttpMethod::Get, "http://localhost/users/:id");
    get.name = "Get user".into();
    get.examples = vec![
        Example {
            body: "{\"id\": 7, \"name\": \"Åse ü😀\"}".into(),
            headers: vec![KeyValue::new("X-Mock", "yes")],
            ..Example::new("found")
        },
        Example {
            status: 404,
            delay_ms: Some(150),
            ..Example::new("missing")
        },
    ];
    collection
        .new_request(&FsStorage, Path::new(""), &get)
        .expect("save");
    Collection::open(&FsStorage, dir).expect("open")
}

async fn send(
    base: &str,
    method: HttpMethod,
    path: &str,
    headers: &[(&str, &str)],
) -> tortuga_core::Response {
    let mut request = Request::new(method, format!("{base}{path}"));
    request.headers = headers.iter().map(|(n, v)| KeyValue::new(*n, *v)).collect();
    request.body = RequestBody::Json("{}".into());
    Client::default().execute(&request).await.expect("send")
}

#[tokio::test]
async fn serves_examples_and_logs_each_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let api = Arc::new(RwLock::new(MockApi::new(&project(dir.path()), None)));
    let hits: Arc<Mutex<Vec<Hit>>> = Arc::default();
    let log = Arc::clone(&hits);
    let mock = server::start(0, Arc::clone(&api), move |hit| {
        log.lock().expect("log").push(hit);
    })
    .await
    .expect("start");
    let base = mock.base_url();
    assert!(base.starts_with("http://127.0.0.1:"), "{base}");

    let found = send(&base, HttpMethod::Get, "/users/7?x=1", &[]).await;
    assert_eq!(found.status, 200);
    assert_eq!(found.header("x-mock"), Some("yes"));
    assert_eq!(found.content_type(), Some("application/json"));
    assert_eq!(
        String::from_utf8(found.body).expect("utf-8"),
        "{\"id\": 7, \"name\": \"Åse ü😀\"}"
    );

    let missing = send(
        &base,
        HttpMethod::Get,
        "/users/7",
        &[("X-Tortuga-Example", "missing")],
    )
    .await;
    assert_eq!(missing.status, 404);
    assert!(
        missing.elapsed >= Duration::from_millis(150),
        "delay_ms holds it back"
    );

    let unknown = send(&base, HttpMethod::Post, "/orders", &[]).await;
    assert_eq!(unknown.status, 404);
    assert!(String::from_utf8_lossy(&unknown.body).contains("POST /orders"));

    let hits = hits.lock().expect("log").clone();
    let seen: Vec<(&str, &str, u16)> = hits
        .iter()
        .map(|h| (h.method.as_str(), h.target.as_str(), h.status))
        .collect();
    assert_eq!(
        seen,
        [
            ("GET", "/users/7?x=1", 200),
            ("GET", "/users/7", 404),
            ("POST", "/orders", 404)
        ]
    );
    assert!(
        matches!(&hits[0].outcome, Outcome::Example { example, request_name, .. }
        if example == "found" && request_name == "Get user")
    );
    assert!(matches!(hits[2].outcome, Outcome::NoMatch(_)));

    // Replacing the answers (as the app does after a save or `git pull`) takes effect at once.
    *api.write().expect("api") = MockApi::default();
    assert_eq!(
        send(&base, HttpMethod::Get, "/users/7", &[]).await.status,
        404
    );
}

#[tokio::test]
async fn an_example_http_cannot_carry_is_a_500_saying_why() {
    let dir = tempfile::tempdir().expect("tempdir");
    let collection = Collection::create(&FsStorage, dir.path(), "Broken").expect("create");
    let mut r = Request::new(HttpMethod::Get, "/x");
    r.name = "X".into();
    r.examples = vec![Example {
        headers: vec![KeyValue::new("Bad Header", "1")],
        ..Example::new("bad")
    }];
    collection
        .new_request(&FsStorage, Path::new(""), &r)
        .expect("save");
    let api = MockApi::new(
        &Collection::open(&FsStorage, dir.path()).expect("open"),
        None,
    );
    let mock = server::start(0, Arc::new(RwLock::new(api)), |_| {})
        .await
        .expect("start");
    let answer = send(&mock.base_url(), HttpMethod::Get, "/x", &[]).await;
    assert_eq!(answer.status, 500);
    assert!(String::from_utf8_lossy(&answer.body).contains("can't be sent as HTTP"));
}

#[tokio::test]
async fn a_port_in_use_is_an_error_and_stopping_frees_it() {
    let api = Arc::new(RwLock::new(MockApi::default()));
    let first = server::start(0, Arc::clone(&api), |_| {})
        .await
        .expect("start");
    let port = first.port();
    let err = server::start(port, Arc::clone(&api), |_| {})
        .await
        .expect_err("in use");
    assert!(matches!(err, Error::MockServer { port: p, .. } if p == port));
    assert!(
        err.to_string()
            .starts_with(&format!("could not start the mock server on port {port}"))
    );

    drop(first);
    // The listener closes once its task sees the stop.
    let mut again = None;
    for _ in 0..50 {
        if let Ok(mock) = server::start(port, Arc::clone(&api), |_| {}).await {
            again = Some(mock);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(again.is_some(), "port {port} is free again after stopping");
}
