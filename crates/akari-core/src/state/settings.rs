use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::gateway::settings_proto::{
    GuildFolder, GuildFolders, Redactions, TextAndImages, decode_preloaded,
};
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

/// How messages' media shows, from the settings proto's `text_and_images` (D138); Discord's
/// defaults where the user set nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DisplaySettings {
    pub render_spoilers: SpoilerSetting,
    /// Show the ALT button on media with a description.
    pub view_image_descriptions: bool,
    /// Show attachments' media inline.
    pub inline_attachment_media: bool,
    /// Show linked media inline.
    pub inline_embed_media: bool,
    /// Play GIFs while the client is focused.
    pub gif_auto_play: bool,
    pub render_embeds: bool,
    pub animate_emoji: bool,
    pub animate_stickers: StickerAnimation,
    pub explicit_content_filter: ExplicitContentFilter,
    pub explicit_content: ExplicitContentSettings,
    pub gore: ExplicitContentSettings,
    pub self_harm: ExplicitContentSettings,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            render_spoilers: SpoilerSetting::OnClick,
            view_image_descriptions: false,
            inline_attachment_media: true,
            inline_embed_media: true,
            gif_auto_play: true,
            render_embeds: true,
            animate_emoji: true,
            animate_stickers: StickerAnimation::Always,
            explicit_content_filter: ExplicitContentFilter::NonFriends,
            explicit_content: ExplicitContentSettings::default(),
            gore: ExplicitContentSettings::default(),
            self_harm: ExplicitContentSettings::default(),
        }
    }
}

impl DisplaySettings {
    // Each absent or unknown value takes the default.
    fn from_proto(proto: Option<&TextAndImages>) -> Self {
        let defaults = Self::default();
        let Some(proto) = proto else {
            return defaults;
        };
        Self {
            render_spoilers: match proto.render_spoilers.as_deref() {
                Some("ALWAYS") => SpoilerSetting::Always,
                Some("IF_MODERATOR") => SpoilerSetting::IfModerator,
                _ => SpoilerSetting::OnClick,
            },
            view_image_descriptions: proto
                .view_image_descriptions
                .unwrap_or(defaults.view_image_descriptions),
            inline_attachment_media: proto
                .inline_attachment_media
                .unwrap_or(defaults.inline_attachment_media),
            inline_embed_media: proto
                .inline_embed_media
                .unwrap_or(defaults.inline_embed_media),
            gif_auto_play: proto.gif_auto_play.unwrap_or(defaults.gif_auto_play),
            render_embeds: proto.render_embeds.unwrap_or(defaults.render_embeds),
            animate_emoji: proto.animate_emoji.unwrap_or(defaults.animate_emoji),
            animate_stickers: match proto.animate_stickers {
                Some(1) => StickerAnimation::OnInteraction,
                Some(2) => StickerAnimation::Never,
                _ => StickerAnimation::Always,
            },
            explicit_content_filter: match proto.explicit_content_filter {
                Some(0) => ExplicitContentFilter::Disabled,
                Some(2) => ExplicitContentFilter::All,
                _ => ExplicitContentFilter::NonFriends,
            },
            explicit_content: ExplicitContentSettings::from_proto(proto.explicit_content),
            gore: ExplicitContentSettings::from_proto(proto.gore),
            self_harm: ExplicitContentSettings::from_proto(proto.self_harm),
        }
    }
}

/// When spoilers are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SpoilerSetting {
    Always,
    /// Discord's default.
    OnClick,
    /// Always for those with Manage Messages.
    IfModerator,
}

/// When stickers animate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StickerAnimation {
    /// Discord's default.
    Always,
    /// While the message is hovered.
    OnInteraction,
    Never,
}

/// The legacy explicit content filter, which decides the defaults of the per-context settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExplicitContentFilter {
    Disabled,
    /// Discord's default.
    NonFriends,
    All,
}

/// How flagged media is shown in each context; `Unset` leaves it to the filter's defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExplicitContentSettings {
    pub guilds: Redaction,
    pub friend_dm: Redaction,
    pub non_friend_dm: Redaction,
}

impl ExplicitContentSettings {
    fn from_proto(proto: Option<Redactions>) -> Self {
        let Some(proto) = proto else {
            return Self::default();
        };
        Self {
            guilds: Redaction::from_proto(proto.guilds),
            friend_dm: Redaction::from_proto(proto.friend_dm),
            non_friend_dm: Redaction::from_proto(proto.non_friend_dm),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Redaction {
    #[default]
    Unset,
    Show,
    Blur,
    Block,
}

impl Redaction {
    fn from_proto(value: u32) -> Self {
        match value {
            1 => Self::Show,
            2 => Self::Blur,
            3 => Self::Block,
            _ => Self::Unset,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UserSettings {
    pub(crate) display: DisplaySettings,
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
                Ok(preloaded) => {
                    settings.folders = preloaded.guild_folders.map(folder_entries);
                    settings.display =
                        DisplaySettings::from_proto(preloaded.text_and_images.as_ref());
                }
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

    // A partial update replaces the top-level fields it carries; a full one every field.
    // Returns whether the display settings changed.
    pub(crate) fn apply_proto(&mut self, update: SettingsProtoUpdate) -> bool {
        if update.settings.kind != PRELOADED {
            return false;
        }
        match decode_preloaded(&update.settings.proto) {
            Ok(preloaded) => {
                match (preloaded.guild_folders, update.partial) {
                    (Some(folders), _) => self.folders = Some(folder_entries(folders)),
                    (None, false) => self.folders = None,
                    (None, true) => {}
                }
                let display = match (preloaded.text_and_images, update.partial) {
                    (Some(display), _) => DisplaySettings::from_proto(Some(&display)),
                    (None, false) => DisplaySettings::default(),
                    (None, true) => self.display,
                };
                let changed = display != self.display;
                self.display = display;
                changed
            }
            Err(err) => {
                tracing::warn!(error = %err, "a settings proto update not read");
                false
            }
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
