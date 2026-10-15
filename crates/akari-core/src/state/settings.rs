use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::gateway::settings_proto::{GuildFolder, GuildFolders, decode_preloaded};
use crate::gateway::{
    ChannelOverride, NotificationSettings, SettingsProtoUpdate, UserGuildSettings, Versioned,
};
use crate::model::{ChannelId, GuildId, MessageNotificationLevel, Timestamp};

const PRELOADED: u32 = 1;
const GUILD_UNREADS_ALL_MESSAGES: u64 = 1 << 11;
const GUILD_UNREADS_ONLY_MENTIONS: u64 = 1 << 12;
const CHANNEL_UNREADS_ONLY_MENTIONS: u64 = 1 << 9;
const CHANNEL_UNREADS_ALL_MESSAGES: u64 = 1 << 10;
const OPT_IN_CHANNELS_ON: u64 = 1 << 14;
const OPT_IN_ENABLED: u64 = 1 << 12;

/// One entry of the server list, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServerListItem {
    Guild(GuildId),
    Folder(Arc<Folder>),
}

/// A server folder from the user's settings. A guild outside a folder is a
/// [`ServerListItem::Guild`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Folder {
    pub id: i64,
    /// `None` shows the guilds' names.
    pub name: Option<Box<str>>,
    /// 0xRRGGBB; `None` for the default color.
    pub color: Option<u32>,
    /// Available guilds the store knows, in the folder's order.
    pub guilds: Box<[GuildId]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NotificationLevel {
    AllMessages,
    OnlyMentions,
    Nothing,
}

impl NotificationLevel {
    // `None` for Inherit, and for values Akari doesn't know.
    pub(crate) fn from_wire(level: MessageNotificationLevel) -> Option<Self> {
        match level {
            MessageNotificationLevel::AllMessages => Some(Self::AllMessages),
            MessageNotificationLevel::OnlyMentions => Some(Self::OnlyMentions),
            MessageNotificationLevel::NoMessages => Some(Self::Nothing),
            MessageNotificationLevel::Inherit | MessageNotificationLevel::Unknown(_) => None,
        }
    }
}

/// Which unread channels show as unread, when the user chose it apart from the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unreads {
    AllMessages,
    OnlyMentions,
}

/// The user's notification settings for a guild, or for DMs. Discord's defaults where the
/// user changed nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GuildSettings {
    /// As Discord sent it; see `muted_now`.
    pub muted: bool,
    /// When a timed mute ends.
    pub mute_ends: Option<Timestamp>,
    /// `None` uses the guild's `default_message_notifications`.
    pub level: Option<NotificationLevel>,
    pub unreads: Option<Unreads>,
    /// @everyone and @here.
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    pub hide_muted_channels: bool,
    /// The user picks this community server's channels ("Channels & Roles").
    pub opt_in: bool,
    pub channels: Arc<HashMap<ChannelId, ChannelSettings>>,
}

impl GuildSettings {
    pub fn muted_now(&self, now: Timestamp) -> bool {
        muted_at(self.muted, self.mute_ends, now)
    }

    fn from_wire(settings: UserGuildSettings) -> Self {
        let flags = settings.flags;
        Self {
            muted: settings.muted,
            mute_ends: settings.mute_config.and_then(|mute| mute.end_time),
            level: NotificationLevel::from_wire(settings.message_notifications),
            unreads: unreads(
                flags,
                GUILD_UNREADS_ALL_MESSAGES,
                GUILD_UNREADS_ONLY_MENTIONS,
            ),
            suppress_everyone: settings.suppress_everyone,
            suppress_roles: settings.suppress_roles,
            hide_muted_channels: settings.hide_muted_channels,
            opt_in: flags & OPT_IN_CHANNELS_ON != 0,
            channels: Arc::new(
                settings
                    .channel_overrides
                    .into_iter()
                    .map(|channel| (channel.channel_id, ChannelSettings::from_wire(channel)))
                    .collect(),
            ),
        }
    }
}

/// A channel's or a category's own settings within its guild's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChannelSettings {
    pub muted: bool,
    pub mute_ends: Option<Timestamp>,
    /// `None` inherits: the category's, then the guild's.
    pub level: Option<NotificationLevel>,
    pub unreads: Option<Unreads>,
    /// The user chose this channel, or this category's channels, in "Channels & Roles".
    pub opted_in: bool,
}

impl ChannelSettings {
    pub fn muted_now(&self, now: Timestamp) -> bool {
        muted_at(self.muted, self.mute_ends, now)
    }

    // `collapsed` isn't read: Discord's clients keep category collapse to themselves.
    fn from_wire(channel: ChannelOverride) -> Self {
        Self {
            muted: channel.muted,
            mute_ends: channel.mute_config.and_then(|mute| mute.end_time),
            level: NotificationLevel::from_wire(channel.message_notifications),
            unreads: unreads(
                channel.flags,
                CHANNEL_UNREADS_ALL_MESSAGES,
                CHANNEL_UNREADS_ONLY_MENTIONS,
            ),
            opted_in: channel.flags & OPT_IN_ENABLED != 0,
        }
    }
}

fn muted_at(muted: bool, ends: Option<Timestamp>, now: Timestamp) -> bool {
    muted && ends.is_none_or(|end| end > now)
}

fn unreads(flags: u64, all: u64, mentions: u64) -> Option<Unreads> {
    match (flags & all != 0, flags & mentions != 0) {
        (true, false) => Some(Unreads::AllMessages),
        (false, true) => Some(Unreads::OnlyMentions),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UserSettings {
    // `None`: the settings proto has no guild_folders.
    pub(crate) folders: Option<Arc<[FolderEntry]>>,
    // `None` holds the DM settings.
    pub(crate) guilds: HashMap<Option<GuildId>, Arc<GuildSettings>>,
    pub(crate) notification_flags: u64,
    // READY's proto didn't decode, so the store keeps the folders it had.
    pub(crate) unreadable_folders: bool,
}

impl UserSettings {
    pub(crate) fn from_ready(
        guilds: Versioned<UserGuildSettings>,
        proto: Option<String>,
        notifications: Option<NotificationSettings>,
    ) -> Self {
        let mut settings = Self {
            guilds: guilds
                .entries
                .into_iter()
                .map(|entry| (entry.guild_id, Arc::new(GuildSettings::from_wire(entry))))
                .collect(),
            notification_flags: notifications.map_or(0, |notifications| notifications.flags),
            ..Self::default()
        };
        // A missing proto means the defaults: no folders.
        if let Some(proto) = proto {
            match decode_preloaded(&proto) {
                Ok(preloaded) => settings.folders = preloaded.guild_folders.map(folder_entries),
                Err(err) => {
                    tracing::warn!(error = %err, "READY's settings proto not read");
                    settings.unreadable_folders = true;
                }
            }
        }
        settings
    }

    pub(crate) fn guild(&self, guild: Option<GuildId>) -> Option<&Arc<GuildSettings>> {
        self.guilds.get(&guild)
    }

    // Each mute that is on and ends at a set time: a guild's own, and its channels'.
    pub(crate) fn mute_ends(&self) -> impl Iterator<Item = (Option<GuildId>, Timestamp)> + '_ {
        self.guilds.iter().flat_map(|(guild, settings)| {
            let own = settings.mute_ends.filter(|_| settings.muted);
            let channels = settings
                .channels
                .values()
                .filter(|channel| channel.muted)
                .filter_map(|channel| channel.mute_ends);
            own.into_iter()
                .chain(channels)
                .map(move |end| (*guild, end))
        })
    }

    // Returns whether the guild's settings changed.
    pub(crate) fn put_guild(&mut self, settings: UserGuildSettings) -> bool {
        let guild = settings.guild_id;
        let next = GuildSettings::from_wire(settings);
        if self.guilds.get(&guild).is_some_and(|known| **known == next) {
            return false;
        }
        self.guilds.insert(guild, Arc::new(next));
        true
    }

    // A partial update replaces the top-level fields it carries, here the folders.
    pub(crate) fn apply_proto(&mut self, update: SettingsProtoUpdate) {
        if update.settings.kind != PRELOADED {
            return;
        }
        match decode_preloaded(&update.settings.proto) {
            Ok(preloaded) => match (preloaded.guild_folders, update.partial) {
                (Some(folders), _) => self.folders = Some(folder_entries(folders)),
                (None, false) => self.folders = None,
                (None, true) => {}
            },
            Err(err) => tracing::warn!(error = %err, "a settings proto update not read"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderEntry {
    // `None`: not a folder, only the guilds' place.
    pub(crate) id: Option<i64>,
    pub(crate) name: Option<Box<str>>,
    pub(crate) color: Option<u32>,
    pub(crate) guilds: Box<[GuildId]>,
}

fn folder_entries(folders: GuildFolders) -> Arc<[FolderEntry]> {
    folders.folders.into_iter().map(folder_entry).collect()
}

fn folder_entry(folder: GuildFolder) -> FolderEntry {
    FolderEntry {
        id: folder.id,
        name: folder
            .name
            .filter(|name| !name.is_empty())
            .map(String::into_boxed_str),
        color: folder
            .color
            .and_then(|color| u32::try_from(color).ok())
            .filter(|color| *color <= 0xff_ffff),
        guilds: folder.guild_ids.into(),
    }
}

// `by_join` holds the available guilds, newest join first. Guilds the folders don't place
// come first in that order, as Discord shows a newly joined server at the top.
pub(crate) fn server_list(
    folders: Option<&[FolderEntry]>,
    by_join: Vec<GuildId>,
) -> Vec<ServerListItem> {
    let Some(folders) = folders else {
        return by_join.into_iter().map(ServerListItem::Guild).collect();
    };
    let available: HashSet<GuildId> = by_join.iter().copied().collect();
    let placed: HashSet<GuildId> = folders
        .iter()
        .flat_map(|entry| entry.guilds.iter().copied())
        .collect();
    let mut items: Vec<ServerListItem> = by_join
        .into_iter()
        .filter(|guild| !placed.contains(guild))
        .map(ServerListItem::Guild)
        .collect();
    let mut shown = HashSet::new();
    for entry in folders {
        let guilds: Vec<GuildId> = entry
            .guilds
            .iter()
            .copied()
            .filter(|guild| available.contains(guild) && shown.insert(*guild))
            .collect();
        match entry.id {
            None => items.extend(guilds.into_iter().map(ServerListItem::Guild)),
            Some(_) if guilds.is_empty() => {}
            Some(id) => items.push(ServerListItem::Folder(Arc::new(Folder {
                id,
                name: entry.name.clone(),
                color: entry.color,
                guilds: guilds.into(),
            }))),
        }
    }
    items
}

#[cfg(test)]
mod tests;
