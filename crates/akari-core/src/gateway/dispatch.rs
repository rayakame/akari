use serde::Deserialize;

use crate::lenient::{skip_invalid, skip_invalid_in_each};
use crate::model::{Channel, ChannelId, GuildId, GuildMember, MessageId, Role, RoleId};

/// READY_SUPPLEMENTAL: what READY leaves out, sent right after it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReadySupplemental {
    pub guilds: Vec<SupplementalGuild>,
    /// One list per entry in `guilds`, in the same order: members of voice users, friends
    /// and DM partners. Not the current user's, which READY carries.
    #[serde(default, deserialize_with = "skip_invalid_in_each")]
    pub merged_members: Vec<Vec<GuildMember>>,
    /// DMs left out of READY. Only sent when Identify names a `private_channels_version`.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub lazy_private_channels: Vec<Channel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SupplementalGuild {
    pub id: GuildId,
}

/// GUILD_DELETE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct GuildDelete {
    pub id: GuildId,
    /// `true` for an outage; `false` when the user left or was removed.
    #[serde(default)]
    pub unavailable: bool,
}

/// GUILD_ROLE_CREATE and GUILD_ROLE_UPDATE.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GuildRoleEvent {
    pub guild_id: GuildId,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct GuildRoleDelete {
    pub guild_id: GuildId,
    pub role_id: RoleId,
}

/// CHANNEL_DELETE, which is partial for DMs, and THREAD_DELETE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ChannelDelete {
    pub id: ChannelId,
    pub guild_id: Option<GuildId>,
    pub parent_id: Option<ChannelId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MessageDelete {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MessageDeleteBulk {
    pub ids: Vec<MessageId>,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
}
