use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

use crate::errors::ClientError;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

// Never shut down: accounts and running requests can outlive any one client, and a runtime
// can't be dropped from one of its own threads.
pub(crate) fn runtime() -> Result<&'static Runtime, ClientError> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("akari")
        .enable_all()
        .build()
        .map_err(|err| {
            tracing::error!(error = %err, "couldn't start the runtime");
            ClientError::Runtime
        })?;
    Ok(RUNTIME.get_or_init(|| runtime))
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
