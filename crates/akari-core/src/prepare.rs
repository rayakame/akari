use std::sync::Arc;

use crate::state::Message;

/// Work a host does on messages before they enter a window, such as parsing their content, so
/// that its reads find it done. It runs for history pages and the disk cache's messages, never
/// for gateway events. It gets them read-only, outside the store's lock and on a blocking
/// thread, so a slow preparer holds up only its own load.
pub trait MessagePreparer: Send + Sync {
    /// Returns how many messages it prepared, for the load's report.
    fn prepare(&self, messages: &[Arc<Message>]) -> u32;
}
