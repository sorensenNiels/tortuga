//! Performance budgets (R-NF-1) that can be checked without a window.
//!
//! The budgets hold for unoptimised test builds too, so a failure here means a real regression.

use std::path::Path;
use std::time::{Duration, Instant};

use tortuga_core::storage::FsStorage;
use tortuga_core::{Auth, Collection, HttpMethod, Item, KeyValue, Request, RequestBody, format};

const FS: FsStorage = FsStorage;

const FOLDERS: usize = 40;
const REQUESTS_PER_FOLDER: usize = 25;

/// A request like the ones people keep: params, headers, auth and a JSON body.
fn realistic_request(folder: usize, i: usize) -> Request {
    let mut r = Request::new(
        HttpMethod::Post,
        format!("/folder-{folder}/items/:id/children?page=1&size=50&sort=name"),
    );
    r.name = format!("Request {folder}-{i}");
    r.server = Some("api".into());
    r.sync_path_variables();
    r.path_vars[0].value = "{{item_id}}".into();
    r.headers = vec![
        KeyValue::new("Accept", "application/json"),
        KeyValue::new("X-Request-Id", "{{request_id}}"),
        KeyValue::new("X-Tenant", "{{tenant}}"),
    ];
    r.auth = Some(Auth::Bearer {
        token: "{{token}}".into(),
    });
    r.body = RequestBody::Json(format!(
        "{{\n  \"name\": \"item {i}\",\n  \"tags\": [\"a\", \"b\", \"c\"],\n  \"owner\": {{ \"id\": {folder}, \"email\": \"{{{{email}}}}\" }}\n}}"
    ));
    r
}

fn project_with_1000_requests(dir: &Path) -> Collection {
    let col = Collection::create(&FS, dir, "Big API").expect("create");
    for f in 0..FOLDERS {
        let folder = col
            .new_folder(&FS, Path::new(""), &format!("Folder {f}"))
            .expect("folder");
        for i in 0..REQUESTS_PER_FOLDER {
            let seq = i64::try_from(i * 1000).expect("seq");
            let text = format::write_request(&realistic_request(f, i), Some(seq));
            std::fs::write(dir.join(&folder).join(format!("request-{i}.toml")), text)
                .expect("write request");
        }
    }
    col
}

fn count_requests(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Item::Request(_) => 1,
            Item::Folder(f) => count_requests(&f.items),
            Item::Broken(_) => 0,
        })
        .sum()
}

/// R-NF-1: a project with 1,000 requests opens in under 1 s.
#[test]
#[expect(clippy::print_stdout, reason = "the timing is useful in test output")]
fn a_project_with_1000_requests_opens_in_under_a_second() {
    let dir = tempfile::tempdir().expect("temp dir");
    let col = project_with_1000_requests(dir.path());

    let started = Instant::now();
    let opened = Collection::open(&FS, &col.root).expect("open");
    let took = started.elapsed();

    assert_eq!(count_requests(&opened.items), FOLDERS * REQUESTS_PER_FOLDER);
    println!("Opened 1,000 requests in {} ms", took.as_millis());
    assert!(
        took < Duration::from_secs(1),
        "opening 1,000 requests took {took:?}"
    );
}
