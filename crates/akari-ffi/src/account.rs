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

    /// The channel the user is reading at its newest messages, or `None`. While set, the
    /// channel is acked up to the newest message its list shows at the present, and again as
    /// that list grows, paced. Hosts set it only while the user is engaged with the channel at
    /// the bottom of its list. A dropped connection clears it: set it again once online and the
    /// list is refreshed.
    pub fn set_reading(&self, channel_id: Option<ChannelId>) {
        self.core.set_reading(channel_id);
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
    /// Returns where the load's time went, or `None` when there was nothing to load.
    pub async fn load_messages(
        &self,
        channel_id: ChannelId,
        load: MessageLoad,
    ) -> Result<Option<LoadReport>, RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.load_messages(channel_id, load.into()).await
        })
        .await
        .map(|report| report.map(Into::into))
        .map_err(Into::into)
    }

    /// Drops the pages of `Around` loads running for the channel, so they don't replace its
    /// window: Escape or a mark as read during a jump, where no `Latest` load overtakes them.
    pub fn supersede_around_loads(&self, channel_id: ChannelId) {
        self.core.supersede_around_loads(channel_id);
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

/// Which messages `Account::load_messages` loads. `limit` is clamped to 1–100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MessageLoad {
    /// The newest messages: fills a window, refreshes a stale one, or jumps to the present.
    Latest { limit: u8 },
    /// Before the window's first message.
    Older { limit: u8 },
    /// After the window's last message, towards the present.
    Newer { limit: u8 },
    /// Replaces the window with the messages around `message_id`, unless a newer `Latest` or
    /// `Around` load, or `Account::supersede_around_loads`, comes before the page.
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
