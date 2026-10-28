//! The new-version check (R-SET-15), against a local mock of GitHub's API (never the internet).

use tortuga_core::Client;
use tortuga_core::update::{Download, Found, Release, check};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PATH: &str = "/repos/sorensenNiels/tortuga/releases/latest";

async fn github_answering(response: ResponseTemplate) -> (MockServer, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .and(header("Accept", "application/vnd.github+json"))
        .respond_with(response)
        .mount(&server)
        .await;
    let url = format!("{}{PATH}", server.uri());
    (server, url)
}

fn release(tag: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "tag_name": tag,
        "html_url": format!("https://github.com/sorensenNiels/tortuga/releases/tag/{tag}"),
        "name": "Tortuga ü😀",
        "draft": false,
        "assets": [{
            "name": "tortuga_0.7.0_amd64.deb",
            "browser_download_url": format!("https://github.com/sorensenNiels/tortuga/releases/download/{tag}/tortuga_0.7.0_amd64.deb"),
            "size": 1,
        }],
    }))
}

/// R-SET-15: a newer published release is found, with its page.
#[tokio::test]
async fn a_newer_release_is_found() {
    let (_server, url) = github_answering(release("v0.7.0")).await;
    let found = check(&Client::default(), &url, "0.6.0")
        .await
        .expect("checked");
    assert_eq!(
        found,
        Found::Newer(Release {
            version: "0.7.0".into(),
            page: "https://github.com/sorensenNiels/tortuga/releases/tag/v0.7.0".into(),
            // R-SET-18
            downloads: vec![Download {
                name: "tortuga_0.7.0_amd64.deb".into(),
                url: "https://github.com/sorensenNiels/tortuga/releases/download/v0.7.0/tortuga_0.7.0_amd64.deb".into(),
            }],
        })
    );
}

/// R-SET-15: the same or an older release is not offered.
#[tokio::test]
async fn the_same_or_an_older_release_is_up_to_date() {
    let (_server, url) = github_answering(release("v0.6.0")).await;
    let client = Client::default();
    assert_eq!(
        check(&client, &url, "0.6.0").await.expect("checked"),
        Found::UpToDate
    );
    assert_eq!(
        check(&client, &url, "0.7.0").await.expect("checked"),
        Found::UpToDate
    );
}

/// R-SET-15: GitHub answers 404 while no release is published.
#[tokio::test]
async fn no_published_release_is_up_to_date() {
    let (_server, url) = github_answering(ResponseTemplate::new(404)).await;
    assert_eq!(
        check(&Client::default(), &url, "0.6.0")
            .await
            .expect("checked"),
        Found::UpToDate
    );
}

/// R-SET-15: an error status or an unexpected body is reported, not taken as up to date.
#[tokio::test]
async fn failures_are_reported() {
    let (_server, url) = github_answering(ResponseTemplate::new(403)).await;
    let err = check(&Client::default(), &url, "0.6.0")
        .await
        .expect_err("rate limited");
    assert_eq!(
        err.to_string(),
        "could not look for a new version: GitHub answered 403 Forbidden"
    );

    let (_server, url) =
        github_answering(ResponseTemplate::new(200).set_body_string("<html>")).await;
    let err = check(&Client::default(), &url, "0.6.0")
        .await
        .expect_err("not JSON");
    assert!(
        err.to_string()
            .starts_with("could not look for a new version: unexpected answer from GitHub"),
        "{err}"
    );
}
