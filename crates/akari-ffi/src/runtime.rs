use std::future::Future;
use std::io;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

use crate::errors::ClientError;

static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();

// Never shut down: accounts and running requests can outlive any one client, and a runtime
// can't be dropped from one of its own threads.
pub(crate) fn runtime() -> Result<&'static Runtime, ClientError> {
    start(&RUNTIME, || {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("akari")
            .enable_all()
            .build()
    })
}

pub(crate) fn start(
    cell: &'static OnceLock<Option<Runtime>>,
    build: impl FnOnce() -> io::Result<Runtime>,
) -> Result<&'static Runtime, ClientError> {
    // Built inside get_or_init: a spare built by a racing caller would have to be dropped,
    // which panics in an async context.
    cell.get_or_init(|| {
        build()
            .inspect_err(|err| tracing::error!(error = %err, "couldn't start the runtime"))
            .ok()
    })
    .as_ref()
    .ok_or(ClientError::Runtime)
}

// Awaitable from any executor, as UniFFI polls from the host's threads. A panic in `work` is
// raised again here, where UniFFI reports it.
pub(crate) async fn run<T: Send + 'static>(
    runtime: &'static Runtime,
    work: impl Future<Output = T> + Send + 'static,
) -> T {
    match runtime.spawn(work).await {
        Ok(value) => value,
        Err(err) => match err.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            // Tasks are only cancelled when the runtime shuts down, which it never does.
            Err(err) => std::panic::resume_unwind(Box::new(err.to_string())),
        },
    }
}
