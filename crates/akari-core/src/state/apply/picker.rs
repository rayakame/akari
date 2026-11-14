use std::sync::Arc;

use super::State;
use crate::emoji::{EmojiVersion, Query, UnicodeEmoji};
use crate::model::{ChannelId, EmojiId, GuildId, Permissions, PremiumType};
use crate::state::picker::{CustomEmoji, EmojiAccess, EmojiSection, Found, PickerEmoji, Source};
#[cfg(test)]
use crate::state::picker::{ranked, unicode_matches};
use crate::state::reactions::ReactionEmoji;
use crate::state::settings::ServerListItem;
use crate::state::types::GuildEmoji;

// The official client's when the user has no frequently used reactions (canary 671549).
const QUICK_DEFAULTS: [&str; 3] = ["💯", "😆", "💖"];
const QUICK: usize = 3;

// What Discord's emoji filter for reactions asks (canary 671549).
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

    // A forum's default reaction names its custom emoji by ID alone; the request's path wants the
    // name.
    pub(crate) fn name_custom_emoji(&self, emoji: &mut ReactionEmoji) {
        if emoji.name.is_none()
            && let Some(id) = emoji.id
        {
            emoji.name = self.guild_emoji(id).map(|(_, found)| found.name);
        }
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

    pub(crate) fn note_reaction(&mut self, emoji: &ReactionEmoji, now_millis: i64) {
        Arc::make_mut(&mut self.entities.settings.reaction_uses).note(emoji, now_millis);
    }

    // `None`: the picker doesn't list it here, the store doesn't know it, or the host can't draw it.
    fn picker_emoji(
        &self,
        here: Option<&Here>,
        emoji: &ReactionEmoji,
        newest: EmojiVersion,
    ) -> Option<PickerEmoji> {
        match emoji.id {
            Some(id) => {
                let (guild_id, found) = self.guild_emoji(id)?;
                let access = self.access_of(here?, guild_id, &found);
                access.listed().then_some(PickerEmoji::Custom(CustomEmoji {
                    guild_id,
                    emoji: found,
                    access,
                }))
            }
            None => UnicodeEmoji::get(emoji.name.as_deref()?)
                .filter(|found| found.version() <= newest)
                .map(PickerEmoji::Unicode),
        }
    }

    pub(crate) fn recent_reactions(
        &self,
        channel: ChannelId,
        newest: EmojiVersion,
        now_millis: i64,
    ) -> Vec<PickerEmoji> {
        let here = self.emoji_here(channel, now_millis);
        self.entities
            .settings
            .reaction_uses
            .ranked(now_millis)
            .into_iter()
            .filter_map(|used| self.picker_emoji(here.as_ref(), &used.emoji, newest))
            .collect()
    }

    pub(crate) fn quick_reactions(
        &self,
        channel: ChannelId,
        newest: EmojiVersion,
        now_millis: i64,
    ) -> Vec<PickerEmoji> {
        let mut quick: Vec<PickerEmoji> = self
            .recent_reactions(channel, newest, now_millis)
            .into_iter()
            .filter(|emoji| emoji.access().usable())
            .take(QUICK)
            .collect();
        for emoji in QUICK_DEFAULTS
            .iter()
            .filter_map(|emoji| UnicodeEmoji::get(emoji))
        {
            let emoji = PickerEmoji::Unicode(emoji);
            if quick.len() < QUICK && !quick.contains(&emoji) {
                quick.push(emoji);
            }
        }
        quick
    }
}

impl State {
    // Under the store's lock: the custom emoji the picker lists here whose name matches.
    pub(crate) fn custom_matches(
        &self,
        channel: ChannelId,
        query: &Query,
        now_millis: i64,
    ) -> Vec<Found> {
        let Some(here) = self.emoji_here(channel, now_millis) else {
            return Vec::new();
        };
        let mut place = 0;
        let mut found = Vec::new();
        for guild_id in self.emoji_guilds(&here) {
            let Some(guild) = self.entities.guilds.get(&guild_id) else {
                continue;
            };
            let source = if here.guild == Some(guild_id) {
                Source::Here
            } else {
                Source::Elsewhere
            };
            for emoji in guild.emojis.iter() {
                place += 1;
                let Some(rank) = query.rank(&emoji.name.to_lowercase()) else {
                    continue;
                };
                let access = self.access_of(&here, guild_id, emoji);
                if access.listed() {
                    found.push(Found {
                        emoji: PickerEmoji::Custom(CustomEmoji {
                            guild_id,
                            emoji: emoji.clone(),
                            access,
                        }),
                        rank,
                        source,
                        place,
                    });
                }
            }
        }
        found
    }

    #[cfg(test)]
    pub(crate) fn search_emoji(
        &self,
        channel: ChannelId,
        query: &str,
        newest: EmojiVersion,
        limit: usize,
        now_millis: i64,
    ) -> Vec<PickerEmoji> {
        let Some(query) = Query::new(query) else {
            return Vec::new();
        };
        let mut found = self.custom_matches(channel, &query, now_millis);
        found.extend(unicode_matches(&query, newest));
        ranked(found, limit)
    }
}
