use serde::de::{self, Deserialize, Deserializer};

use crate::lenient::skip_invalid_option;
use crate::model::{
    Attachment, ChannelId, ChannelType, Embed, GuildId, MessageId, MessageType,
    PermissionOverwrite, PremiumType, Role, RoleId, StickerItem, ThreadMetadata, Timestamp, User,
    UserId, WebhookId, double_option,
};

/// GUILD_UPDATE. A missing field is unchanged, `Some(None)` is `null`, a list replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildUpdate {
    pub id: GuildId,
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub banner: Option<Option<String>>,
    pub owner_id: Option<UserId>,
    pub roles: Option<Vec<Role>>,
}

#[derive(Default, serde::Deserialize)]
struct GuildFields {
    id: Option<GuildId>,
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    banner: Option<Option<String>>,
    owner_id: Option<UserId>,
    roles: Option<Vec<Role>>,
    properties: Option<Box<GuildFields>>,
}

// The reference documents a flat guild object, but CLIENT_STATE_V2 moves a guild's own
// fields into `properties` in READY and GUILD_CREATE, so both are read.
impl<'de> Deserialize<'de> for GuildUpdate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut top = GuildFields::deserialize(deserializer)?;
        let nested = top
            .properties
            .take()
            .map(|nested| *nested)
            .unwrap_or_default();
        Ok(Self {
            id: nested
                .id
                .or(top.id)
                .ok_or_else(|| de::Error::missing_field("id"))?,
            name: nested.name.or(top.name),
            icon: nested.icon.or(top.icon),
            banner: nested.banner.or(top.banner),
            owner_id: nested.owner_id.or(top.owner_id),
            roles: top.roles.or(nested.roles),
        })
    }
}

/// CHANNEL_UPDATE and THREAD_UPDATE. A missing field is unchanged, `Some(None)` is `null`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ChannelUpdate {
    pub id: ChannelId,
    #[serde(rename = "type")]
    pub kind: Option<ChannelType>,
    pub guild_id: Option<GuildId>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id: Option<Option<ChannelId>>,
    #[serde(default, deserialize_with = "double_option")]
    pub name: Option<Option<String>>,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub topic: Option<Option<String>>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub permission_overwrites: Option<Vec<PermissionOverwrite>>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub recipients: Option<Vec<User>>,
    #[serde(default, deserialize_with = "double_option")]
    pub icon: Option<Option<String>>,
    pub owner_id: Option<UserId>,
    pub thread_metadata: Option<ThreadMetadata>,
    pub message_count: Option<u32>,
    pub member_count: Option<u32>,
    pub flags: Option<u64>,
}

/// MESSAGE_UPDATE. A missing field is unchanged, `Some(None)` is `null`. `tts` isn't
/// read: Discord always sends `false` in updates.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct MessageUpdate {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    #[serde(rename = "type")]
    pub kind: Option<MessageType>,
    pub author: Option<User>,
    pub webhook_id: Option<WebhookId>,
    pub content: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub edited_timestamp: Option<Option<Timestamp>>,
    pub mention_everyone: Option<bool>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub mentions: Option<Vec<User>>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub mention_roles: Option<Vec<RoleId>>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub attachments: Option<Vec<Attachment>>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub embeds: Option<Vec<Embed>>,
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub sticker_items: Option<Vec<StickerItem>>,
    pub pinned: Option<bool>,
    pub flags: Option<u64>,
}

/// GUILD_MEMBER_UPDATE. A missing field is unchanged, `Some(None)` is `null`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct GuildMemberUpdate {
    pub guild_id: GuildId,
    pub user: User,
    #[serde(default, deserialize_with = "double_option")]
    pub nick: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub avatar: Option<Option<String>>,
    pub roles: Option<Vec<RoleId>>,
    pub joined_at: Option<Timestamp>,
    #[serde(default, deserialize_with = "double_option")]
    pub communication_disabled_until: Option<Option<Timestamp>>,
    pub flags: Option<u64>,
    pub pending: Option<bool>,
}

/// USER_UPDATE, about the current user. A missing field is unchanged, `Some(None)` is
/// `null`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct UserUpdate {
    pub id: UserId,
    pub username: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub global_name: Option<Option<String>>,
    pub discriminator: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub avatar: Option<Option<String>>,
    pub bot: Option<bool>,
    pub system: Option<bool>,
    pub public_flags: Option<u64>,
    pub premium_type: Option<PremiumType>,
    pub mfa_enabled: Option<bool>,
    pub verified: Option<bool>,
}
