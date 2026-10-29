use std::sync::Arc;

use super::types::{Channel, CurrentUser, Guild, Member, Message, User};
use crate::gateway::GatewayError;
use crate::model::{ChannelId, GuildId, MessageId};

/// A change in a [`Store`](super::Store). Values are the state after the change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StoreEvent {
    Connection(ConnectionState),
    /// The disk cache's state is in place, before the first READY: read everything, as
    /// after READY. The first READY then reports only what changed since.
    Restored,
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
    /// The user left or was removed. Its channels, threads and messages are gone; our pending
    /// and failed messages stay in [`Store::messages`](super::Store::messages) until discarded.
    GuildRemoved {
        guild_id: GuildId,
    },
    /// The guild is down. Its channels, threads and messages are gone until `GuildAdded`; our
    /// pending and failed messages stay in [`Store::messages`](super::Store::messages).
    GuildUnavailable {
        guild_id: GuildId,
    },
    /// The current user's membership changed, which can change channel permissions.
    CurrentMemberUpdated(Arc<Member>),
    /// The server list's order or folders changed: read
    /// [`Store::server_list`](super::Store::server_list) again.
    ServerListChanged,
    /// The display settings changed: read
    /// [`Store::display_settings`](super::Store::display_settings) again.
    DisplaySettingsChanged,
    /// The user agreed to see the guild's age-restricted channels: read
    /// [`Store::age_gate`](super::Store::age_gate) again.
    AgeGateChanged {
        guild_id: GuildId,
    },
    /// The user's notification settings for a guild changed (`None`: for DMs), or a timed
    /// mute in it ended. Mutes, what counts as a mention and which channels are hidden may
    /// have changed.
    GuildSettingsChanged {
        guild_id: Option<GuildId>,
    },
    /// The channel's [`Badge`](super::Badge) or [`ReadState`](super::ReadState) changed.
    BadgeChanged {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    /// The guild's [`Badge`](super::Badge) changed. A folder's badge is its guilds' sum.
    GuildBadgeChanged {
        guild_id: GuildId,
    },
    /// The thread joined or left its guild's [`channel_list`](super::Store::channel_list) while
    /// it didn't change itself: its inactivity window ended, it was read past it, or a message
    /// came after it.
    ThreadListingChanged {
        guild_id: GuildId,
        thread_id: ChannelId,
    },
    /// A thread's newest message, message count or latest message changed without a
    /// `ChannelUpdated`: its place in [`Store::thread_browser`](super::Store::thread_browser) and
    /// its [`latest_message`](super::Store::latest_message) may have changed.
    ThreadActivity {
        guild_id: GuildId,
        parent_id: ChannelId,
        thread_id: ChannelId,
    },
    /// Loading the channel's archived threads moved on: a page began, came, failed or was given
    /// up. Read [`Store::thread_browser`](super::Store::thread_browser) again.
    OlderThreadsChanged {
        parent_id: ChannelId,
    },
    /// The open forum's list changed: it refreshed, a post was deleted or pinned, someone posted,
    /// or loading its archived posts moved on. Read
    /// [`Store::forum_posts`](super::Store::forum_posts) again.
    ForumPostsChanged {
        forum_id: ChannelId,
    },
    /// A forum post's first message arrived, changed or went: read
    /// [`Store::post_preview`](super::Store::post_preview) again.
    PostPreviewChanged {
        post_id: ChannelId,
    },
    /// Discord refused the thread a line in the guild names, or it is gone: see
    /// [`Store::thread_unavailable`](super::Store::thread_unavailable).
    ThreadUnavailable {
        guild_id: GuildId,
        thread_id: ChannelId,
    },
    /// A channel, category, thread, DM or group DM appeared.
    ChannelAdded(Arc<Channel>),
    ChannelUpdated(Arc<Channel>),
    /// The channel and its messages are gone; our pending and failed messages in it stay in
    /// [`Store::messages`](super::Store::messages) until discarded.
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
    /// Discord confirmed a pending message: drop `pending_id`'s row. `message` is in the
    /// window if it is at the present; otherwise it comes with a later load.
    MessageReplaced {
        channel_id: ChannelId,
        pending_id: MessageId,
        message: Arc<Message>,
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
    /// A stale window is current again: a refresh reconciled it, or a load replaced it. Sent
    /// also when that changed none of its messages, so read its state (`stale`, `latest`)
    /// again; new messages show from now on.
    MessagesRefreshed {
        channel_id: ChannelId,
    },
    /// The window's messages were dropped: the channel was viewed least recently, a jump to
    /// the present or to a message replaced them, or newer messages trimmed them all and were
    /// deleted since.
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
