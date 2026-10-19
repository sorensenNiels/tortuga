//! Cookie jars (R-CK-1..5).
//!
//! Each collection has a [`CookieJar`]. Cookies from responses are stored in it
//! and sent with later matching requests, like a browser does (domain, path,
//! expiry and `Secure` rules come from the `cookie_store` crate, which reqwest
//! uses too). The jar is plugged into the HTTP client, so cookies set during
//! redirects are kept as well. Jars are local data (R-CK-3): the app saves them
//! with [`CookieJar::to_json`] outside the collection directory.
//!
//! In the browser (wasm32) the browser sends and stores cookies itself, so the
//! jar is never filled there.

mod imp {
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

    use cookie_store::{CookieDomain, CookieExpiration, CookieStore};
    #[cfg(not(target_arch = "wasm32"))]
    use reqwest::header::HeaderValue;

    use crate::{Error, Result};

    /// One cookie in a jar, for the cookie viewer (R-CK-5).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CookieInfo {
        /// The host it is sent to (with its subdomains when `host_only` is false).
        pub domain: String,
        /// Set without a `Domain` attribute: sent to exactly `domain`.
        pub host_only: bool,
        pub path: String,
        pub name: String,
        pub value: String,
        /// When it expires, in milliseconds since 1970; `None` for a session cookie.
        pub expires_ms: Option<u64>,
        pub secure: bool,
        pub http_only: bool,
    }

    /// A cookie jar. Cheap to clone; clones share the same cookies.
    #[derive(Clone, Default)]
    pub struct CookieJar {
        store: Arc<Mutex<CookieStore>>,
    }

    impl std::fmt::Debug for CookieJar {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CookieJar")
                .field("cookies", &self.len())
                .finish()
        }
    }

    impl CookieJar {
        /// Reads a jar saved with [`CookieJar::to_json`].
        pub fn from_json(text: &str) -> Result<Self> {
            let store = cookie_store::serde::json::load_all(text.as_bytes())
                .map_err(|e| Error::InvalidCookies(e.to_string()))?;
            Ok(Self {
                store: Arc::new(Mutex::new(store)),
            })
        }

        /// Everything in the jar, including session cookies, so a restart keeps
        /// you logged in to your dev server. Expired cookies are never sent.
        pub fn to_json(&self) -> Result<String> {
            let mut out = Vec::new();
            cookie_store::serde::json::save_incl_expired_and_nonpersistent(&self.lock(), &mut out)
                .map_err(|e| Error::SaveCookies(e.to_string()))?;
            String::from_utf8(out).map_err(|e| Error::SaveCookies(e.to_string()))
        }

        /// Number of cookies that have not expired.
        pub fn len(&self) -> usize {
            self.lock().iter_unexpired().count()
        }

        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        /// Removes every cookie (R-CK-3).
        pub fn clear(&self) {
            self.lock().clear();
        }

        /// The cookies that have not expired, by domain, path and name (R-CK-5).
        pub fn list(&self) -> Vec<CookieInfo> {
            let mut cookies: Vec<CookieInfo> = self
                .lock()
                .iter_unexpired()
                .map(|c| CookieInfo {
                    domain: String::from(&c.domain),
                    host_only: matches!(c.domain, CookieDomain::HostOnly(_)),
                    path: c.path.to_string(),
                    name: c.name().to_owned(),
                    value: c.value().to_owned(),
                    expires_ms: match &c.expires {
                        CookieExpiration::AtUtc(at) => {
                            Some(u64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(0))
                        }
                        CookieExpiration::SessionEnd => None,
                    },
                    secure: c.secure().unwrap_or(false),
                    http_only: c.http_only().unwrap_or(false),
                })
                .collect();
            cookies
                .sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
            cookies
        }

        /// Removes one cookie (R-CK-5). Returns whether it was in the jar.
        pub fn remove(&self, domain: &str, path: &str, name: &str) -> bool {
            self.lock().remove(domain, path, name).is_some()
        }

        /// `name=value` pairs that would be sent to `url`.
        pub fn cookies_for(&self, url: &url::Url) -> Vec<(String, String)> {
            self.lock()
                .get_request_values(url)
                .map(|(n, v)| (n.to_owned(), v.to_owned()))
                .collect()
        }

        /// Stores cookies from `Set-Cookie` header values received from `url`.
        pub fn store_response_cookies<'a>(
            &self,
            set_cookie: impl IntoIterator<Item = &'a str>,
            url: &url::Url,
        ) {
            let cookies = set_cookie
                .into_iter()
                .filter_map(|v| cookie_store::RawCookie::parse(v.to_owned()).ok());
            self.lock().store_response_cookies(cookies, url);
        }

        /// Identifies the jar (its shared storage), e.g. to cache a client per jar.
        #[cfg_attr(
            target_arch = "wasm32",
            expect(dead_code, reason = "no per-jar client in the browser")
        )]
        pub(crate) fn id(&self) -> usize {
            Arc::as_ptr(&self.store) as usize
        }

        fn lock(&self) -> MutexGuard<'_, CookieStore> {
            self.store.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    /// Lets reqwest read and fill the jar on every request, including redirect hops.
    #[cfg(not(target_arch = "wasm32"))]
    impl reqwest::cookie::CookieStore for CookieJar {
        fn set_cookies(
            &self,
            cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
            url: &url::Url,
        ) {
            let values: Vec<&str> = cookie_headers.filter_map(|v| v.to_str().ok()).collect();
            self.store_response_cookies(values, url);
        }

        fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
            let header = self
                .cookies_for(url)
                .into_iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            if header.is_empty() {
                None
            } else {
                HeaderValue::from_str(&header).ok()
            }
        }
    }
}

pub use imp::{CookieInfo, CookieJar};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).expect("valid url")
    }

    #[test]
    fn cookies_match_domain_and_path_like_a_browser() {
        let jar = CookieJar::default();
        jar.store_response_cookies(
            ["session=abc; Path=/api", "theme=dark", "gone=x; Max-Age=0"],
            // Without a Path, a cookie applies to the directory of the URL that set it (RFC 6265).
            &url("http://example.test/login"),
        );
        assert_eq!(jar.len(), 2, "expired cookies are not kept");
        let mut sent = jar.cookies_for(&url("http://example.test/api/users"));
        sent.sort();
        assert_eq!(
            sent,
            [
                ("session".into(), "abc".into()),
                ("theme".into(), "dark".into())
            ]
        );
        assert_eq!(
            jar.cookies_for(&url("http://example.test/other")),
            [("theme".to_owned(), "dark".to_owned())]
        );
        assert_eq!(
            jar.cookies_for(&url("http://elsewhere.test/api")),
            Vec::<(String, String)>::new(),
            "other hosts get nothing"
        );
    }

    /// R-CK-5: the jar lists its cookies with their attributes, and removes one.
    #[test]
    fn cookies_are_listed_and_removed_one_by_one() {
        let jar = CookieJar::default();
        jar.store_response_cookies(
            [
                "session=abc; Path=/api; HttpOnly; Secure",
                "theme=dark; Domain=example.test; Expires=Wed, 01 Jan 2120 00:00:00 GMT",
            ],
            &url("https://api.example.test/login"),
        );
        let list = jar.list();
        assert_eq!(list.len(), 2);
        let session = &list[0];
        assert_eq!(
            (
                session.domain.as_str(),
                session.path.as_str(),
                session.name.as_str()
            ),
            ("api.example.test", "/api", "session")
        );
        assert!(session.host_only && session.secure && session.http_only);
        assert_eq!(session.expires_ms, None);
        let theme = &list[1];
        assert_eq!(
            (theme.domain.as_str(), theme.path.as_str()),
            ("example.test", "/")
        );
        assert!(!theme.host_only && !theme.secure && !theme.http_only);
        assert_eq!(theme.expires_ms, Some(4_733_510_400_000));

        assert!(!jar.remove("example.test", "/", "session"), "wrong domain");
        assert!(jar.remove("api.example.test", "/api", "session"));
        assert_eq!(jar.list().len(), 1);
        assert_eq!(
            jar.cookies_for(&url("https://api.example.test/api/x")),
            [("theme".to_owned(), "dark".to_owned())]
        );
    }

    #[test]
    fn jar_round_trips_through_json_including_session_cookies() {
        let jar = CookieJar::default();
        jar.store_response_cookies(
            ["session=abc", "id=7; Max-Age=3600"],
            &url("http://a.test/"),
        );
        let back = CookieJar::from_json(&jar.to_json().expect("save")).expect("load");
        assert_eq!(back.len(), 2);
        back.clear();
        assert!(back.is_empty());
        assert_eq!(jar.len(), 2, "a loaded jar is independent");
    }

    #[test]
    fn clones_share_cookies() {
        let jar = CookieJar::default();
        let clone = jar.clone();
        clone.store_response_cookies(["a=1"], &url("http://a.test/"));
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.id(), clone.id());
    }
}
