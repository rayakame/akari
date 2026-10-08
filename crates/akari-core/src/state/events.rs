use std::sync::Arc;

use super::types::{Channel, CurrentUser, Guild, Member, Message, User};
use crate::gateway::GatewayError;
use crate::model::{ChannelId, GuildId, MessageId};

/// A change in a [`Store`](super::Store). Values are the state after the change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StoreEvent {
    Connection(ConnectionState),
    /// A new session's state is in place. After the first READY, read everything; after a
    /// later one, the events before this one described every difference.
    Ready,
    CurrentUserUpdated(Arc<CurrentUser>),
    /// A DM or group DM recipient changed.
    UserUpdated(Arc<User>),
    /// The user joined the guild or it is available again; its channels, threads and the
    /// current user's member can be read.
    GuildAdded(Arc<Guild>),
    /// Settings or roles changed. Role changes can change channel permissions.
    GuildUpdated(Arc<Guild>),
    /// The user left or was removed. Its channels, threads and messages are gone.
    GuildRemoved {
        guild_id: GuildId,
    },
    /// The guild is down. Its channels, threads and messages are gone until `GuildAdded`.
    GuildUnavailable {
        guild_id: GuildId,
    },
    /// The current user's membership changed, which can change channel permissions.
    CurrentMemberUpdated(Arc<Member>),
    /// A channel, category, thread, DM or group DM appeared.
    ChannelAdded(Arc<Channel>),
    ChannelUpdated(Arc<Channel>),
    /// The channel and its messages are gone.
    ChannelRemoved {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    /// A message was added to a viewed channel's window. Discord repeating a loaded message
    /// is a `MessageUpdated` if it changed, and no event otherwise.
    MessageInserted(Arc<Message>),
    /// A loaded message was edited, or Discord added embeds to it.
    MessageUpdated(Arc<Message>),
    MessageDeleted {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    /// Messages were added within `first..=last`: older or newer history, or messages a
    /// refresh filled in. Read the window for that range.
    MessagesLoaded {
        channel_id: ChannelId,
        first: MessageId,
        last: MessageId,
    },
    /// Messages outside `first..=last` were dropped to keep the window within its limit.
    /// They still exist and can be loaded again.
    MessagesTrimmed {
        channel_id: ChannelId,
        first: MessageId,
        last: MessageId,
    },
    /// A new session may have missed changes to the window's messages. They stay until
    /// the window is refreshed; new messages wait for the refresh. Sent again when the
    /// refresh can't reach the window: it stays stale, and `latest` is false until a jump
    /// to the present.
    MessagesStale {
        channel_id: ChannelId,
    },
    /// The window's messages were dropped: the channel was viewed least recently, or a jump
    /// to the present or to a message replaced them.
    MessagesCleared {
        channel_id: ChannelId,
    },
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ConnectionState {
    /// Before `connect()`, or after `disconnect()`.
    Offline,
    /// Connecting, resuming, or waiting to reconnect.
    Connecting,
    /// A session is ready.
    Online,
    /// Closed by `close()`, or ended by a fatal error such as a rejected token.
    Closed { error: Option<Arc<GatewayError>> },
}
