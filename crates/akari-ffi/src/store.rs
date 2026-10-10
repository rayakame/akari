use std::sync::Arc;

use akari_core::model::{ChannelId, GuildId, MessageId, Permissions, UserId};
use akari_core::state;

use crate::records::{Channel, ConnectionState, Guild, Message, MessageWindow, User};
use crate::subscription::StoreSubscription;

/// An account's state. Reads are synchronous and cheap (one read lock each) and return
/// copies the host keeps until an event says they changed.
#[derive(uniffi::Object)]
pub struct Store {
    core: state::Store,
}

impl Store {
    pub(crate) fn new(core: state::Store) -> Arc<Self> {
        Arc::new(Self { core })
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

    /// Available guilds in server list order.
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

    /// The guild's channel list as Discord shows it, categories included.
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
        ids.into_iter()
            .filter_map(|id| {
                window
                    .messages
                    .binary_search_by_key(&id, |message| message.id)
                    .ok()
                    .and_then(|index| window.messages.get(index))
                    .or_else(|| window.pending.iter().find(|message| message.id == id))
                    .map(|message| Message::from(&**message))
            })
            .collect()
    }
}
