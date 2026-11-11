use std::path::PathBuf;

use super::*;
use crate::collection::RequestItem;
use crate::format::Manifest;
use crate::model::{HttpMethod, KeyValue};

fn project(requests: Vec<(&str, Request)>) -> Collection {
    Collection {
        root: PathBuf::from("/p"),
        manifest: Manifest::new("shop"),
        read_only: false,
        items: requests
            .into_iter()
            .map(|(path, request)| {
                Item::Request(RequestItem {
                    path: path.into(),
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

fn request(method: HttpMethod, url: &str) -> Request {
    let mut r = Request::new(method, url);
    r.name = "Orders ü😀".into();
    r
}

fn texts(d: &ApiDiff) -> Vec<String> {
    d.requests
        .iter()
        .flat_map(|r| r.changes.iter())
        .map(|c| format!("{}{}", if c.breaking { "!" } else { "" }, c.text))
        .collect()
}

/// Changes of one request edited from `a` to `b`.
fn changes(a: Request, b: Request) -> Vec<String> {
    texts(&diff(
        Some(&project(vec![("r.toml", a)])),
        Some(&project(vec![("r.toml", b)])),
    ))
}

#[test]
fn requests_are_added_removed_moved_and_changed() {
    // R-CLI-7
    let list = request(HttpMethod::Get, "{{base_url}}/orders");
    let mut paged = list.clone();
    paged.params.push(KeyValue::new("page", "1"));
    let before = project(vec![
        ("orders/list.toml", list.clone()),
        (
            "orders/delete.toml",
            request(HttpMethod::Delete, "{{base_url}}/orders/:id"),
        ),
        (
            "old/get.toml",
            request(HttpMethod::Get, "https://x.io/orders/:id?full=1"),
        ),
    ]);
    let after = project(vec![
        ("orders/list.toml", paged),
        (
            "orders/get.toml",
            request(HttpMethod::Get, "{{base_url}}/orders/:id"),
        ),
        (
            "orders/refund.toml",
            request(HttpMethod::Post, "{{base_url}}/orders/:id/refund"),
        ),
    ]);
    let d = diff(Some(&before), Some(&after));
    let rows: Vec<(String, &Status)> = d
        .requests
        .iter()
        .map(|r| (format!("{} {} {}", r.file, r.method, r.path), &r.status))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "orders/delete.toml DELETE /orders/:id".into(),
                &Status::Removed
            ),
            (
                "orders/get.toml GET /orders/:id".into(),
                &Status::Moved {
                    from: "old/get.toml".into()
                }
            ),
            ("orders/list.toml GET /orders".into(), &Status::Changed),
            (
                "orders/refund.toml POST /orders/:id/refund".into(),
                &Status::Added
            ),
        ]
    );
    assert_eq!(
        texts(&d),
        [
            "base URL changed",
            "!param `full` removed",
            "param `page` added"
        ]
    );
    assert_eq!(
        d.breaking(),
        2,
        "the removed one, and the moved one lost a param"
    );
}

#[test]
fn a_new_or_gone_project_lists_every_request() {
    // R-CLI-7
    let p = project(vec![("a.toml", request(HttpMethod::Get, "/a"))]);
    assert_eq!(diff(None, Some(&p)).requests[0].status, Status::Added);
    assert_eq!(diff(Some(&p), None).requests[0].status, Status::Removed);
    assert_eq!(diff(None, None), ApiDiff::default());
    assert_eq!(diff(Some(&p), Some(&p)), ApiDiff::default());
}

#[test]
fn edits_a_client_does_not_see_are_not_changes() {
    // R-CLI-7: descriptions, scripts, captures and values are not compared.
    let mut a = request(HttpMethod::Get, "/a");
    a.params.push(KeyValue::new("page", "1"));
    let mut b = a.clone();
    b.name = "Renamed".into();
    b.description = "Now documented".into();
    b.scripts.tests = "tortuga.test('ok', () => {})".into();
    b.params[0].value = "2".into();
    assert_eq!(changes(a, b), Vec::<String>::new());
}

#[test]
fn method_path_params_headers_and_auth() {
    // R-CLI-7, D77: removed, renamed and changed may break clients; added does not.
    let mut a = request(HttpMethod::Get, "/orders");
    a.params.push(KeyValue::new("page", "1"));
    a.params.push(KeyValue::new("sort", "id"));
    a.headers.push(KeyValue::new("X-Old", "1"));
    a.headers
        .push(KeyValue::new("Authorization", "Bearer secret-before"));
    let mut b = request(HttpMethod::Post, "/v2/orders");
    b.params.push(KeyValue::new("cursor", "x"));
    b.params.push(KeyValue::new("sort", "id"));
    b.headers
        .push(KeyValue::new("authorization", "Bearer secret-after"));
    b.headers.push(KeyValue::new("X-New", "1"));
    b.headers.push(KeyValue::new("X-Newer", "1"));
    b.auth = Some(Auth::Bearer {
        token: "secret-token".into(),
    });
    let found = changes(a, b);
    assert_eq!(
        found,
        [
            "!method `GET` → `POST`",
            "!path `/orders` → `/v2/orders`",
            "!param `page` renamed to `cursor`",
            "!header `X-Old` removed",
            "header `X-New` added",
            "header `X-Newer` added",
            "header `Authorization` changed",
            "!auth inherited → Bearer",
        ]
    );
    assert!(
        found.iter().all(|t| !t.contains("secret")),
        "values are never shown: {found:?}"
    );
}

#[test]
fn json_bodies_are_compared_by_shape_also_with_variables() {
    // R-CLI-7
    let with = |body: &str| {
        let mut r = request(HttpMethod::Post, "/orders");
        r.body = RequestBody::Json(body.into());
        r
    };
    let base = with(r#"{"id": {{id}}, "qty": 1, "note": "a {{b}} \" c", "items": [{"sku": "x"}]}"#);
    let typed =
        with(r#"{"id": {{id}}, "qty": "1", "note": "z", "items": [{"sku": "x", "ü😀": 1}]}"#);
    assert_eq!(
        changes(base.clone(), typed),
        [
            "!body: `$.qty` number → string",
            "body: `$.items[*]['ü😀']` added (number)",
        ]
    );
    let values = with(r#"{"id": {{id}}, "qty": 2, "note": "", "items": []}"#);
    assert_eq!(
        changes(base.clone(), values),
        Vec::<String>::new(),
        "values only"
    );
    assert_eq!(changes(base.clone(), with("not json {")), ["body changed"]);
    let mut form = base.clone();
    form.body = RequestBody::Form(vec![KeyValue::new("id", "1")]);
    assert_eq!(changes(base.clone(), form), ["!body JSON → form"]);
    let mut none = base.clone();
    none.body = RequestBody::None;
    assert_eq!(changes(base.clone(), none.clone()), ["!JSON body removed"]);
    assert_eq!(changes(none, base), ["JSON body added"]);
}

#[test]
fn form_fields_are_compared_by_name() {
    // R-CLI-7
    let mut a = request(HttpMethod::Post, "/f");
    a.body = RequestBody::Form(vec![KeyValue::new("a", "1"), KeyValue::new("b", "2")]);
    let mut b = a.clone();
    b.body = RequestBody::Form(vec![KeyValue::new("a", "9")]);
    assert_eq!(changes(a, b), ["!form field `b` removed"]);
}

#[test]
fn examples_are_compared_by_name_status_type_and_shape() {
    // R-CLI-7
    let example = |name: &str, status: u16, body: &str| {
        let mut e = Example::new(name);
        e.status = status;
        e.body = body.into();
        e.headers.push(KeyValue::new(
            "Content-Type",
            "application/json; charset=utf-8",
        ));
        e
    };
    let mut a = request(HttpMethod::Get, "/o");
    a.examples = vec![
        example("ok", 200, r#"{"id": 1, "total": 2}"#),
        example("missing", 404, ""),
        example("gone", 410, ""),
    ];
    let mut b = a.clone();
    let mut text = example("ok", 201, r#"{"id": 1, "refunded_at": "x"}"#);
    text.headers[0].value = "text/plain".into();
    b.examples = vec![text, example("missing", 404, ""), example("slow", 200, "")];
    assert_eq!(
        changes(a.clone(), b),
        [
            "example `gone` renamed to `slow`",
            "!example `ok`: status 200 → 201",
            "!example `ok`: Content-Type `application/json` → `text/plain`",
            "!example `ok`: `$.total` removed",
            "example `ok`: `$.refunded_at` added (string)",
            "!example `slow`: status 410 → 200",
        ]
    );
    let mut c = a.clone();
    c.examples.truncate(1);
    assert_eq!(
        changes(a, c),
        ["!example `missing` removed", "!example `gone` removed"]
    );
}

#[test]
fn the_path_leaves_out_server_base_url_and_query() {
    // R-CLI-7
    for (url, path) in [
        ("{{base_url}}/orders/:id?x=1", "/orders/:id"),
        ("https://api.x.io/v1/ü😀#top", "/v1/ü😀"),
        ("https://api.x.io", "/"),
        ("{{base_url}}", "/"),
        ("/orders", "/orders"),
        ("", "/"),
        ("{{broken", "{{broken"),
    ] {
        assert_eq!(api_path(url), path, "{url}");
    }
    let long = format!("/{}", "a".repeat(100_000));
    assert_eq!(api_path(&long), long);
}

#[test]
fn bare_variables_become_null_but_strings_stay() {
    // R-CLI-7
    assert_eq!(
        bare_variables_as_null(r#"{"a": {{x}}, "b": "{{y}} \" {{z}}", "ü😀": [{{n}}]}"#),
        r#"{"a": null, "b": "{{y}} \" {{z}}", "ü😀": [null]}"#
    );
    assert_eq!(bare_variables_as_null("{{unclosed"), "{{unclosed");
    assert_eq!(json_value("  "), None);
}

#[test]
fn the_summary_is_a_table_with_details_for_more_than_one_change() {
    // R-CLI-7, D77
    let mut a = request(HttpMethod::Get, "/orders");
    a.name = "List <orders> | all".into();
    let mut b = a.clone();
    b.params.push(KeyValue::new("page", "1"));
    b.headers.push(KeyValue::new("X-Key", "secret-value"));
    let gone = request(HttpMethod::Delete, "/orders/:id");
    let before = project(vec![("list.toml", a), ("delete.toml", gone)]);
    let after = project(vec![("list.toml", b)]);
    let text = markdown(&diff(Some(&before), Some(&after)), "shop", "main..HEAD");
    assert_eq!(
        text,
        "### API changes: shop (main..HEAD)\n\n\
         **1 removed, 1 changed** · ⚠ 1 may break clients\n\n\
         | | Request | Change |\n|---|---|---|\n\
         | ➖ ⚠ | `DELETE /orders/:id` Orders ü😀 | ⚠ removed |\n\
         | ✏ | `GET /orders` List &lt;orders&gt; \\| all | param `page` added (+1 more) |\n\
         \n<details><summary><code>GET /orders</code> List &lt;orders&gt; | all</summary>\n\n\
         - param `page` added\n\
         - header `X-Key` added\n\
         \n</details>\n"
    );
    assert!(!text.contains("secret"));
    assert_eq!(
        markdown(&ApiDiff::default(), "shop", "HEAD..working tree"),
        "### API changes: shop (HEAD..working tree)\n\nNo API changes.\n"
    );
}
