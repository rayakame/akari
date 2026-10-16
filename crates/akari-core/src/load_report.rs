use std::time::Duration;

/// Where one history load's time went, and what became of its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LoadReport {
    pub outcome: PageOutcome,
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
    /// In the host's [`MessagePreparer`](crate::MessagePreparer): this page and, for a
    /// channel's first page, the disk cache's messages shown before it.
    pub prepare: Duration,
    /// Messages the preparer reported as prepared.
    pub prepared: u32,
}

/// What became of a history load's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PageOutcome {
    /// It went into the window.
    #[default]
    Applied,
    /// Dropped: a newer `Latest` or `Around` load, or
    /// [`Account::supersede_around_loads`](crate::Account::supersede_around_loads), came after
    /// this `Around` load began.
    Overtaken,
    /// Dropped: the window was replaced, or the end the page continues from changed, meanwhile.
    Outdated,
}
