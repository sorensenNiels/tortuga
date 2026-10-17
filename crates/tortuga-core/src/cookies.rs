//! Cookie jars (R-CK-1..4).
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

    use cookie_store::CookieStore;
    #[cfg(not(target_arch = "wasm32"))]
    use reqwest::header::HeaderValue;

    use crate::{Error, Result};

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
        pub fn to_json(&self) -> String {
            let mut out = Vec::new();
            // Writing to a Vec cannot fail.
            let _ = cookie_store::serde::json::save_incl_expired_and_nonpersistent(
                &self.lock(),
                &mut out,
            );
            String::from_utf8(out).unwrap_or_default()
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

pub use imp::CookieJar;

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

    #[test]
    fn jar_round_trips_through_json_including_session_cookies() {
        let jar = CookieJar::default();
        jar.store_response_cookies(
            ["session=abc", "id=7; Max-Age=3600"],
            &url("http://a.test/"),
        );
        let back = CookieJar::from_json(&jar.to_json()).expect("load");
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
