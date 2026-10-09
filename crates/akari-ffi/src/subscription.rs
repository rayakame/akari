use std::sync::{Arc, Mutex, PoisonError};

use akari_core::model::{ChannelId, GuildId, MessageId, UserId};
use akari_core::state;
use tokio_util::sync::CancellationToken;

use crate::records::ConnectionState;

const BATCH: usize = 256;

/// A store's change events, pulled in batches.
#[derive(uniffi::Object)]
pub struct StoreSubscription {
    inner: Mutex<Option<Arc<state::Subscription>>>,
    closed: CancellationToken,
}

impl StoreSubscription {
    pub(crate) fn new(subscription: state::Subscription) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Some(Arc::new(subscription))),
            closed: CancellationToken::new(),
        })
    }

    fn inner(&self) -> Option<Arc<state::Subscription>> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn inner_for_tests(&self) -> std::sync::Weak<state::Subscription> {
        self.inner()
            .map(|inner| Arc::downgrade(&inner))
            .unwrap_or_default()
    }
}

#[uniffi::export]
impl StoreSubscription {
    /// The next changes in order, at most 256; waits while there are none. `None` after
    /// `close()`, or once the account is closed and every event was read.
    pub async fn next(&self) -> Option<Vec<StoreEvent>> {
        // Polled directly, not spawned: the store's queue only gives up events in the poll
        // that returns them, so a dropped call loses nothing.
        loop {
            let subscription = self.inner()?;
            let batch = tokio::select! {
                biased;
                () = self.closed.cancelled() => return None,
                batch = subscription.next_batch(BATCH) => batch,
            };
            if batch.is_empty() {
                return None;
            }
            let events: Vec<StoreEvent> = batch.into_iter().filter_map(StoreEvent::new).collect();
            if !events.is_empty() {
                return Some(events);
            }
        }
    }

    /// Ends the subscription: a waiting `next()` returns `None` and the store stops
    /// buffering for it. Swift cancellation doesn't reach Rust, so call this when done.
    pub fn close(&self) {
        self.closed.cancel();
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

/// A change in the store. IDs only; read the values with the store's reads.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum StoreEvent {
    Connection {
        state: ConnectionState,
    },
    /// A new session's state is in place; read everything.
    Ready,
    CurrentUserUpdated,
    /// A DM or group DM recipient changed.
    UserUpdated {
        user_id: UserId,
    },
    GuildAdded {
        guild_id: GuildId,
    },
    /// Settings or roles changed; role changes can change channel permissions.
    GuildUpdated {
        guild_id: GuildId,
    },
    /// The user left or was removed. Its channels and messages are gone.
    GuildRemoved {
        guild_id: GuildId,
    },
    /// The guild is down; its channels and messages are gone until `GuildAdded`.
    GuildUnavailable {
        guild_id: GuildId,
    },
    /// The current user's membership changed, which can change channel permissions.
    CurrentMemberUpdated {
        guild_id: GuildId,
    },
    ChannelAdded {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    ChannelUpdated {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    ChannelRemoved {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    MessageInserted {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    /// A loaded message was edited, or Discord added embeds to it.
    MessageUpdated {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    MessageDeleted {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    /// Discord confirmed a pending message. The message is in the window if it is at the
    /// present; otherwise it comes with a later load.
    MessageReplaced {
        channel_id: ChannelId,
        pending_id: MessageId,
        message_id: MessageId,
    },
    /// Messages were added within `first..=last`.
    MessagesLoaded {
        channel_id: ChannelId,
        first: MessageId,
        last: MessageId,
    },
    /// Messages outside `first..=last` were dropped to keep the window within its limit.
    MessagesTrimmed {
        channel_id: ChannelId,
        first: MessageId,
        last: MessageId,
    },
    /// A new session may have missed changes; the messages stay until a refresh.
    MessagesStale {
        channel_id: ChannelId,
    },
    /// The window's messages were dropped.
    MessagesCleared {
        channel_id: ChannelId,
    },
}

impl StoreEvent {
    fn new(event: state::StoreEvent) -> Option<Self> {
        use state::StoreEvent as E;

        Some(match event {
            E::Connection(state) => Self::Connection {
                state: ConnectionState::from(&state),
            },
            E::Ready => Self::Ready,
            E::CurrentUserUpdated(_) => Self::CurrentUserUpdated,
            E::UserUpdated(user) => Self::UserUpdated { user_id: user.id },
            E::GuildAdded(guild) => Self::GuildAdded { guild_id: guild.id },
            E::GuildUpdated(guild) => Self::GuildUpdated { guild_id: guild.id },
            E::GuildRemoved { guild_id } => Self::GuildRemoved { guild_id },
            E::GuildUnavailable { guild_id } => Self::GuildUnavailable { guild_id },
            E::CurrentMemberUpdated(member) => Self::CurrentMemberUpdated {
                guild_id: member.guild_id,
            },
            E::ChannelAdded(channel) => Self::ChannelAdded {
                channel_id: channel.id,
                guild_id: channel.guild_id,
            },
            E::ChannelUpdated(channel) => Self::ChannelUpdated {
                channel_id: channel.id,
                guild_id: channel.guild_id,
            },
            E::ChannelRemoved {
                channel_id,
                guild_id,
            } => Self::ChannelRemoved {
                channel_id,
                guild_id,
            },
            E::MessageInserted(message) => Self::MessageInserted {
                channel_id: message.channel_id,
                message_id: message.id,
            },
            E::MessageUpdated(message) => Self::MessageUpdated {
                channel_id: message.channel_id,
                message_id: message.id,
            },
            E::MessageDeleted {
                channel_id,
                message_id,
            } => Self::MessageDeleted {
                channel_id,
                message_id,
            },
            E::MessageReplaced {
                channel_id,
                pending_id,
                message,
            } => Self::MessageReplaced {
                channel_id,
                pending_id,
                message_id: message.id,
            },
            E::MessagesLoaded {
                channel_id,
                first,
                last,
            } => Self::MessagesLoaded {
                channel_id,
                first,
                last,
            },
            E::MessagesTrimmed {
                channel_id,
                first,
                last,
            } => Self::MessagesTrimmed {
                channel_id,
                first,
                last,
            },
            E::MessagesStale { channel_id } => Self::MessagesStale { channel_id },
            E::MessagesCleared { channel_id } => Self::MessagesCleared { channel_id },
            other => {
                tracing::debug!(?other, "skipping a store event the bindings don't know");
                return None;
            }
        })
    }
}
