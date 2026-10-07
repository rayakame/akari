use serde::Deserialize;

use super::int_enum::int_enum;
use super::permissions::Permissions;
use super::snowflake::Snowflake;
use super::timestamp::Timestamp;
use super::user::User;

/// A guild channel, thread, DM or group DM. Which fields are set depends on `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Channel {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: ChannelType,
    pub guild_id: Option<Snowflake>,
    pub position: Option<i32>,
    #[serde(default)]
    pub permission_overwrites: Vec<PermissionOverwrite>,
    pub name: Option<String>,
    pub topic: Option<String>,
    #[serde(default)]
    pub nsfw: bool,
    /// May point to a message that no longer exists.
    pub last_message_id: Option<Snowflake>,
    pub last_pin_timestamp: Option<Timestamp>,
    /// Slowmode in seconds.
    pub rate_limit_per_user: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    /// DM and group DM recipients, without the current user.
    #[serde(default)]
    pub recipients: Vec<User>,
    /// Replaces `recipients` in deduplicated READY payloads.
    #[serde(default)]
    pub recipient_ids: Vec<Snowflake>,
    /// Group DM icon hash.
    pub icon: Option<String>,
    /// Owner of a group DM or thread.
    pub owner_id: Option<Snowflake>,
    /// The category of a guild channel, or the parent channel of a thread.
    pub parent_id: Option<Snowflake>,
    pub thread_metadata: Option<ThreadMetadata>,
    pub message_count: Option<u32>,
    pub member_count: Option<u32>,
    #[serde(default)]
    pub is_message_request: bool,
    #[serde(default)]
    pub is_spam: bool,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PermissionOverwrite {
    /// A role ID or a user ID, depending on `kind`.
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(default)]
    pub allow: Permissions,
    #[serde(default)]
    pub deny: Permissions,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadMetadata {
    pub archived: bool,
    /// Minutes of inactivity before the thread is hidden.
    pub auto_archive_duration: u32,
    pub archive_timestamp: Timestamp,
    pub locked: bool,
    /// Only set on private threads.
    pub invitable: Option<bool>,
    /// Missing for threads created before 2022-01-09.
    pub create_timestamp: Option<Timestamp>,
}

int_enum! {
    /// The kind of channel.
    pub enum ChannelType {
        GuildText = 0,
        Dm = 1,
        GuildVoice = 2,
        GroupDm = 3,
        GuildCategory = 4,
        /// An announcement channel.
        GuildNews = 5,
        GuildStore = 6,
        NewsThread = 10,
        PublicThread = 11,
        PrivateThread = 12,
        GuildStageVoice = 13,
        GuildDirectory = 14,
        GuildForum = 15,
        GuildMedia = 16,
        Lobby = 17,
        EphemeralDm = 18,
    }
}

int_enum! {
    /// Whether a permission overwrite applies to a role or a member.
    pub enum OverwriteType {
        Role = 0,
        Member = 1,
    }
}
