use tracing_subscriber::EnvFilter;

use crate::errors::ClientError;

/// Sends akari-core's logs to stderr without colors, filtered like `RUST_LOG`, e.g.
/// `akari_core=debug`. Logs hold IDs and event names, never content or tokens. Only the
/// first call takes effect.
#[uniffi::export]
pub fn enable_logging(filter: String) -> Result<(), ClientError> {
    let filter = EnvFilter::try_new(filter).map_err(|_| ClientError::InvalidLogFilter)?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .try_init();
    Ok(())
}
