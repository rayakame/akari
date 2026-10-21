use std::sync::Arc;

use akari_core::model::{ChannelId, GuildId, MessageId};
use tokio::runtime::Runtime;

use crate::contents::Contents;
use crate::errors::{GatewayError, RequestError};
use crate::records::LoadReport;
use crate::runtime::run;
use crate::store::Store;

/// A logged-in account. Releasing the last reference ends the session like `close()`.
#[derive(uniffi::Object)]
pub struct Account {
    core: Arc<akari_core::Account>,
    runtime: &'static Runtime,
    contents: Arc<Contents>,
}

impl Account {
    pub(crate) fn new(core: akari_core::Account, runtime: &'static Runtime) -> Arc<Self> {
        let contents = Contents::new();
        core.set_message_preparer(contents.clone());
        Arc::new(Self {
            core: Arc::new(core),
            runtime,
            contents,
        })
    }

    #[cfg(test)]
    pub(crate) fn contents(&self) -> &Arc<Contents> {
        &self.contents
    }
}

#[uniffi::export]
impl Account {
    /// The account's state; it stays readable after the account is closed.
    pub fn store(&self) -> Arc<Store> {
        Store::new(self.core.store().clone(), self.contents.clone())
    }

    /// Connects, or resumes after `disconnect()`. Fails with `Closed` after `close()`.
    pub fn connect(&self) -> Result<(), GatewayError> {
        let _entered = self.runtime.enter();
        self.core.connect().map_err(Into::into)
    }

    /// Closes the connection but keeps the session, e.g. before the Mac sleeps.
    pub fn disconnect(&self) {
        self.core.disconnect();
    }

    /// Ends the session.
    pub fn close(&self) {
        self.core.close();
    }

    /// Writes what changed to the disk cache now, e.g. before the app quits. Resolves when
    /// it's written, or at once without a cache.
    pub async fn flush_cache(&self) {
        let core = self.core.clone();
        run(self.runtime, async move { core.flush_cache().await }).await;
    }

    /// Keeps the channel's messages and adds new ones from the gateway.
    pub fn view_channel(&self, channel_id: ChannelId) {
        self.core.view_channel(channel_id);
    }

    /// The channels the user is reading at their newest messages, at most two (a channel and
    /// the thread beside it); further ones are ignored, and an empty list reads none. Each is
    /// acked up to the newest message its list shows at the present, and again as that list
    /// grows, paced per channel. Hosts name a channel only while the user is engaged with it at
    /// the bottom of its list. A dropped connection clears them: set them again once online and
    /// the lists are refreshed.
    pub fn set_reading(&self, channel_ids: Vec<ChannelId>) {
        self.core.set_reading(&channel_ids);
    }

    /// Marks the channel read now (Escape, the context menu).
    pub fn mark_read(&self, channel_id: ChannelId) {
        self.core.mark_read(channel_id);
    }

    /// Marks these guilds read now with one bulk ack (Shift+Escape, a server's or a folder's
    /// context menu).
    pub fn mark_guilds_read(&self, guild_ids: Vec<GuildId>) {
        self.core.mark_guilds_read(&guild_ids);
    }

    /// Loads messages into the channel's window within Discord's rate limits. `Latest` and
    /// `Around` view the channel first; `Older` and `Newer` do nothing without a window.
    /// `placing` from `reserve_placing`, taken where the user acted, orders a `Latest` or
    /// `Around` load before later supersedes and loads; `None` takes its place when it begins.
    /// Returns where the load's time went, or `None` when there was nothing to load.
    pub async fn load_messages(
        &self,
        channel_id: ChannelId,
        load: MessageLoad,
        placing: Option<u64>,
    ) -> Result<Option<LoadReport>, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            match placing {
                Some(placing) => {
                    core.load_messages_placed(channel_id, load.into(), placing)
                        .await
                }
                None => core.load_messages(channel_id, load.into()).await,
            }
        })
        .await
        .map(|report| report.map(Into::into))
        .map_err(Into::into)
    }

    /// Fetches a thread that a "started a thread" line in `parent_id` names when the store
    /// doesn't have it (archived, or never synced) and keeps it, unjoined unless the user is a
    /// member. Returns whether the store has it now; when Discord refuses it or it is gone,
    /// `Store.thread_unavailable` says so for the session.
    pub async fn fetch_thread(
        &self,
        parent_id: ChannelId,
        thread_id: ChannelId,
    ) -> Result<bool, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.fetch_thread(parent_id, thread_id).await
        })
        .await
        .map_err(Into::into)
    }

    /// Joins the thread; the store has it joined once Discord says yes. `InvalidRequest` for an
    /// archived thread or one the store doesn't have, before asking.
    pub async fn join_thread(
        &self,
        thread_id: ChannelId,
        location: ThreadLocation,
    ) -> Result<(), RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.join_thread(thread_id, location.into()).await
        })
        .await
        .map_err(Into::into)
    }

    /// Leaves the thread, as `join_thread` joins it.
    pub async fn leave_thread(
        &self,
        thread_id: ChannelId,
        location: ThreadLocation,
    ) -> Result<(), RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.leave_thread(thread_id, location.into()).await
        })
        .await
        .map_err(Into::into)
    }

    /// Drops the pages of `Around` loads running for the channel, so they don't replace its
    /// window: Escape or a mark as read during a jump, where no `Latest` load overtakes them.
    pub fn supersede_around_loads(&self, channel_id: ChannelId) {
        self.core.supersede_around_loads(channel_id);
    }

    /// The next place among loads and supersedes, for `load_messages`; take it where the user
    /// acts, so a supersede after it overtakes the load even before the load begins.
    pub fn reserve_placing(&self) -> u64 {
        self.core.reserve_placing()
    }

    /// Shows the message as pending at once and resolves with Discord's ID. On an error the
    /// message stays as failed until retried or discarded.
    pub async fn send_message(
        &self,
        channel_id: ChannelId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.send_message(channel_id, content).await
        })
        .await
        .map_err(Into::into)
    }

    /// Shows the message as pending and returns its provisional ID, or refuses (empty or too
    /// long content, a closed account) before anything is queued; `deliver_message` sends it.
    pub fn queue_message(
        &self,
        channel_id: ChannelId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        self.core
            .queue_message(channel_id, &content)
            .map_err(Into::into)
    }

    /// Sends a message `queue_message` queued; a detached window jumps to the present, as with
    /// `send_message`. On an error the message stays as failed. A second call for a message
    /// already on its way fails with `InvalidRequest`.
    pub async fn deliver_message(
        &self,
        channel_id: ChannelId,
        pending_id: MessageId,
    ) -> Result<MessageId, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.deliver_message(channel_id, pending_id).await
        })
        .await
        .map_err(Into::into)
    }

    /// Sends a failed message again with the same nonce.
    pub async fn retry_message(
        &self,
        channel_id: ChannelId,
        pending_id: MessageId,
    ) -> Result<MessageId, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.retry_message(channel_id, pending_id).await
        })
        .await
        .map_err(Into::into)
    }

    /// Drops a failed message.
    pub fn discard_message(&self, channel_id: ChannelId, pending_id: MessageId) {
        self.core.discard_message(channel_id, pending_id);
    }
}

/// Where the user chose to join or leave a thread, which Discord's own client tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ThreadLocation {
    /// The "…" menu of a thread beside its parent.
    SidebarOverflow,
    /// The "…" menu of a thread open full width.
    ToolbarOverflow,
    /// A thread row's context menu.
    ContextMenu,
}

impl From<ThreadLocation> for akari_core::ThreadLocation {
    fn from(location: ThreadLocation) -> Self {
        match location {
            ThreadLocation::SidebarOverflow => Self::SidebarOverflow,
            ThreadLocation::ToolbarOverflow => Self::ToolbarOverflow,
            ThreadLocation::ContextMenu => Self::ContextMenu,
        }
    }
}

/// Which messages `Account::load_messages` loads. `limit` is clamped to 1–100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MessageLoad {
    /// The newest messages: fills a window, refreshes a stale one, or jumps to the present; not
    /// when it would replace the window for a later-placed `Around` (`PageOutcome::Overtaken`).
    Latest { limit: u8 },
    /// Before the window's first message.
    Older { limit: u8 },
    /// After the window's last message, towards the present.
    Newer { limit: u8 },
    /// Replaces the window with the messages around `message_id`, unless a later-placed `Latest`
    /// or `Around` load, or `Account::supersede_around_loads`, comes first.
    Around { message_id: MessageId, limit: u8 },
}

impl From<MessageLoad> for akari_core::MessageLoad {
    fn from(load: MessageLoad) -> Self {
        match load {
            MessageLoad::Latest { limit } => Self::Latest { limit },
            MessageLoad::Older { limit } => Self::Older { limit },
            MessageLoad::Newer { limit } => Self::Newer { limit },
            MessageLoad::Around { message_id, limit } => Self::Around {
                id: message_id,
                limit,
            },
        }
    }
}
