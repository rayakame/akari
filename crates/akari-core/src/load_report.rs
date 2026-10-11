use std::time::Duration;

/// Where one history load's time went. Numbers only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LoadReport {
    /// Behind earlier requests on the same channel (they run one at a time) or a warm-up.
    pub queued: Duration,
    /// Waiting for Discord's limits: a bucket reset, a 429 pause or the global limit.
    pub limited: Duration,
    /// From sending until the response headers, over all attempts.
    pub headers: Duration,
    /// A connection was opened for it (DNS, TCP, TLS). Approximate when requests overlap.
    pub new_connection: bool,
    pub body: Duration,
    /// After decompression.
    pub bytes: u32,
    pub decode: Duration,
    /// Into the window, under the store's write lock.
    pub store: Duration,
    /// Messages in the page, including ones that didn't decode.
    pub received: u16,
    /// 429 and 5xx answers that were retried.
    pub retries: u8,
    /// `X-RateLimit-Remaining` and `-Reset-After` of the last answer, when Discord sent them.
    pub remaining: Option<u32>,
    pub reset_after: Option<Duration>,
}
