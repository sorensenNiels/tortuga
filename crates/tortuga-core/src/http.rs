//! Executes [`Request`]s over HTTP (R-HTTP-1..8).
//!
//! Cancelling (R-HTTP-7) is done by dropping the future returned by
//! [`Client::execute`]; the connection is closed and nothing else is needed.

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use web_time::Instant;

use crate::cookies::CookieJar;
use crate::model::{FormPart, HttpMethod, KeyValue, Request, RequestBody, Response};
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
    /// Send and store cookies (R-CK-2, R-SET-1). Applied through
    /// [`crate::vars::Context`], where workspaces and projects can override it.
    pub send_cookies: bool,
    /// PEM files whose certificates are trusted besides the system's (R-HTTP-16).
    /// Local to this machine (D22); read by the app, see [`Client::with_ca_certificates`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ca_certificates: Vec<std::path::PathBuf>,
    /// Hosts whose certificates are not verified, e.g. `localhost` or `*.dev.internal`
    /// (R-SET-3). Only ever set on this machine, never by a shared file (D39).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub insecure_hosts: Vec<String>,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            verify_tls: true,
            default_timeout_ms: 30_000,
            send_cookies: true,
            ca_certificates: Vec::new(),
            insecure_hosts: Vec::new(),
        }
    }
}

impl ClientSettings {
    pub const MAX_REDIRECTS: usize = 10;

    pub fn default_timeout(&self) -> Duration {
        Duration::from_millis(self.default_timeout_ms)
    }

    /// Requests to `host` are sent without verifying its certificate: verification is
    /// off, or the host is in [`Self::insecure_hosts`] (R-SET-3).
    pub fn skips_verification(&self, host: &str) -> bool {
        !self.verify_tls || self.is_insecure_host(host)
    }

    /// `host` is in [`Self::insecure_hosts`]: exactly, or under a `*.domain` entry.
    pub fn is_insecure_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.insecure_hosts.iter().any(|pattern| {
            let pattern = host_of_pattern(pattern);
            match pattern.strip_prefix("*.") {
                Some(domain) => !domain.is_empty() && host.ends_with(&format!(".{domain}")),
                None => !pattern.is_empty() && host == pattern,
            }
        })
    }
}

/// The host part of an entry in [`ClientSettings::insecure_hosts`], lower-case; a
/// scheme, port or path typed with it is left out.
fn host_of_pattern(pattern: &str) -> String {
    let p = pattern.trim();
    let p = p.split_once("://").map_or(p, |(_, rest)| rest);
    let p = p.split('/').next().unwrap_or_default();
    // `[::1]:8443` and `host:8443` lose the port; `::1` stays as it is.
    let p = match p.rsplit_once(':') {
        Some((host, port))
            if (host.ends_with(']') || !host.contains(':'))
                && !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit()) =>
        {
            host
        }
        _ => p,
    };
    p.trim_end_matches('.').to_ascii_lowercase()
}

/// Reads the files of multipart uploads when a request is sent (R-HTTP-15). The app
/// supplies it (the file system on desktop), so the engine never reads files itself.
#[derive(Clone)]
pub struct FileReader(pub std::sync::Arc<ReadFile>);

/// Reads a whole file.
pub type ReadFile = dyn Fn(&std::path::Path) -> std::io::Result<Vec<u8>> + Send + Sync;

impl std::fmt::Debug for FileReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileReader")
    }
}

/// Reusable HTTP client. Cheap to clone; keeps a shared connection pool.
/// Create a new one when [`ClientSettings`] change.
#[derive(Debug, Clone)]
pub struct Client {
    inner: reqwest::Client,
    settings: ClientSettings,
    /// Extra trusted CA certificates (R-HTTP-16), also for the clients with cookie jars.
    #[cfg(not(target_arch = "wasm32"))]
    roots: Vec<reqwest::Certificate>,
    /// Reads files to upload; `None`: uploading files is not possible here.
    files: Option<FileReader>,
    /// Clients other than `inner`, created on first use: one per cookie jar (R-CK-1),
    /// redirect policy (R-SET-1) and host exception (R-SET-3). Each keeps its jar
    /// alive, so the key (the jar's address) cannot be reused by another jar.
    #[cfg(not(target_arch = "wasm32"))]
    clients:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<ClientKey, reqwest::Client>>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(ClientSettings::default()).expect("default HTTP client settings are valid")
    }
}

impl Client {
    pub fn new(settings: ClientSettings) -> Result<Self> {
        Self::with_ca_certificates(settings, &[]).map(|(client, _)| client)
    }

    /// A client that also trusts the certificates in `files`, given as (path, PEM
    /// contents) (R-HTTP-16). A file without certificates is returned as a problem; the
    /// others still apply.
    pub fn with_ca_certificates(
        settings: ClientSettings,
        files: &[(std::path::PathBuf, Vec<u8>)],
    ) -> Result<(Self, Vec<Error>)> {
        #[cfg(not(target_arch = "wasm32"))]
        let (roots, problems) = ca_roots(files);
        #[cfg(target_arch = "wasm32")]
        let problems = files
            .iter()
            .map(|(path, _)| Error::CaCertificate {
                path: path.clone(),
                reason: "not supported in the browser".into(),
            })
            .collect();
        #[cfg(not(target_arch = "wasm32"))]
        let inner = builder(&settings, ClientKey::plain(&settings), &roots)
            .build()
            .map_err(|e| Error::ClientSetup(e.to_string()))?;
        #[cfg(target_arch = "wasm32")]
        let inner = builder(&settings)
            .build()
            .map_err(|e| Error::ClientSetup(e.to_string()))?;
        let client = Self {
            inner,
            settings,
            #[cfg(not(target_arch = "wasm32"))]
            roots,
            files: None,
            #[cfg(not(target_arch = "wasm32"))]
            clients: std::sync::Arc::default(),
        };
        Ok((client, problems))
    }

    /// Lets the client upload files in multipart bodies (R-HTTP-15).
    #[must_use]
    pub fn with_file_reader(mut self, files: FileReader) -> Self {
        self.files = Some(files);
        self
    }

    /// The client that sends `request` to `host`: with `jar` unless the request has
    /// cookies switched off (R-CK-2), following redirects as the request says, else as
    /// the app's settings say (R-SET-1), and without verifying the certificate of a
    /// host in the settings' list (R-SET-3). Without a jar, no cookies are sent or stored.
    #[cfg(not(target_arch = "wasm32"))]
    fn client_for(
        &self,
        request: &Request,
        jar: Option<&CookieJar>,
        host: &str,
    ) -> Result<reqwest::Client> {
        let jar = jar.filter(|_| request.cookies);
        let key = ClientKey {
            jar: jar.map(CookieJar::id),
            follow_redirects: request
                .follow_redirects
                .unwrap_or(self.settings.follow_redirects),
            insecure_host: self.settings.verify_tls && self.settings.is_insecure_host(host),
        };
        if key == ClientKey::plain(&self.settings) {
            return Ok(self.inner.clone());
        }
        let clients = || {
            self.clients
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let cached = clients().get(&key).cloned();
        if let Some(client) = cached {
            return Ok(client);
        }
        // Built without the lock: loading certificates takes a while, and other sends
        // shouldn't wait for it. If two sends build the same client, the first one is kept.
        let mut b = builder(&self.settings, key, &self.roots);
        if let Some(jar) = jar {
            b = b.cookie_provider(std::sync::Arc::new(jar.clone()));
        }
        let client = b.build().map_err(|e| Error::ClientSetup(e.to_string()))?;
        Ok(clients().entry(key).or_insert(client).clone())
    }

    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::unnecessary_wraps, reason = "same signature as native")]
    fn client_for(
        &self,
        _request: &Request,
        _jar: Option<&CookieJar>,
        _host: &str,
    ) -> Result<reqwest::Client> {
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
        let full_url = query::join_url(&request.url, &request.params);
        let url = url::Url::parse(full_url.trim()).map_err(|source| Error::InvalidUrl {
            url: full_url.clone(),
            source,
        })?;
        let client = self.client_for(request, jar, url.host_str().unwrap_or_default())?;

        let timeout = request
            .timeout()
            .unwrap_or_else(|| self.settings.default_timeout());
        let mut builder = client
            .request(to_reqwest_method(request.method), url)
            .timeout(timeout);

        // A multipart body sets its own Content-Type, with the boundary.
        let multipart = matches!(request.body, RequestBody::Multipart(_));
        for header in request.headers.iter().filter(|h| h.is_active()) {
            if multipart
                && header
                    .name
                    .trim()
                    .eq_ignore_ascii_case(CONTENT_TYPE.as_str())
            {
                continue;
            }
            builder = builder.header(header_name(header)?, header_value(header)?);
        }

        if !multipart
            && !request.has_header(CONTENT_TYPE.as_str())
            && let Some(content_type) = request.body.default_content_type()
        {
            builder = builder.header(CONTENT_TYPE, content_type);
        }

        builder = match &request.body {
            RequestBody::None => builder,
            RequestBody::Text(text) | RequestBody::Json(text) | RequestBody::Xml(text) => {
                builder.body(text.clone())
            }
            RequestBody::Form(fields) => builder.body(query::encode_form(fields)),
            RequestBody::Multipart(parts) => builder.multipart(self.multipart_form(parts)?),
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
        // Takes over the received buffer instead of copying it (bodies can be 50 MB, R-NF-2).
        let body = Vec::from(
            response
                .bytes()
                .await
                .map_err(|e| map_send_error(&e, timeout))?,
        );
        let elapsed = started.elapsed();

        Ok(Response {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or_default().to_owned(),
            headers,
            body,
            elapsed,
        })
    }
}

impl Client {
    /// The form for a multipart body, with the files read now (R-HTTP-15).
    fn multipart_form(&self, parts: &[FormPart]) -> Result<reqwest::multipart::Form> {
        let mut form = reqwest::multipart::Form::new();
        for p in parts.iter().filter(|p| p.is_active()) {
            let name = p.name.trim().to_owned();
            let mut part = if p.file {
                let path = std::path::Path::new(&p.value);
                let upload_error = |reason: String| Error::UploadFile {
                    path: path.to_owned(),
                    reason,
                };
                let files = self
                    .files
                    .as_ref()
                    .ok_or_else(|| upload_error("files can't be read here".into()))?;
                let data = (files.0)(path).map_err(|e| upload_error(e.to_string()))?;
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                reqwest::multipart::Part::bytes(data).file_name(file_name)
            } else {
                reqwest::multipart::Part::text(p.value.clone())
            };
            if let Some(ct) = p
                .content_type
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                part = part.mime_str(ct).map_err(|_| {
                    Error::InvalidOperation(format!("“{ct}” is not a content type (part “{name}”)"))
                })?;
            }
            form = form.part(name, part);
        }
        Ok(form)
    }
}

/// What sets one HTTP client apart from another (see [`Client::client_for`]).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClientKey {
    /// The cookie jar's address.
    jar: Option<usize>,
    follow_redirects: bool,
    /// The host is in the settings' list of hosts not to verify (R-SET-3).
    insecure_host: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl ClientKey {
    /// The client most requests use: no jar, the app's settings.
    fn plain(settings: &ClientSettings) -> Self {
        Self {
            jar: None,
            follow_redirects: settings.follow_redirects,
            insecure_host: false,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn builder(
    settings: &ClientSettings,
    key: ClientKey,
    roots: &[reqwest::Certificate],
) -> reqwest::ClientBuilder {
    let redirect = if !key.follow_redirects {
        reqwest::redirect::Policy::none()
    } else if key.insecure_host {
        // The exception is for the listed hosts only: a redirect elsewhere is not followed,
        // since this client doesn't verify certificates (R-SET-3).
        let settings = settings.clone();
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= ClientSettings::MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if settings.is_insecure_host(attempt.url().host_str().unwrap_or_default()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    } else {
        reqwest::redirect::Policy::limited(ClientSettings::MAX_REDIRECTS)
    };
    reqwest::Client::builder()
        .redirect(redirect)
        .tls_danger_accept_invalid_certs(!settings.verify_tls || key.insecure_host)
        // Besides the system's certificates, not instead of them.
        .tls_certs_merge(roots.iter().cloned())
}

#[cfg(target_arch = "wasm32")]
fn builder(_settings: &ClientSettings) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
}

/// The certificates in each PEM file; a file without any is a problem (R-HTTP-16).
#[cfg(not(target_arch = "wasm32"))]
fn ca_roots(files: &[(std::path::PathBuf, Vec<u8>)]) -> (Vec<reqwest::Certificate>, Vec<Error>) {
    let mut roots = Vec::new();
    let mut problems = Vec::new();
    for (path, pem) in files {
        match reqwest::Certificate::from_pem_bundle(pem) {
            Ok(certs) if !certs.is_empty() => roots.extend(certs),
            Ok(_) => problems.push(Error::CaCertificate {
                path: path.clone(),
                reason: "no PEM certificate (-----BEGIN CERTIFICATE-----) in it".into(),
            }),
            Err(e) => problems.push(Error::CaCertificate {
                path: path.clone(),
                reason: e.to_string(),
            }),
        }
    }
    (roots, problems)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// R-SET-3: exact hosts and `*.domain` entries, whatever else was typed with them.
    #[test]
    fn insecure_host_patterns() {
        let settings = ClientSettings {
            insecure_hosts: vec![
                "localhost".into(),
                "*.dev.internal".into(),
                "https://Staging.Test:8443/api".into(),
                "[::1]:8443".into(),
                "  ".into(),
                "*.".into(),
            ],
            ..ClientSettings::default()
        };
        for host in [
            "localhost",
            "LOCALHOST",
            "a.dev.internal",
            "x.y.dev.internal",
            "staging.test",
            "[::1]",
        ] {
            assert!(settings.is_insecure_host(host), "{host}");
        }
        for host in [
            "dev.internal",
            "localhost.evil.test",
            "staging.test.evil",
            "127.0.0.1",
            "",
        ] {
            assert!(!settings.is_insecure_host(host), "{host}");
        }
        assert!(settings.skips_verification("localhost"));
        assert!(!settings.skips_verification("example.com"));
        let off = ClientSettings {
            verify_tls: false,
            ..ClientSettings::default()
        };
        assert!(
            off.skips_verification("example.com"),
            "verification off for all"
        );
    }
}
