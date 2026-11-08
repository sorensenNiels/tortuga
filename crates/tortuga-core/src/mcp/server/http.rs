//! The app's MCP server (R-MCP-5, D75): Streamable HTTP on `127.0.0.1`, at `/mcp`, for
//! agents that hold the bearer token made when it started. Native only.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower::Service as _;

use super::{OnAction, TortugaMcp};
use crate::mcp::SharedProjects;
use crate::{Client, Error, Result};

/// The path agents post to.
pub const PATH: &str = "/mcp";

type Response = hyper::Response<BoxBody<Bytes, Infallible>>;

/// A running MCP server. It stops when this is dropped.
pub struct RunningMcp {
    addr: SocketAddr,
    token: String,
    stop: watch::Sender<bool>,
}

impl std::fmt::Debug for RunningMcp {
    // Hand-written so the token never reaches a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningMcp")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl RunningMcp {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://127.0.0.1:<port>/mcp`, what agents connect to.
    pub fn url(&self) -> String {
        format!("http://{}{PATH}", self.addr)
    }

    /// The bearer token agents must send, new at each start (D75).
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for RunningMcp {
    fn drop(&mut self) {
        // Nobody listening means the server already stopped.
        let _ = self.stop.send(true);
    }
}

/// Starts the MCP server for `projects` on `127.0.0.1:port` (`0`: a free port) on the
/// current tokio runtime, with a new random token (R-MCP-5, D75).
pub async fn start(
    port: u16,
    projects: SharedProjects,
    client: Client,
    on_action: OnAction,
) -> Result<RunningMcp> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|source| Error::McpListen { port, source })?;
    let addr = listener
        .local_addr()
        .map_err(|source| Error::McpListen { port, source })?;
    let token = new_token();
    let handler = TortugaMcp::new(projects, client, on_action);
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        // Only loopback `Host` headers by default: no DNS rebinding.
        StreamableHttpServerConfig::default(),
    );
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(accept(
        listener,
        service,
        Arc::from(token.as_str()),
        stopped,
    ));
    Ok(RunningMcp { addr, token, stop })
}

/// 128 random bits as hex, from the thread's cryptographically secure generator.
fn new_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

async fn accept(
    listener: TcpListener,
    service: StreamableHttpService<TortugaMcp, LocalSessionManager>,
    token: Arc<str>,
    mut stopped: watch::Receiver<bool>,
) {
    loop {
        let stream = tokio::select! {
            _ = stopped.changed() => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                // A connection that failed to open only affects its client.
                Err(_) => continue,
            },
        };
        let service = service.clone();
        let token = Arc::clone(&token);
        let mut stopped = stopped.clone();
        tokio::spawn(async move {
            let handle =
                service_fn(move |request| handle(request, service.clone(), Arc::clone(&token)));
            let connection = http1::Builder::new().serve_connection(TokioIo::new(stream), handle);
            tokio::select! {
                // A broken or closed connection only ends itself.
                _ = connection => {}
                // Stopping the server closes kept-alive connections and streams too.
                _ = stopped.changed() => {}
            }
        });
    }
}

async fn handle(
    request: hyper::Request<Incoming>,
    mut service: StreamableHttpService<TortugaMcp, LocalSessionManager>,
    token: Arc<str>,
) -> std::result::Result<Response, Infallible> {
    if request.uri().path() != PATH {
        return Ok(plain(
            hyper::StatusCode::NOT_FOUND,
            "Tortuga's MCP server is at /mcp.",
        ));
    }
    let given = request
        .headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !same(given.trim().as_bytes(), token.as_bytes()) {
        return Ok(plain(
            hyper::StatusCode::UNAUTHORIZED,
            "Send the token from Tortuga's MCP server dialog as 'Authorization: Bearer <token>'.",
        ));
    }
    service.call(request).await
}

/// Compares in time that doesn't depend on where the bytes differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn plain(status: hyper::StatusCode, text: &'static str) -> Response {
    let mut response = hyper::Response::new(Full::new(Bytes::from_static(text.as_bytes())).boxed());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_differ() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn comparing_tokens() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"abcd"));
        assert!(!same(b"", b"abc"));
    }
}
