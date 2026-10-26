//! Measures where a send's time goes (R-HTTP-25): a name resolver and a connector layer
//! that time themselves, installed on every native client, and a task-local that
//! tells them which send they work for.
//!
//! The pool opens connections inside the send that needs one, so the task-local
//! finds the right send. When two sends race for one new connection, the one that
//! gets it may be the other; it then shows as reused. Good enough for a breakdown.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use web_time::Instant;

tokio::task_local! {
    static SEND: Arc<Mutex<Measured>>;
}

/// What a send's resolver and connector measured, summed over redirects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Measured {
    /// `None`: no name was looked up (an IP address, or a reused connection).
    pub dns: Option<Duration>,
    /// `None`: no connection was opened. Includes `dns`.
    pub connect: Option<Duration>,
}

impl Measured {
    /// The connection's setup: a new connection when one was opened, its connecting
    /// without the lookup; else a reused one.
    pub(super) fn setup(self) -> super::Setup {
        match self.connect {
            Some(total) => super::Setup {
                dns: self.dns,
                connect: Some(total.saturating_sub(self.dns.unwrap_or_default())),
                connection: crate::model::Connection::New,
            },
            None => super::Setup {
                dns: None,
                connect: None,
                connection: crate::model::Connection::Reused,
            },
        }
    }
}

/// Runs `send` and returns what was measured while it ran.
pub(super) async fn measure<F: Future>(send: F) -> (F::Output, Measured) {
    let measured = Arc::new(Mutex::new(Measured::default()));
    let output = SEND.scope(Arc::clone(&measured), send).await;
    let measured = *measured.lock().unwrap_or_else(PoisonError::into_inner);
    (output, measured)
}

/// The current send's record; `None` outside [`measure`].
fn current() -> Option<Arc<Mutex<Measured>>> {
    SEND.try_with(Arc::clone).ok()
}

fn add(sum: &mut Option<Duration>, d: Duration) {
    *sum = Some(sum.unwrap_or_default() + d);
}

fn record(to: Option<&Arc<Mutex<Measured>>>, f: impl FnOnce(&mut Measured)) {
    if let Some(to) = to {
        f(&mut to.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// Looks names up as reqwest's default resolver does (the system's `getaddrinfo`),
/// timing each lookup.
#[derive(Debug)]
pub(super) struct TimedResolver;

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let to = current();
        Box::pin(async move {
            let started = Instant::now();
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            record(to.as_ref(), |m| add(&mut m.dns, started.elapsed()));
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Times each connection the client opens, TLS and proxy tunnel included.
#[derive(Debug, Clone, Copy)]
pub(super) struct TimedConnect;

impl<S> tower::Layer<S> for TimedConnect {
    type Service = Timed<S>;

    fn layer(&self, inner: S) -> Timed<S> {
        Timed(inner)
    }
}

#[derive(Debug, Clone)]
pub(super) struct Timed<S>(S);

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

impl<S, R> tower::Service<R> for Timed<S>
where
    S: tower::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, target: R) -> Self::Future {
        let to = current();
        let connecting = self.0.call(target);
        Box::pin(async move {
            let started = Instant::now();
            let connection = connecting.await;
            record(to.as_ref(), |m| add(&mut m.connect, started.elapsed()));
            connection
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_what_runs_inside_measure_is_recorded() {
        record(current().as_ref(), |m| {
            add(&mut m.dns, Duration::from_secs(9));
        });
        let ((), measured) = measure(async {
            record(current().as_ref(), |m| {
                add(&mut m.dns, Duration::from_millis(2));
            });
            record(current().as_ref(), |m| {
                add(&mut m.dns, Duration::from_millis(3));
            });
        })
        .await;
        assert_eq!(measured.dns, Some(Duration::from_millis(5)));
        assert_eq!(measured.connect, None);
    }
}
