use std::sync::Arc;

use akari_core::model::{ChannelId, MessageId};
use tokio::runtime::Runtime;

use crate::errors::{GatewayError, RequestError};
use crate::runtime::run;
use crate::store::Store;

/// A logged-in account. Releasing the last reference ends the session like `close()`.
#[derive(uniffi::Object)]
pub struct Account {
    core: Arc<akari_core::Account>,
    runtime: &'static Runtime,
}

impl Account {
    pub(crate) fn new(core: akari_core::Account, runtime: &'static Runtime) -> Arc<Self> {
        Arc::new(Self {
            core: Arc::new(core),
            runtime,
        })
    }
}

#[uniffi::export]
impl Account {
    /// The account's state; it stays readable after the account is closed.
    pub fn store(&self) -> Arc<Store> {
        Store::new(self.core.store().clone())
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

    /// Keeps the channel's messages and adds new ones from the gateway.
    pub fn view_channel(&self, channel_id: ChannelId) {
        self.core.view_channel(channel_id);
    }

    /// Loads messages into the channel's window within Discord's rate limits. `Latest` and
    /// `Around` view the channel first; `Older` and `Newer` do nothing without a window.
    pub async fn load_messages(
        &self,
        channel_id: ChannelId,
        load: MessageLoad,
    ) -> Result<(), RequestError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.load_messages(channel_id, load.into()).await
        })
        .await
        .map_err(Into::into)
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
    /// Replaces the window with the messages around `message_id`.
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
