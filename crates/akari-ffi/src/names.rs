use std::sync::Arc;

use akari_core::model::{ChannelId, ChannelType, GuildId, Permissions, RoleId, UserId};
use akari_core::state;

use crate::formatted::MentionName;

// Looked up on every read, so a rename needs no invalidation (D58).
pub(crate) struct Names<'a> {
    store: Option<&'a state::Store>,
    current: Option<Arc<state::CurrentUser>>,
    guild_id: Option<GuildId>,
    guild: Option<Arc<state::Guild>>,
}

fn known(name: &str) -> MentionName {
    MentionName::Known {
        name: name.to_owned(),
    }
}

impl<'a> Names<'a> {
    pub(crate) fn new(store: &'a state::Store, channel: ChannelId) -> Self {
        let guild_id = store.channel(channel).and_then(|channel| channel.guild_id);
        Self {
            store: Some(store),
            current: store.current_user(),
            guild_id,
            guild: guild_id.and_then(|guild| store.guild(guild)),
        }
    }

    pub(crate) fn none() -> Self {
        Self {
            store: None,
            current: None,
            guild_id: None,
            guild: None,
        }
    }

    // Discord sends the mentioned users with the message; no nicknames until a member cache.
    pub(crate) fn user(&self, id: UserId, message: &state::Message) -> MentionName {
        if let Some(user) = message.mentions.iter().find(|user| user.id == id) {
            return known(user.display_name());
        }
        let (Some(store), Some(current)) = (self.store, &self.current) else {
            return MentionName::NotLoaded;
        };
        if current.user.id == id {
            return known(current.user.display_name());
        }
        if message.author.id == id && message.webhook_id.is_none() {
            return known(message.author.display_name());
        }
        if let Some(user) = store.user(id) {
            return known(user.display_name());
        }
        match message.delivery {
            state::Delivery::Sent => MentionName::Missing,
            _ => MentionName::NotLoaded,
        }
    }

    pub(crate) fn role(&self, id: RoleId) -> (MentionName, u32) {
        if self.current.is_none() {
            return (MentionName::NotLoaded, 0);
        }
        match (self.guild_id, &self.guild) {
            (Some(_), Some(guild)) => guild
                .roles
                .iter()
                .find(|role| role.id == id)
                .map_or((MentionName::Missing, 0), |role| {
                    (known(&role.name), role.color)
                }),
            // The guild is unavailable, or not in the store yet.
            (Some(_), None) => (MentionName::NotLoaded, 0),
            (None, _) => (MentionName::Missing, 0),
        }
    }

    pub(crate) fn channel(&self, id: ChannelId) -> (MentionName, Option<ChannelType>) {
        let Some(store) = self.store else {
            return (MentionName::NotLoaded, None);
        };
        let Some(channel) = store.channel(id) else {
            return match self.current {
                Some(_) => (MentionName::Missing, None),
                None => (MentionName::NotLoaded, None),
            };
        };
        let viewable = store
            .permissions(id)
            .is_none_or(|permissions| permissions.contains(Permissions::VIEW_CHANNEL));
        let name = if !viewable {
            MentionName::NoAccess
        } else if let Some(name) = &channel.name {
            known(name)
        } else {
            let recipients: Vec<String> = channel
                .recipients
                .iter()
                .filter_map(|&user| store.user(user))
                .map(|user| user.display_name().to_owned())
                .collect();
            if recipients.is_empty() {
                MentionName::Missing
            } else {
                known(&recipients.join(", "))
            }
        };
        (name, Some(channel.kind))
    }
}
