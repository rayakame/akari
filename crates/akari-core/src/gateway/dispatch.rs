use serde::Deserialize;

use crate::lenient::{skip_invalid, skip_invalid_in_each};
use crate::model::{
    Channel, ChannelId, GuildId, GuildMember, MessageId, Role, RoleId, ThreadMember, UserId,
};

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

/// THREAD_MEMBER_UPDATE: the current user's member of a thread changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMemberUpdate {
    pub guild_id: Option<GuildId>,
    pub member: ThreadMember,
}

/// THREAD_MEMBERS_UPDATE: members were added to or removed from a thread.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadMembersUpdate {
    /// The thread.
    pub id: ChannelId,
    pub guild_id: Option<GuildId>,
    pub member_count: Option<u32>,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub added_members: Vec<ThreadMember>,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub removed_member_ids: Vec<UserId>,
}

/// THREAD_LIST_SYNC: the active threads of some of a guild's channels, sent when the user
/// subscribes to them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadListSync {
    pub guild_id: Option<GuildId>,
    /// The current user's members of the synced threads it joined.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub members: Vec<ThreadMember>,
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
