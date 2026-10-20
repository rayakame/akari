use std::sync::Arc;

use akari_core::model::{ChannelId, GuildId, MessageId, Permissions, UserId};
use akari_core::state;

use crate::contents::{Contents, Found};
use crate::formatted::FormattedText;
use crate::names::Names;
use crate::records::{
    Badge, Channel, ChannelBadge, ConnectionState, ContentStats, Guild, GuildBadge, GuildSettings,
    Message, MessageWindow, ReadState, ServerListItem, Slowmode, StarterMessage, User,
};
use crate::subscription::StoreSubscription;

/// An account's state. Reads are synchronous and cheap (one read lock each) and return
/// copies the host keeps until an event says they changed.
#[derive(uniffi::Object)]
pub struct Store {
    core: state::Store,
    contents: Arc<Contents>,
}

impl Store {
    pub(crate) fn new(core: state::Store, contents: Arc<Contents>) -> Arc<Self> {
        Arc::new(Self { core, contents })
    }

    #[cfg(test)]
    pub(crate) fn core(&self) -> &state::Store {
        &self.core
    }

    fn message(&self, message: &state::Message, names: &Names<'_>, misses: &mut u32) -> Message {
        let (document, found) = self.contents.document(message);
        if found == Found::Miss {
            *misses += 1;
        }
        let formatted = FormattedText::new(&document, &message.content, names, Some(message));
        Message::new(message, formatted, self.core.mentions_me(message))
    }
}

#[uniffi::export]
impl Store {
    /// Changes from now on. Subscribe first, then read, then apply the events.
    pub fn subscribe(&self) -> Arc<StoreSubscription> {
        StoreSubscription::new(self.core.subscribe())
    }

    pub fn connection(&self) -> ConnectionState {
        ConnectionState::from(&self.core.connection())
    }

    /// `None` before the first READY.
    pub fn current_user(&self) -> Option<User> {
        self.core
            .current_user()
            .map(|current| User::from(&current.user))
    }

    /// A DM or group DM recipient.
    pub fn user(&self, id: UserId) -> Option<User> {
        self.core.user(id).map(|user| User::from(&*user))
    }

    /// The server list in the user's order, with their folders. Unavailable guilds are left
    /// out; see `unavailable_guild_ids`.
    pub fn server_list(&self) -> Vec<ServerListItem> {
        self.core
            .server_list()
            .iter()
            .filter_map(ServerListItem::new)
            .collect()
    }

    /// The user's notification settings for a guild, or for DMs with `None`.
    pub fn guild_settings(&self, guild_id: Option<GuildId>) -> GuildSettings {
        let settings = self.core.guild_settings(guild_id);
        GuildSettings::new(&settings, self.core.guild_muted(guild_id))
    }

    /// Of `ids`, the channels muted now by their own settings, in the order given; a thread by
    /// its own membership. A muted category or guild doesn't mute its channels, nor a muted
    /// channel its threads.
    pub fn muted_channels(&self, ids: Vec<ChannelId>) -> Vec<ChannelId> {
        self.core.muted_channels(&ids)
    }

    /// The badges of `ids`, in the order given. A channel the user can't view, or a kind
    /// without messages such as a category, has none.
    pub fn channel_badges(&self, ids: Vec<ChannelId>) -> Vec<ChannelBadge> {
        let badges = self.core.channel_badges(&ids);
        ids.into_iter()
            .zip(badges)
            .map(|(channel_id, badge)| ChannelBadge {
                channel_id,
                badge: Badge::from(badge),
            })
            .collect()
    }

    /// The badges of `ids`, in the order given. `unread` is false while a guild is muted;
    /// its mentions stay. A folder's badge is its guilds' sum.
    pub fn guild_badges(&self, ids: Vec<GuildId>) -> Vec<GuildBadge> {
        let badges = self.core.guild_badges(&ids);
        ids.into_iter()
            .zip(badges)
            .map(|(guild_id, badge)| GuildBadge {
                guild_id,
                badge: Badge::from(badge),
            })
            .collect()
    }

    /// Where the user stopped reading the channel; `None` when Discord sent nothing for it.
    pub fn read_state(&self, channel_id: ChannelId) -> Option<ReadState> {
        self.core.read_state(channel_id).map(ReadState::from)
    }

    /// DMs and group DMs with unread mentions, the newest message first: the rail's DM
    /// icons.
    pub fn unread_private_channels(&self) -> Vec<ChannelId> {
        self.core
            .unread_private_channels()
            .iter()
            .map(|channel| channel.id)
            .collect()
    }

    /// Available guilds in server list order: `server_list` with each folder's guilds in its
    /// place.
    pub fn guild_ids(&self) -> Vec<GuildId> {
        self.core
            .guild_list()
            .iter()
            .map(|guild| guild.id)
            .collect()
    }

    pub fn guild(&self, id: GuildId) -> Option<Guild> {
        self.core.guild(id).map(|guild| Guild::from(&*guild))
    }

    /// Guilds that are down or blocked in the user's region.
    pub fn unavailable_guild_ids(&self) -> Vec<GuildId> {
        self.core.unavailable_guilds()
    }

    /// The guild's channel list as Discord shows it, categories included, each channel followed
    /// by its listed threads, the last joined first.
    pub fn channel_list(&self, guild_id: GuildId) -> Vec<ChannelId> {
        self.core
            .channel_list(guild_id)
            .iter()
            .map(|channel| channel.id)
            .collect()
    }

    /// DMs and group DMs, the latest conversation first.
    pub fn private_channel_list(&self) -> Vec<ChannelId> {
        self.core
            .private_channel_list()
            .iter()
            .map(|channel| channel.id)
            .collect()
    }

    pub fn channel(&self, id: ChannelId) -> Option<Channel> {
        self.core
            .channel(id)
            .map(|channel| Channel::from(&*channel))
    }

    /// The channels among `ids` the store knows, in the order given.
    pub fn channels(&self, ids: Vec<ChannelId>) -> Vec<Channel> {
        ids.into_iter().filter_map(|id| self.channel(id)).collect()
    }

    /// The current user's permissions in a guild channel or thread; `None` for DMs and
    /// unknown channels. In a thread, `SEND_MESSAGES` means the user may send there.
    pub fn permissions(&self, channel_id: ChannelId) -> Option<Permissions> {
        self.core.permissions(channel_id)
    }

    /// The longest message the current user may send: 4,000 characters with Nitro, else 2,000.
    pub fn message_length_limit(&self) -> u32 {
        u32::try_from(self.core.message_length_limit()).unwrap_or(u32::MAX)
    }

    /// The channel's slowmode as it applies to the current user; `None` without slowmode, in
    /// DMs and group DMs, and for unknown channels.
    pub fn slowmode(&self, channel_id: ChannelId) -> Option<Slowmode> {
        self.core.slowmode(channel_id).map(Slowmode::from)
    }

    /// The message IDs and state of a viewed channel; `None` if it isn't viewed.
    pub fn window(&self, channel_id: ChannelId) -> Option<MessageWindow> {
        self.core
            .messages(channel_id)
            .map(|window| MessageWindow::from(&window))
    }

    /// The loaded, pending or failed messages among `ids`, in the order given.
    pub fn messages(&self, channel_id: ChannelId, ids: Vec<MessageId>) -> Vec<Message> {
        let Some(window) = self.core.messages(channel_id) else {
            return Vec::new();
        };
        let names = Names::new(&self.core, channel_id);
        let mut misses = 0;
        let messages = ids
            .into_iter()
            .filter_map(|id| {
                window
                    .messages
                    .binary_search_by_key(&id, |message| message.id)
                    .ok()
                    .and_then(|index| window.messages.get(index))
                    .or_else(|| window.pending.iter().find(|message| message.id == id))
                    .map(|message| self.message(message, &names, &mut misses))
            })
            .collect();
        if misses > 0 {
            tracing::debug!(
                target: "akari_ffi::content",
                channel_id = channel_id.get(),
                misses,
                "parsed on read"
            );
        }
        messages
    }

    /// Whether `Account.fetch_thread` found this session that the user can't see the thread, or
    /// that it is gone.
    pub fn thread_unavailable(&self, thread_id: ChannelId) -> bool {
        self.core.thread_unavailable(thread_id)
    }

    /// The message `thread_id` started from, once its window holds the thread's first message;
    /// `None` for a thread started without one, or before that message is loaded.
    pub fn starter_message(&self, thread_id: ChannelId) -> Option<StarterMessage> {
        Some(match self.core.starter_message(thread_id)? {
            state::StarterMessage::Loaded(message) => {
                let names = Names::new(&self.core, message.channel_id);
                StarterMessage::Loaded {
                    message: self.message(&message, &names, &mut 0),
                }
            }
            state::StarterMessage::Loading => StarterMessage::Loading,
            state::StarterMessage::Unavailable => StarterMessage::Unavailable,
        })
    }

    /// How this account's message contents were parsed so far.
    pub fn content_stats(&self) -> ContentStats {
        self.contents.stats()
    }
}

/// How long `content` is as Discord counts it: Unicode code points.
#[uniffi::export]
pub fn message_length(content: String) -> u32 {
    u32::try_from(akari_core::message_length(&content)).unwrap_or(u32::MAX)
}
