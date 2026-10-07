use serde::Deserialize;

use super::int_enum::int_enum;
use super::permissions::Permissions;
use super::snowflake::{ChannelMarker, GuildMarker, RoleMarker, Snowflake, UserMarker};
use super::timestamp::Timestamp;
use super::user::User;

/// A guild's own settings. Its channels, roles and members arrive next to it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Guild {
    pub id: Snowflake<GuildMarker>,
    pub name: String,
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub splash: Option<String>,
    pub description: Option<String>,
    pub owner_id: Option<Snowflake<UserMarker>>,
    /// Feature names such as `COMMUNITY`.
    #[serde(default)]
    pub features: Vec<String>,
    pub afk_channel_id: Option<Snowflake<ChannelMarker>>,
    /// Seconds.
    pub afk_timeout: Option<u32>,
    pub system_channel_id: Option<Snowflake<ChannelMarker>>,
    pub rules_channel_id: Option<Snowflake<ChannelMarker>>,
    pub vanity_url_code: Option<String>,
    #[serde(default = "default_locale")]
    pub preferred_locale: String,
    #[serde(default)]
    pub default_message_notifications: MessageNotificationLevel,
    #[serde(default)]
    pub nsfw_level: NsfwLevel,
    /// Boost level.
    #[serde(default)]
    pub premium_tier: PremiumTier,
}

// The reference's documented default.
fn default_locale() -> String {
    "en-US".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Role {
    pub id: Snowflake<RoleMarker>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: u32,
    /// Supersedes `color`, but the reference's own examples sometimes omit it.
    pub colors: Option<RoleColors>,
    #[serde(default)]
    pub hoist: bool,
    pub icon: Option<String>,
    pub unicode_emoji: Option<String>,
    pub position: i32,
    pub permissions: Permissions,
    #[serde(default)]
    pub managed: bool,
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RoleColors {
    #[serde(default)]
    pub primary_color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
}

/// A user's membership in a guild.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GuildMember {
    /// Missing on the member in MESSAGE_CREATE and in deduplicated READY payloads.
    pub user: Option<User>,
    /// Set instead of `user` in deduplicated READY payloads.
    pub user_id: Option<Snowflake<UserMarker>>,
    pub nick: Option<String>,
    /// Guild-specific avatar hash.
    pub avatar: Option<String>,
    pub banner: Option<String>,
    #[serde(default)]
    pub roles: Vec<Snowflake<RoleMarker>>,
    pub joined_at: Option<Timestamp>,
    /// When the member started boosting the guild.
    pub premium_since: Option<Timestamp>,
    /// End of a timeout; a time in the past means the timeout is over.
    pub communication_disabled_until: Option<Timestamp>,
    #[serde(default)]
    pub deaf: bool,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub flags: u64,
}

int_enum! {
    /// Which messages notify members by default.
    #[derive(Default)]
    pub enum MessageNotificationLevel {
        #[default]
        AllMessages = 0,
        OnlyMentions = 1,
        NoMessages = 2,
        Inherit = 3,
    }
}

int_enum! {
    #[derive(Default)]
    pub enum NsfwLevel {
        #[default]
        Default = 0,
        Explicit = 1,
        Safe = 2,
        AgeRestricted = 3,
    }
}

int_enum! {
    /// A guild's boost level.
    #[derive(Default)]
    pub enum PremiumTier {
        #[default]
        None = 0,
        Tier1 = 1,
        Tier2 = 2,
        Tier3 = 3,
    }
}
