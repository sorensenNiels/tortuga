//! A request run with its scripts and captures (R-TEST-1..5), against a mock server.

use std::collections::BTreeMap;

use tortuga_core::runner::{Extra, capture, run_step};
use tortuga_core::vars::Context;
use tortuga_core::{Client, HttpMethod, KeyValue, Request, Response};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ctx(vars: &[(&str, &str)]) -> Context {
    Context {
        variables: vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        ..Context::default()
    }
}

/// R-TEST-2, R-TEST-4, R-TEST-1: the pre-request script sets a variable and a header,
/// the response is captured and tested.
#[tokio::test]
async fn pre_request_capture_and_tests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tenants/acme/login"))
        .and(header("x-signature", "signed-acme"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-trace", "t-1")
                .set_body_json(
                    serde_json::json!({ "access_token": "tok-123", "user": { "id": 7 } }),
                ),
        )
        .mount(&server)
        .await;

    let mut req = Request::new(
        HttpMethod::Post,
        format!("{}/tenants/{{{{tenant}}}}/login", server.uri()),
    );
    req.scripts.pre_request = r#"
        vars.set("tenant", "acme");
        req.setHeader("X-Signature", "signed-" + vars.get("tenant"));
    "#
    .into();
    req.capture.insert("token".into(), "$.access_token".into());
    req.capture.insert("trace".into(), "header:X-Trace".into());
    req.scripts.tests = r#"
        test("ok", () => expect(res.status).toBe(200));
        test("token captured", () => expect(vars.get("token")).toBe("tok-123"));
        vars.set("user_id", res.json().user.id);
        console.log("logged in");
    "#
    .into();
    let step_capture = BTreeMap::from([("status".to_owned(), "status".to_owned())]);
    let out = run_step(
        &Client::default(),
        None,
        &req,
        &ctx(&[]),
        Extra {
            capture: Some(&step_capture),
            tests: Some(r#"test("step check", () => expect(vars.get("status")).toBe("200"));"#),
        },
    )
    .await;

    assert_eq!(out.errors, Vec::<String>::new());
    assert!(!out.failed(), "{:?}", out.failure());
    let names: Vec<(&str, bool)> = out
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed))
        .collect();
    assert_eq!(
        names,
        [("ok", true), ("token captured", true), ("step check", true)]
    );
    let set = |v: &str| Some(v.to_owned());
    assert_eq!(
        out.vars,
        BTreeMap::from([
            ("status".to_owned(), set("200")),
            ("tenant".to_owned(), set("acme")),
            ("token".to_owned(), set("tok-123")),
            ("trace".to_owned(), set("t-1")),
            ("user_id".to_owned(), set("7")),
        ])
    );
    assert_eq!(out.logs, ["logged in"]);
    assert_eq!(
        out.sent.as_ref().map(|r| r.url.clone()),
        Some(format!("{}/tenants/acme/login", server.uri()))
    );
}

/// R-FLOW-3: what counts as a failed step.
#[tokio::test]
async fn failures() {
    let server = MockServer::start().await;
    Mock::given(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("plain"))
        .mount(&server)
        .await;
    let client = Client::default();
    let run = |url: String, setup: fn(&mut Request)| {
        let client = client.clone();
        async move {
            let mut r = Request::new(HttpMethod::Get, url);
            setup(&mut r);
            run_step(&client, None, &r, &ctx(&[]), Extra::default()).await
        }
    };

    let not_found = run(format!("{}/missing", server.uri()), |_| {}).await;
    assert_eq!(not_found.failure().as_deref(), Some("404 Not Found"));

    let failed_test = run(format!("{}/ok", server.uri()), |r| {
        r.scripts.tests =
            r#"test("is json", () => expect(res.header("content-type")).toMatch(/json/));"#.into();
    })
    .await;
    assert!(
        failed_test.failure().is_some_and(|f| f.contains("is json")),
        "{:?}",
        failed_test.failure()
    );

    let bad_capture = run(format!("{}/ok", server.uri()), |r| {
        r.capture.insert("id".into(), "$.id".into());
    })
    .await;
    assert!(
        bad_capture
            .failure()
            .is_some_and(|f| f.contains("not JSON")),
        "{:?}",
        bad_capture.failure()
    );

    let script_error = run(format!("{}/ok", server.uri()), |r| {
        r.scripts.pre_request = "throw new Error('no key')".into();
    })
    .await;
    assert!(script_error.response.is_none(), "not sent");
    assert!(
        script_error.failure().is_some_and(|f| f.contains("no key")),
        "{:?}",
        script_error.failure()
    );

    let undefined = run(format!("{}/{{{{nothing}}}}", server.uri()), |_| {}).await;
    assert!(
        undefined.failure().is_some_and(|f| f.contains("nothing")),
        "{:?}",
        undefined.failure()
    );
}

#[test]
fn captures_from_json_headers_and_status() {
    let res = Response {
        status: 201,
        status_text: "Created".into(),
        headers: vec![KeyValue::new("Location", "/users/9")],
        body: br#"{"items": [{"id": 1}, {"id": 2}], "name": "x", "meta": {"n": 2}}"#.to_vec(),
        elapsed: std::time::Duration::ZERO,
    };
    assert_eq!(capture("status", &res).as_deref(), Ok("201"));
    assert_eq!(capture("header:location", &res).as_deref(), Ok("/users/9"));
    assert_eq!(capture("$.name", &res).as_deref(), Ok("x"));
    assert_eq!(
        capture("$.items[*].id", &res).as_deref(),
        Ok("1"),
        "the first match"
    );
    assert_eq!(capture("$.meta", &res).as_deref(), Ok(r#"{"n":2}"#));
    assert!(capture("$.nope", &res).is_err());
    assert!(capture("header:X-None", &res).is_err());
    assert!(capture("$[", &res).is_err());
}
