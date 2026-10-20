//! The mock server's listener (R-MOCK-2, D46): HTTP/1.1 on localhost, answering with
//! a [`MockApi`]. Native only; the app and `tortuga-cli` both run it on their tokio
//! runtime (P7).

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::SystemTime;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::watch;

use super::{Answer, Hit, MockApi, MockRequest};
use crate::model::KeyValue;
use crate::{Error, Result};

/// The mock's answers, replaced when the project changes on disk (R-MOCK-2).
pub type SharedApi = Arc<RwLock<MockApi>>;

/// A running mock server. It stops when this is dropped.
#[derive(Debug)]
pub struct RunningMock {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
}

impl RunningMock {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://127.0.0.1:<port>`, what the mock's environment points servers at.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for RunningMock {
    fn drop(&mut self) {
        // Nobody listening means the server already stopped.
        let _ = self.stop.send(true);
    }
}

/// Starts the mock on `127.0.0.1:port` (`0`: a free port) on the current tokio runtime.
/// `on_hit` is called for every request it answers.
pub async fn start(
    port: u16,
    api: SharedApi,
    on_hit: impl Fn(Hit) + Send + Sync + 'static,
) -> Result<RunningMock> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|source| Error::MockServer { port, source })?;
    let addr = listener
        .local_addr()
        .map_err(|source| Error::MockServer { port, source })?;
    let (stop, stopped) = watch::channel(false);
    let on_hit: Arc<dyn Fn(Hit) + Send + Sync> = Arc::new(on_hit);
    tokio::spawn(accept(listener, api, on_hit, stopped));
    Ok(RunningMock { addr, stop })
}

async fn accept(
    listener: TcpListener,
    api: SharedApi,
    on_hit: Arc<dyn Fn(Hit) + Send + Sync>,
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
        let api = Arc::clone(&api);
        let on_hit = Arc::clone(&on_hit);
        let mut stopped = stopped.clone();
        tokio::spawn(async move {
            let service =
                service_fn(move |request| handle(request, Arc::clone(&api), Arc::clone(&on_hit)));
            let connection = http1::Builder::new().serve_connection(TokioIo::new(stream), service);
            tokio::select! {
                // A broken or closed connection only ends itself.
                _ = connection => {}
                // Stopping the mock closes kept-alive connections too.
                _ = stopped.changed() => {}
            }
        });
    }
}

async fn handle(
    request: hyper::Request<Incoming>,
    api: SharedApi,
    on_hit: Arc<dyn Fn(Hit) + Send + Sync>,
) -> std::result::Result<hyper::Response<Full<Bytes>>, Infallible> {
    let method = request.method().as_str().to_owned();
    let target = request
        .uri()
        .path_and_query()
        .map_or("/", |p| p.as_str())
        .to_owned();
    let mut incoming = MockRequest::new(&method, &target);
    incoming.headers = request
        .headers()
        .iter()
        .map(|(name, value)| {
            KeyValue::new(name.as_str(), String::from_utf8_lossy(value.as_bytes()))
        })
        .collect();
    // A body cut off by the client is answered as if it were empty.
    incoming.body = request
        .into_body()
        .collect()
        .await
        .map(|body| body.to_bytes().to_vec())
        .unwrap_or_default();
    let answer = api
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .answer(&incoming);
    if let Some(delay) = answer.delay {
        tokio::time::sleep(delay).await;
    }
    let response = response(&answer);
    on_hit(Hit {
        at: SystemTime::now(),
        method,
        target,
        status: response.status().as_u16(),
        outcome: answer.outcome,
    });
    Ok(response)
}

/// The HTTP response for `answer`; a `500` saying why when the example can't be sent
/// (a status outside 100–999, a header name or value HTTP doesn't allow).
fn response(answer: &Answer) -> hyper::Response<Full<Bytes>> {
    let mut builder = hyper::Response::builder().status(answer.status);
    for header in &answer.headers {
        builder = builder.header(header.name.as_str(), header.value.as_str());
    }
    builder
        .body(Full::new(Bytes::from(answer.body.clone())))
        .unwrap_or_else(|e| {
            let mut broken = hyper::Response::new(Full::new(Bytes::from(format!(
                "The example can't be sent as HTTP: {e}"
            ))));
            *broken.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
            broken
        })
}
