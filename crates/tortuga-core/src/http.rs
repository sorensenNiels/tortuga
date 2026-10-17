//! Executes [`Request`]s over HTTP (R-HTTP-1..8).
//!
//! Cancelling (R-HTTP-7) is done by dropping the future returned by
//! [`Client::execute`]; the connection is closed and nothing else is needed.

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use web_time::Instant;

use crate::cookies::CookieJar;
use crate::model::{HttpMethod, KeyValue, Request, RequestBody, Response};
use crate::{Error, Result, query};

/// Global HTTP settings (R-HTTP-7, R-HTTP-8). Proxies come from the system settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientSettings {
    pub follow_redirects: bool,
    /// When false, invalid TLS certificates are accepted. The UI must warn about this.
    pub verify_tls: bool,
    /// Used when a request has no timeout of its own.
    pub default_timeout_ms: u64,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            verify_tls: true,
            default_timeout_ms: 30_000,
        }
    }
}

impl ClientSettings {
    pub const MAX_REDIRECTS: usize = 10;

    pub fn default_timeout(&self) -> Duration {
        Duration::from_millis(self.default_timeout_ms)
    }
}

/// Reusable HTTP client. Cheap to clone; keeps a shared connection pool.
/// Create a new one when [`ClientSettings`] change.
#[derive(Debug, Clone)]
pub struct Client {
    inner: reqwest::Client,
    settings: ClientSettings,
    /// One client per cookie jar, created on first use (R-CK-1). Each keeps its
    /// jar alive, so the key (the jar's address) cannot be reused by another jar.
    #[cfg(not(target_arch = "wasm32"))]
    jar_clients:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<usize, reqwest::Client>>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(ClientSettings::default()).expect("default HTTP client settings are valid")
    }
}

impl Client {
    pub fn new(settings: ClientSettings) -> Result<Self> {
        let inner = builder(&settings)
            .build()
            .map_err(|e| Error::ClientSetup(e.to_string()))?;
        Ok(Self {
            inner,
            settings,
            #[cfg(not(target_arch = "wasm32"))]
            jar_clients: std::sync::Arc::default(),
        })
    }

    /// The client that sends `request`: with `jar` unless the request has cookies
    /// switched off (R-CK-2). Without a jar, no cookies are sent or stored.
    #[cfg(not(target_arch = "wasm32"))]
    fn client_for(&self, request: &Request, jar: Option<&CookieJar>) -> Result<reqwest::Client> {
        let Some(jar) = jar.filter(|_| request.cookies) else {
            return Ok(self.inner.clone());
        };
        let mut clients = self
            .jar_clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(client) = clients.get(&jar.id()) {
            return Ok(client.clone());
        }
        let client = builder(&self.settings)
            .cookie_provider(std::sync::Arc::new(jar.clone()))
            .build()
            .map_err(|e| Error::ClientSetup(e.to_string()))?;
        clients.insert(jar.id(), client.clone());
        Ok(client)
    }

    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::unnecessary_wraps, reason = "same signature as native")]
    fn client_for(&self, _request: &Request, _jar: Option<&CookieJar>) -> Result<reqwest::Client> {
        Ok(self.inner.clone())
    }

    pub fn settings(&self) -> &ClientSettings {
        &self.settings
    }

    /// Sends `request` without a cookie jar.
    pub async fn execute(&self, request: &Request) -> Result<Response> {
        self.execute_with_cookies(request, None).await
    }

    /// Sends `request`, using and filling `jar` unless the request has cookies
    /// switched off (R-CK-1, R-CK-2). A `Cookie` header set on the request is
    /// always sent and then replaces the jar's cookies (R-CK-4).
    pub async fn execute_with_cookies(
        &self,
        request: &Request,
        jar: Option<&CookieJar>,
    ) -> Result<Response> {
        let client = self.client_for(request, jar)?;
        let full_url = query::join_url(&request.url, &request.params);
        let url = url::Url::parse(full_url.trim()).map_err(|source| Error::InvalidUrl {
            url: full_url.clone(),
            source,
        })?;

        let timeout = request
            .timeout()
            .unwrap_or_else(|| self.settings.default_timeout());
        let mut builder = client
            .request(to_reqwest_method(request.method), url)
            .timeout(timeout);

        for header in request.headers.iter().filter(|h| h.is_active()) {
            builder = builder.header(header_name(header)?, header_value(header)?);
        }

        if !request.has_header(CONTENT_TYPE.as_str())
            && let Some(content_type) = request.body.default_content_type()
        {
            builder = builder.header(CONTENT_TYPE, content_type);
        }

        builder = match &request.body {
            RequestBody::None => builder,
            RequestBody::Text(text) | RequestBody::Json(text) => builder.body(text.clone()),
            RequestBody::Form(fields) => builder.body(query::encode_form(fields)),
        };

        let started = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|e| map_send_error(&e, timeout))?;
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                KeyValue::new(name.as_str(), String::from_utf8_lossy(value.as_bytes()))
            })
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| map_send_error(&e, timeout))?
            .to_vec();
        let elapsed = started.elapsed();

        tracing::debug!(%status, ?elapsed, "request completed");

        Ok(Response {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or_default().to_owned(),
            headers,
            body,
            elapsed,
        })
    }
}

fn builder(settings: &ClientSettings) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder
        .redirect(if settings.follow_redirects {
            reqwest::redirect::Policy::limited(ClientSettings::MAX_REDIRECTS)
        } else {
            reqwest::redirect::Policy::none()
        })
        .tls_danger_accept_invalid_certs(!settings.verify_tls);
    #[cfg(target_arch = "wasm32")]
    let _ = settings;
    builder
}

fn map_send_error(err: &reqwest::Error, timeout: Duration) -> Error {
    if err.is_timeout() {
        Error::Timeout(timeout)
    } else {
        Error::transport(err)
    }
}

fn header_name(h: &KeyValue) -> Result<HeaderName> {
    HeaderName::from_bytes(h.name.trim().as_bytes()).map_err(|_| Error::InvalidHeader {
        name: h.name.clone(),
        reason: "name contains characters not allowed in a header name",
    })
}

fn header_value(h: &KeyValue) -> Result<HeaderValue> {
    HeaderValue::from_str(&h.value).map_err(|_| Error::InvalidHeader {
        name: h.name.clone(),
        reason: "value contains characters not allowed in a header (e.g. a line break)",
    })
}

fn to_reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
        HttpMethod::Head => reqwest::Method::HEAD,
        HttpMethod::Options => reqwest::Method::OPTIONS,
    }
}
