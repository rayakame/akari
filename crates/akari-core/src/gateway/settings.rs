use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::lenient::skip_invalid;
use crate::model::{ChannelId, GuildId, MessageNotificationLevel, Timestamp};

/// A list that READY sends whole, or as the changes since a version Identify named.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct Versioned<T> {
    #[serde(default, deserialize_with = "skip_invalid")]
    pub entries: Vec<T>,
    /// Only the changes since the version Identify named.
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub version: u64,
}

impl<T> Default for Versioned<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            partial: false,
            version: 0,
        }
    }
}

/// The user's notification settings for a guild or for DMs: an entry of READY's
/// `user_guild_settings`, and USER_GUILD_SETTINGS_UPDATE.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UserGuildSettings {
    /// `None` for DMs.
    pub guild_id: Option<GuildId>,
    #[serde(default)]
    pub muted: bool,
    pub mute_config: Option<MuteConfig>,
    /// `Inherit` means the guild's default.
    #[serde(default = "inherit")]
    pub message_notifications: MessageNotificationLevel,
    /// @everyone and @here.
    #[serde(default)]
    pub suppress_everyone: bool,
    #[serde(default)]
    pub suppress_roles: bool,
    #[serde(default)]
    pub hide_muted_channels: bool,
    /// `UNREADS_ALL_MESSAGES` 1 << 11, `UNREADS_ONLY_MENTIONS` 1 << 12, and others.
    #[serde(default)]
    pub flags: u64,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub channel_overrides: Vec<ChannelOverride>,
}

/// A channel's or a category's settings within a guild's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ChannelOverride {
    pub channel_id: ChannelId,
    /// A category the user collapsed.
    #[serde(default)]
    pub collapsed: bool,
    /// `Inherit` means the category's or the guild's.
    #[serde(default = "inherit")]
    pub message_notifications: MessageNotificationLevel,
    #[serde(default)]
    pub muted: bool,
    pub mute_config: Option<MuteConfig>,
    /// `UNREADS_ONLY_MENTIONS` 1 << 9, `UNREADS_ALL_MESSAGES` 1 << 10, and others.
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MuteConfig {
    /// `None` until the user unmutes.
    pub end_time: Option<Timestamp>,
    /// Seconds; -1 until the user unmutes.
    #[serde(default)]
    pub selected_time_window: i64,
}

/// READY's `notification_settings`, and NOTIFICATION_SETTINGS_UPDATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct NotificationSettings {
    /// `USE_NEW_NOTIFICATIONS` 1 << 4, `MENTION_ON_ALL_MESSAGES` 1 << 5.
    #[serde(default)]
    pub flags: u64,
}

/// USER_SETTINGS_PROTO_UPDATE.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SettingsProtoUpdate {
    pub settings: SettingsProto,
    /// Carries only the fields that changed.
    #[serde(default)]
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SettingsProto {
    /// 1 for the preloaded settings READY carries, 2 for frecency.
    #[serde(rename = "type")]
    pub kind: u32,
    /// Base64.
    pub proto: String,
}

fn inherit() -> MessageNotificationLevel {
    MessageNotificationLevel::Inherit
}
