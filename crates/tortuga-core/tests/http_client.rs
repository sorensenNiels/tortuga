//! HTTP behaviour of `tortuga_core::Client`, against a local mock server (never the internet).

use std::time::Duration;

use tortuga_core::{Client, ClientSettings, Error, HttpMethod, KeyValue, Request, RequestBody};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn server_replying(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

async fn only_received(server: &MockServer) -> wiremock::Request {
    let mut received = server.received_requests().await.expect("recording enabled");
    assert_eq!(received.len(), 1, "expected exactly one request");
    received.remove(0)
}

fn header_values(req: &wiremock::Request, name: &str) -> Vec<String> {
    req.headers
        .get_all(name)
        .iter()
        .map(|v| v.to_str().expect("ascii").to_owned())
        .collect()
}

/// R-HTTP-1
#[tokio::test]
async fn sends_every_supported_method() {
    for m in HttpMethod::ALL {
        let server = server_replying(204).await;
        let res = Client::default()
            .execute(&Request::new(m, server.uri()))
            .await
            .expect("request succeeds");
        assert_eq!(res.status, 204);
        assert_eq!(only_received(&server).await.method.as_str(), m.as_str());
    }
}

/// R-HTTP-2
#[tokio::test]
async fn sends_only_enabled_params_encoded() {
    let server = server_replying(200).await;
    let mut req = Request::new(
        HttpMethod::Get,
        format!("{}/search?q=a b&x=1", server.uri()),
    );
    req.params.push(KeyValue {
        enabled: false,
        ..KeyValue::new("off", "1")
    });
    req.params.push(KeyValue::new("amp", "a&b"));

    Client::default()
        .execute(&req)
        .await
        .expect("request succeeds");

    let got = only_received(&server).await;
    assert_eq!(got.url.path(), "/search");
    assert_eq!(got.url.query(), Some("q=a%20b&x=1&amp=a%26b"));
}

/// R-HTTP-3
#[tokio::test]
async fn sends_enabled_headers_including_duplicates() {
    let server = server_replying(200).await;
    let mut req = Request::new(HttpMethod::Get, server.uri());
    req.headers.push(KeyValue::new("X-Dup", "one"));
    req.headers.push(KeyValue::new("X-Dup", "two"));
    req.headers.push(KeyValue {
        enabled: false,
        ..KeyValue::new("X-Off", "no")
    });
    req.headers
        .push(KeyValue::new("", "blank names are ignored"));

    Client::default()
        .execute(&req)
        .await
        .expect("request succeeds");

    let got = only_received(&server).await;
    assert_eq!(header_values(&got, "x-dup"), ["one", "two"]);
    assert_eq!(header_values(&got, "x-off"), Vec::<String>::new());
}

/// R-HTTP-4: each body type gets the right automatic Content-Type.
#[tokio::test]
async fn body_types_set_content_type_automatically() {
    let cases = [
        (
            RequestBody::Text("hi".into()),
            "text/plain; charset=utf-8",
            "hi",
        ),
        (
            RequestBody::Json(r#"{"a":1}"#.into()),
            "application/json",
            r#"{"a":1}"#,
        ),
        (
            RequestBody::Form(vec![
                KeyValue::new("name", "Ada L"),
                KeyValue::new("n", "1"),
            ]),
            "application/x-www-form-urlencoded",
            "name=Ada+L&n=1",
        ),
    ];
    for (body, content_type, sent) in cases {
        let server = server_replying(200).await;
        let mut req = Request::new(HttpMethod::Post, server.uri());
        req.body = body;
        Client::default()
            .execute(&req)
            .await
            .expect("request succeeds");

        let got = only_received(&server).await;
        assert_eq!(header_values(&got, "content-type"), [content_type]);
        assert_eq!(String::from_utf8(got.body).expect("utf8"), sent);
    }
}

/// R-HTTP-4: a user-set Content-Type wins and is not duplicated.
#[tokio::test]
async fn explicit_content_type_overrides_automatic_one() {
    let server = server_replying(200).await;
    let mut req = Request::new(HttpMethod::Post, server.uri());
    req.headers
        .push(KeyValue::new("content-type", "application/vnd.api+json"));
    req.body = RequestBody::Json("{}".into());

    Client::default()
        .execute(&req)
        .await
        .expect("request succeeds");

    let got = only_received(&server).await;
    assert_eq!(
        header_values(&got, "content-type"),
        ["application/vnd.api+json"]
    );
}

/// R-HTTP-6: status, headers, body and timing are captured.
#[tokio::test]
async fn captures_response_details() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/teapot"))
        .respond_with(
            ResponseTemplate::new(418)
                .insert_header("x-custom", "yes")
                .set_body_raw(r#"{"short":"stout"}"#, "application/json"),
        )
        .mount(&server)
        .await;

    let req = Request::new(HttpMethod::Get, format!("{}/teapot", server.uri()));
    let res = Client::default()
        .execute(&req)
        .await
        .expect("request succeeds");

    assert_eq!(
        (res.status, res.status_text.as_str()),
        (418, "I'm a teapot")
    );
    assert!(!res.is_success());
    assert_eq!(res.header("X-Custom"), Some("yes"));
    assert_eq!(res.body, br#"{"short":"stout"}"#);
    assert!(matches!(res.body_view(), tortuga_core::BodyView::Json(_)));
}

/// R-HTTP-7: the request's own timeout wins over the default.
#[tokio::test]
async fn per_request_timeout() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let mut req = Request::new(HttpMethod::Get, server.uri());
    req.timeout_ms = Some(100);
    let err = Client::default()
        .execute(&req)
        .await
        .expect_err("must time out");
    assert!(
        matches!(err, Error::Timeout(d) if d == Duration::from_millis(100)),
        "{err}"
    );
    assert_eq!(err.to_string(), "request timed out after 100 ms");
}

/// R-HTTP-7: the global default applies when the request has none.
#[tokio::test]
async fn default_timeout_from_settings() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let settings = ClientSettings {
        default_timeout_ms: 150,
        ..ClientSettings::default()
    };
    let client = Client::new(settings).expect("client");
    let err = client
        .execute(&Request::new(HttpMethod::Get, server.uri()))
        .await
        .expect_err("must time out");
    assert!(
        matches!(err, Error::Timeout(d) if d == Duration::from_millis(150)),
        "{err}"
    );
}

/// R-HTTP-7: cancelling = dropping the future; it must return immediately.
#[tokio::test]
async fn dropping_the_future_cancels_the_request() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;

    let client = Client::default();
    let req = Request::new(HttpMethod::Get, server.uri());
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(Duration::from_millis(100), client.execute(&req)).await;
    assert!(outcome.is_err(), "request should still have been in flight");
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// R-HTTP-8: redirects are followed by default and can be switched off.
#[tokio::test]
async fn redirect_setting() {
    let server = MockServer::start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("moved here"))
        .mount(&server)
        .await;
    let req = Request::new(HttpMethod::Get, format!("{}/old", server.uri()));

    let followed = Client::default()
        .execute(&req)
        .await
        .expect("request succeeds");
    assert_eq!(
        (followed.status, followed.body.as_slice()),
        (200, b"moved here".as_slice())
    );

    let settings = ClientSettings {
        follow_redirects: false,
        ..ClientSettings::default()
    };
    let not_followed = Client::new(settings)
        .expect("client")
        .execute(&req)
        .await
        .expect("ok");
    assert_eq!(not_followed.status, 302);
    assert_eq!(not_followed.header("location"), Some("/new"));
}

#[tokio::test]
async fn invalid_url_is_reported() {
    let req = Request::new(HttpMethod::Get, "not a url");
    let err = Client::default()
        .execute(&req)
        .await
        .expect_err("must fail");
    assert!(matches!(err, Error::InvalidUrl { .. }), "{err}");
}

#[tokio::test]
async fn invalid_header_is_reported() {
    let mut req = Request::new(HttpMethod::Get, "http://127.0.0.1:9");
    req.headers.push(KeyValue::new("X-Bad", "line\nbreak"));
    let err = Client::default()
        .execute(&req)
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, Error::InvalidHeader { ref name, .. } if name == "X-Bad"),
        "{err}"
    );
}

#[tokio::test]
async fn connection_errors_include_the_cause() {
    // Bind then drop a listener to get a port nothing listens on.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();
    let req = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/"));
    let err = Client::default()
        .execute(&req)
        .await
        .expect_err("must fail");
    let message = err.to_string().to_lowercase();
    assert!(matches!(err, Error::Transport(_)), "{err}");
    assert!(
        message.contains("refused") || message.contains("connect"),
        "{message}"
    );
}

/// R-HTTP-5, R-COL-4, R-VAR-3: collection auth and headers, with a secret token, reach the server.
#[tokio::test]
async fn sends_inherited_auth_and_headers_with_secret_values() {
    use std::collections::BTreeMap;

    use tortuga_core::Auth;
    use tortuga_core::vars::{self, Context};

    let server = server_replying(200).await;
    let ctx = Context {
        headers: vec![KeyValue::new("X-Team", "tortuga")],
        auth: Auth::Bearer {
            token: "{{token}}".into(),
        },
        ..Context::default()
    }
    .with_secrets(BTreeMap::from([("token".into(), "s3cret".into())]));
    let request = Request::new(HttpMethod::Get, server.uri());

    let prepared = vars::prepare(&request, &ctx).expect("prepared");
    Client::default()
        .execute(&prepared)
        .await
        .expect("request succeeds");

    let received = only_received(&server).await;
    assert_eq!(header_values(&received, "authorization"), ["Bearer s3cret"]);
    assert_eq!(header_values(&received, "x-team"), ["tortuga"]);
}

/// R-HTTP-5
#[tokio::test]
async fn sends_basic_auth() {
    use tortuga_core::Auth;
    use tortuga_core::vars::{self, Context};

    let server = server_replying(200).await;
    let mut request = Request::new(HttpMethod::Get, server.uri());
    request.auth = Some(Auth::Basic {
        username: "user".into(),
        password: "pass".into(),
    });
    let prepared = vars::prepare(&request, &Context::default()).expect("prepared");
    Client::default().execute(&prepared).await.expect("sent");
    assert_eq!(
        header_values(&only_received(&server).await, "authorization"),
        ["Basic dXNlcjpwYXNz"]
    );
}

/// R-HTTP-10: path variables are filled (encoded) before sending.
#[tokio::test]
async fn sends_filled_path_variables() {
    use tortuga_core::vars::{self, Context};

    let server = server_replying(200).await;
    let mut request = Request::new(
        HttpMethod::Get,
        format!("{}/users/:id/files/:name", server.uri()),
    );
    request.path_vars[0].value = "42".into();
    request.path_vars[1].value = "my report.pdf".into();
    let prepared = vars::prepare(&request, &Context::default()).expect("prepared");
    Client::default().execute(&prepared).await.expect("sent");
    assert_eq!(
        only_received(&server).await.url.path(),
        "/users/42/files/my%20report.pdf"
    );
}

/// A server that sets a session cookie at `/login` (via a redirect to `/home`)
/// and answers 200 everywhere else.
async fn server_with_login() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/login"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("set-cookie", "session=abc; Path=/")
                .insert_header("location", "/home"),
        )
        .mount(&server)
        .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

fn cookie_headers_received(requests: &[wiremock::Request], at: &str) -> Vec<String> {
    requests
        .iter()
        .filter(|r| r.url.path() == at)
        .flat_map(|r| header_values(r, "cookie"))
        .collect()
}

/// R-CK-1: cookies from responses (also during redirects) are sent with later requests.
#[tokio::test]
async fn cookie_jar_keeps_cookies_set_during_a_redirect() {
    let server = server_with_login().await;
    let jar = tortuga_core::CookieJar::default();
    let client = Client::default();
    let login = Request::new(HttpMethod::Get, format!("{}/login", server.uri()));
    let res = client
        .execute_with_cookies(&login, Some(&jar))
        .await
        .expect("login");
    assert_eq!(res.status, 200, "redirect followed");
    let me = Request::new(HttpMethod::Get, format!("{}/me", server.uri()));
    client
        .execute_with_cookies(&me, Some(&jar))
        .await
        .expect("me");

    let received = server.received_requests().await.expect("recording");
    assert_eq!(
        cookie_headers_received(&received, "/home"),
        ["session=abc"],
        "redirect hop"
    );
    assert_eq!(cookie_headers_received(&received, "/me"), ["session=abc"]);
    assert_eq!(jar.len(), 1);
}

/// R-CK-2: with cookies off, nothing is sent from the jar or stored in it.
#[tokio::test]
async fn requests_with_cookies_off_ignore_the_jar() {
    let server = server_with_login().await;
    let jar = tortuga_core::CookieJar::default();
    let client = Client::default();
    let mut login = Request::new(HttpMethod::Get, format!("{}/login", server.uri()));
    login.cookies = false;
    client
        .execute_with_cookies(&login, Some(&jar))
        .await
        .expect("login");
    assert!(jar.is_empty(), "response cookies are not stored");

    jar.store_response_cookies(
        ["session=abc"],
        &url::Url::parse(&server.uri()).expect("url"),
    );
    let mut me = Request::new(HttpMethod::Get, format!("{}/me", server.uri()));
    me.cookies = false;
    client
        .execute_with_cookies(&me, Some(&jar))
        .await
        .expect("me");
    let received = server.received_requests().await.expect("recording");
    assert!(
        cookie_headers_received(&received, "/me").is_empty(),
        "jar cookies are not sent"
    );
}

/// R-CK-4: a Cookie header set by hand is always sent, also with cookies off.
#[tokio::test]
async fn manual_cookie_header_is_always_sent() {
    let server = server_with_login().await;
    let jar = tortuga_core::CookieJar::default();
    jar.store_response_cookies(
        ["session=abc"],
        &url::Url::parse(&server.uri()).expect("url"),
    );
    let client = Client::default();
    for cookies in [true, false] {
        let mut me = Request::new(HttpMethod::Get, format!("{}/me", server.uri()));
        me.cookies = cookies;
        me.headers.push(KeyValue::new("Cookie", "mine=1"));
        client
            .execute_with_cookies(&me, Some(&jar))
            .await
            .expect("me");
    }
    let received = server.received_requests().await.expect("recording");
    assert_eq!(
        cookie_headers_received(&received, "/me"),
        ["mine=1", "mine=1"]
    );
}
