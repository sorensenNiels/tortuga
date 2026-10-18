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
    /// PEM files whose certificates are trusted besides the system's (R-HTTP-16).
    /// Local to this machine (D22); read by the app, see [`Client::with_ca_certificates`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ca_certificates: Vec<std::path::PathBuf>,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            verify_tls: true,
            default_timeout_ms: 30_000,
            ca_certificates: Vec::new(),
        }
    }
}

impl ClientSettings {
    pub const MAX_REDIRECTS: usize = 10;

    pub fn default_timeout(&self) -> Duration {
        Duration::from_millis(self.default_timeout_ms)
    }
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
        let inner = builder(
            &settings,
            #[cfg(not(target_arch = "wasm32"))]
            &roots,
        )
        .build()
        .map_err(|e| Error::ClientSetup(e.to_string()))?;
        let client = Self {
            inner,
            settings,
            #[cfg(not(target_arch = "wasm32"))]
            roots,
            files: None,
            #[cfg(not(target_arch = "wasm32"))]
            jar_clients: std::sync::Arc::default(),
        };
        Ok((client, problems))
    }

    /// Lets the client upload files in multipart bodies (R-HTTP-15).
    #[must_use]
    pub fn with_file_reader(mut self, files: FileReader) -> Self {
        self.files = Some(files);
        self
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
        let client = builder(&self.settings, &self.roots)
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
            RequestBody::Text(text) | RequestBody::Json(text) => builder.body(text.clone()),
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

#[cfg(not(target_arch = "wasm32"))]
fn builder(settings: &ClientSettings, roots: &[reqwest::Certificate]) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(if settings.follow_redirects {
            reqwest::redirect::Policy::limited(ClientSettings::MAX_REDIRECTS)
        } else {
            reqwest::redirect::Policy::none()
        })
        .tls_danger_accept_invalid_certs(!settings.verify_tls)
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
