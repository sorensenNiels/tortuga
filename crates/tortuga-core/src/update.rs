//! Whether a newer version of Tortuga has been released (R-SET-15, D63): asks GitHub
//! for the latest published release and compares its version with this one.

use serde::Deserialize;

use crate::model::{HttpMethod, KeyValue, Request};
use crate::{Client, Error, Result};

/// Where GitHub lists the latest published release. Drafts and pre-releases are not
/// returned, so a release is only seen once it is published.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/sorensenNiels/tortuga/releases/latest";

/// How long a check may take; it runs in the background, but should not hang around.
const TIMEOUT_MS: u64 = 10_000;

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version, without the tag's `v` ("0.7.0").
    pub version: String,
    /// Its page on GitHub, with the release notes and installers.
    pub page: String,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// No release is newer than this version (or none is published yet).
    UpToDate,
    Newer(Release),
}

/// What GitHub says about a release; only what the check needs.
#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// The request for the latest release at `url`. It carries nothing about the user:
/// GitHub requires a `User-Agent`, which names the app and its version.
fn latest_release_request(url: &str) -> Request {
    let mut request = Request::new(HttpMethod::Get, url);
    request.headers = vec![
        KeyValue::new("Accept", "application/vnd.github+json"),
        KeyValue::new(
            "User-Agent",
            format!("Tortuga/{}", env!("CARGO_PKG_VERSION")),
        ),
    ];
    request.timeout_ms = Some(TIMEOUT_MS);
    request
}

/// Asks `url` (normally [`LATEST_RELEASE_URL`]) through `client`, so the app's proxy
/// and certificate settings apply, whether a release newer than `current` exists.
pub async fn check(client: &Client, url: &str, current: &str) -> Result<Found> {
    let response = client.execute(&latest_release_request(url)).await?;
    match response.status {
        // GitHub answers 404 while no release is published.
        404 => Ok(Found::UpToDate),
        200..=299 => {
            let release: GitHubRelease = serde_json::from_slice(&response.body)
                .map_err(|e| Error::UpdateCheck(format!("unexpected answer from GitHub: {e}")))?;
            let version = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name)
                .to_owned();
            Ok(if is_newer(&version, current) {
                Found::Newer(Release {
                    version,
                    page: release.html_url,
                })
            } else {
                Found::UpToDate
            })
        }
        status => Err(Error::UpdateCheck(format!(
            "GitHub answered {status} {}",
            response.status_text
        ))),
    }
}

/// Whether version `candidate` comes after `current` ("1.2.3", optionally with a
/// pre-release part as in "1.2.3-beta.1", which comes before "1.2.3"). A version that
/// can't be read is never newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// `major.minor.patch`, and whether it is a final release (sorts after its pre-releases).
fn parse(version: &str) -> Option<([u64; 3], bool)> {
    let (numbers, pre) = match version.trim().split_once('-') {
        Some((numbers, pre)) => (numbers, Some(pre)),
        None => (version.trim(), None),
    };
    // Build metadata ("+abc") doesn't change the order.
    let numbers = numbers.split('+').next().unwrap_or_default();
    let mut parts = numbers.split('.').map(|p| p.parse::<u64>().ok());
    let triple = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some((triple, pre.is_none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-SET-15: versions compare by number, not as text, and a pre-release comes
    /// before its release.
    #[test]
    fn newer_versions_compare_by_number() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.6.1", "0.6.0"));
        assert!(!is_newer("0.6.0", "0.6.0"));
        assert!(!is_newer("0.5.9", "0.6.0"));
        assert!(is_newer("1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer("1.0.0-beta.1", "1.0.0"));
        assert!(is_newer("1.0.0-beta.1", "0.9.0"));
        assert!(!is_newer("1.0.0+build.5", "1.0.0"));
    }

    /// R-SET-15: a tag that isn't a version is never offered as an update.
    #[test]
    fn unreadable_versions_are_never_newer() {
        for odd in ["", "latest", "1.0", "1.0.0.0", "ü😀", "1.x.0", "-1.0.0"] {
            assert!(!is_newer(odd, "0.6.0"), "{odd:?}");
        }
        assert!(!is_newer("9.9.9", "not a version"));
    }

    /// R-SET-15: the request names the app and nothing else about the user.
    #[test]
    fn the_request_only_names_the_app() {
        let request = latest_release_request(LATEST_RELEASE_URL);
        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(
            request.header("Accept"),
            Some("application/vnd.github+json")
        );
        assert_eq!(
            request.header("User-Agent"),
            Some(format!("Tortuga/{}", env!("CARGO_PKG_VERSION")).as_str())
        );
        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.params, Vec::<KeyValue>::new());
    }
}
