//! Auth that a collection sets on three levels (collection, folder, request), as the
//! Postman (R-COL-16) and Bruno (R-COL-31) imports map it: Tortuga's own auth where it
//! has one, API keys as headers or query parameters, credentials as secret variables
//! (D35), and what Tortuga's folder inheritance can't express carried down to the
//! requests.

use crate::error::sentence;
use crate::format::{FolderDefaults, Manifest};
use crate::import::{Collected, variable_name};
use crate::model::{Auth, KeyValue, Request};

/// One level's auth as the other tool describes it.
pub(crate) enum AuthSpec {
    Inherit,
    NoAuth,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        in_query: bool,
    },
    OAuth2 {
        access_token: String,
    },
    /// Auth Tortuga doesn't have, by the other tool's name for it.
    Other(String),
}

/// Auth and an API key that one level (collection, folder or request) sets.
#[derive(Debug, Clone, Default)]
pub(crate) struct Credentials {
    /// `None` = inherit.
    pub auth: Option<Auth>,
    pub header: Option<KeyValue>,
    pub query: Option<KeyValue>,
}

impl Collected {
    /// The credentials of `spec`, set at `place` ("the collection", "folder “Users”").
    pub fn credentials(&mut self, spec: AuthSpec, place: &str) -> Credentials {
        let mut out = Credentials::default();
        match spec {
            AuthSpec::Inherit => {}
            AuthSpec::NoAuth => out.auth = Some(Auth::None),
            AuthSpec::Bearer { token } => {
                out.auth = Some(Auth::Bearer {
                    token: self.credential(&token, "token"),
                });
            }
            AuthSpec::Basic { username, password } => {
                out.auth = Some(Auth::Basic {
                    username: self.credential(&username, "username"),
                    password: self.credential(&password, "password"),
                });
            }
            AuthSpec::ApiKey {
                key,
                value,
                in_query,
            } => {
                let key = Some(key)
                    .filter(|k| !k.trim().is_empty())
                    .unwrap_or_else(|| "X-API-Key".into());
                let value = self.credential(&value, &variable_name(&key));
                let key_value = KeyValue::new(key.trim(), value);
                if in_query {
                    out.query = Some(key_value);
                } else {
                    out.header = Some(key_value);
                }
                out.auth = Some(Auth::None);
            }
            AuthSpec::OAuth2 { access_token } => {
                out.auth = Some(Auth::Bearer {
                    token: self.credential(&access_token, "access_token"),
                });
                self.warn(format!(
                    "{} uses OAuth 2, which became Bearer {{{{access_token}}}}: get a token and \
                     put it in access_token under Environments….",
                    sentence(place)
                ));
            }
            AuthSpec::Other(kind) => {
                out.auth = Some(Auth::None);
                self.warn(format!(
                    "{} uses {kind} auth, which Tortuga does not support, so it was left out.",
                    sentence(place)
                ));
            }
        }
        out
    }
}

/// What a request gets from the levels above it that Tortuga's inheritance doesn't
/// carry by itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct Inherited {
    /// API keys the collection or a folder sends as query parameters.
    query: Vec<KeyValue>,
    /// API key headers of the nearest level with auth, which own auth turns off.
    headers: Vec<KeyValue>,
    /// The project or a folder has auth that Tortuga passes down.
    auth: bool,
    /// A folder turned that auth off, which Tortuga's folders can't (R-COL-11).
    no_auth: bool,
}

impl Inherited {
    /// The collection's credentials as the project's auth and headers, and what its
    /// requests inherit.
    pub fn top(creds: Credentials, manifest: &mut Manifest) -> Self {
        let auth = creds.auth.unwrap_or_default();
        manifest.headers.extend(creds.header.clone());
        let top = Self {
            query: creds.query.into_iter().collect(),
            headers: creds.header.into_iter().collect(),
            auth: !auth.is_none(),
            no_auth: false,
        };
        manifest.auth = Some(auth).filter(|a| !a.is_none());
        top
    }

    /// A folder's credentials as its auth and headers, and what its items inherit.
    pub fn folder(&self, creds: Credentials, defaults: &mut FolderDefaults) -> Self {
        let Some(auth) = creds.auth else {
            return self.clone();
        };
        defaults
            .headers
            .extend(turned_off(&self.headers, creds.header.as_ref()));
        defaults.headers.extend(creds.header.clone());
        let below = Self {
            query: creds.query.into_iter().collect(),
            headers: creds.header.into_iter().collect(),
            auth: self.auth || !auth.is_none(),
            no_auth: auth.is_none(),
        };
        // "No auth" reaches the requests through `no_auth`.
        defaults.auth = Some(auth).filter(|a| !a.is_none());
        below
    }

    /// A request's own credentials, or what it gets from above when it inherits.
    pub fn apply(&self, own: Credentials, request: &mut Request) {
        let Some(auth) = own.auth else {
            if self.no_auth && self.auth {
                request.auth = Some(Auth::None);
            }
            request.params.extend(self.query.iter().cloned());
            return;
        };
        request.auth = Some(auth);
        request
            .headers
            .extend(turned_off(&self.headers, own.header.as_ref()));
        request.headers.extend(own.header);
        request.params.extend(own.query);
    }
}

/// Disabled copies of `headers`, which turn the inherited ones off (R-COL-11), except
/// one with the name of `own`, which replaces it anyway.
fn turned_off(headers: &[KeyValue], own: Option<&KeyValue>) -> Vec<KeyValue> {
    headers
        .iter()
        .filter(|h| own.is_none_or(|o| !o.is_named(&h.name)))
        .map(|h| KeyValue {
            enabled: false,
            ..h.clone()
        })
        .collect()
}
