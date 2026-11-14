use super::State;
use crate::model::{ChannelId, EmojiId, GuildId, Permissions, PremiumType};
use crate::state::picker::{CustomEmoji, EmojiAccess, EmojiSection};
use crate::state::settings::ServerListItem;
use crate::state::types::GuildEmoji;

// What decides a custom emoji's use in a channel (the REACTION intention of Discord's emoji
// filter, research §2.1).
pub(crate) struct Here {
    // `None` in a DM or group DM, where every server's emoji is another's.
    guild: Option<GuildId>,
    external: bool,
    nitro: bool,
}

impl State {
    pub(crate) fn emoji_here(&self, channel: ChannelId, now_millis: i64) -> Option<Here> {
        let place = self.entities.channels.get(&channel)?;
        let guild = place.guild_id;
        let external = guild.is_none()
            || self
                .permissions(channel, now_millis)
                .is_some_and(|permissions| permissions.contains(Permissions::USE_EXTERNAL_EMOJIS));
        // Any tier counts for emoji.
        let nitro = self
            .entities
            .current_user
            .as_ref()
            .is_some_and(|me| me.premium_type != PremiumType::None);
        Some(Here {
            guild,
            external,
            nitro,
        })
    }

    pub(crate) fn access_of(&self, here: &Here, guild: GuildId, emoji: &GuildEmoji) -> EmojiAccess {
        // What the picker hides goes first: an unavailable emoji there would only be noise.
        let other = here.guild != Some(guild);
        if other && here.guild.is_some() && !here.external {
            return EmojiAccess::External;
        }
        if !emoji.roles.is_empty()
            && !self
                .entities
                .members
                .get(&guild)
                .is_some_and(|member| member.roles.iter().any(|role| emoji.roles.contains(role)))
        {
            return EmojiAccess::RoleLocked;
        }
        if !emoji.available {
            return EmojiAccess::Unavailable;
        }
        if !here.nitro && (emoji.animated || (other && !emoji.managed)) {
            return EmojiAccess::NitroLocked;
        }
        EmojiAccess::Usable
    }

    pub(crate) fn emoji_access(
        &self,
        channel: ChannelId,
        emoji: EmojiId,
        now_millis: i64,
    ) -> Option<EmojiAccess> {
        let here = self.emoji_here(channel, now_millis)?;
        let (guild, found) = self.guild_emoji(emoji)?;
        Some(self.access_of(&here, guild, &found))
    }

    pub(crate) fn guild_emoji(&self, emoji: EmojiId) -> Option<(GuildId, GuildEmoji)> {
        self.entities.guilds.values().find_map(|guild| {
            let index = guild
                .emojis
                .binary_search_by_key(&emoji, |known| known.id)
                .ok()?;
            Some((guild.id, guild.emojis[index].clone()))
        })
    }

    // The channel's own server first, then the rail's order.
    pub(crate) fn emoji_guilds(&self, here: &Here) -> Vec<GuildId> {
        let rail = self
            .entities
            .server_list()
            .into_iter()
            .flat_map(|item| match item {
                ServerListItem::Guild(id) => vec![id],
                ServerListItem::Folder(folder) => folder.guilds.to_vec(),
            })
            .filter(|id| Some(*id) != here.guild);
        here.guild
            .into_iter()
            .chain(rail)
            .filter(|id| self.entities.guilds.contains_key(id))
            .collect()
    }

    pub(crate) fn custom_emoji_sections(
        &self,
        channel: ChannelId,
        now_millis: i64,
    ) -> Vec<EmojiSection> {
        let Some(here) = self.emoji_here(channel, now_millis) else {
            return Vec::new();
        };
        self.emoji_guilds(&here)
            .into_iter()
            .filter_map(|guild_id| {
                let guild = self.entities.guilds.get(&guild_id)?;
                let emojis: Vec<CustomEmoji> = guild
                    .emojis
                    .iter()
                    .filter_map(|emoji| {
                        let access = self.access_of(&here, guild_id, emoji);
                        access.listed().then(|| CustomEmoji {
                            guild_id,
                            emoji: emoji.clone(),
                            access,
                        })
                    })
                    .collect();
                (!emojis.is_empty()).then_some(EmojiSection { guild_id, emojis })
            })
            .collect()
    }
}
