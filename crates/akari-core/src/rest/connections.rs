use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug, Default)]
pub(crate) struct Connections {
    opened: AtomicU64,
}

impl Connections {
    pub(crate) fn opened(&self) -> u64 {
        self.opened.load(Ordering::Acquire)
    }
}

// Wraps reqwest's whole connector, so a connect's time covers DNS, TCP and TLS.
#[derive(Clone)]
pub(crate) struct TimedConnectLayer(pub(crate) Arc<Connections>);

impl<S> Layer<S> for TimedConnectLayer {
    type Service = TimedConnect<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimedConnect {
            inner,
            connections: self.0.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TimedConnect<S> {
    inner: S,
    connections: Arc<Connections>,
}

type Connecting<R, E> = Pin<Box<dyn Future<Output = Result<R, E>> + Send>>;

impl<S, R> Service<R> for TimedConnect<S>
where
    S: Service<R>,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Connecting<S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let connections = self.connections.clone();
        let started = Instant::now();
        let connecting = self.inner.call(request);
        Box::pin(async move {
            let connected = connecting.await;
            if connected.is_ok() {
                connections.opened.fetch_add(1, Ordering::AcqRel);
                tracing::debug!(
                    connect_ms = crate::millis(started.elapsed()),
                    "rest connection opened"
                );
            }
            connected
        })
    }
}
