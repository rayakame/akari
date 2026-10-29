use serde::Deserialize;

use super::guild::GuildMember;
use super::int_enum::int_enum;
use super::message::Message;
use super::permissions::Permissions;
use super::snowflake::{
    ChannelMarker, EmojiMarker, GenericMarker, GuildMarker, Snowflake, TagMarker, UserMarker,
};
use super::timestamp::Timestamp;
use super::user::User;
use crate::lenient::{default_on_error, skip_invalid, skip_invalid_option};

/// A guild channel, thread, DM or group DM. Which fields are set depends on `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Channel {
    pub id: Snowflake<ChannelMarker>,
    #[serde(rename = "type")]
    pub kind: ChannelType,
    pub guild_id: Option<Snowflake<GuildMarker>>,
    pub position: Option<i32>,
    #[serde(default)]
    pub permission_overwrites: Vec<PermissionOverwrite>,
    pub name: Option<String>,
    pub topic: Option<String>,
    #[serde(default)]
    pub nsfw: bool,
    /// The last message, or the last thread in forum and media channels; may no longer exist.
    pub last_message_id: Option<Snowflake<GenericMarker>>,
    pub last_pin_timestamp: Option<Timestamp>,
    /// Slowmode in seconds.
    pub rate_limit_per_user: Option<u32>,
    /// Minutes of inactivity the official client preselects for a new thread here.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_auto_archive_duration: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    /// DM and group DM recipients, without the current user.
    #[serde(default)]
    pub recipients: Vec<User>,
    /// Replaces `recipients` in deduplicated READY payloads.
    #[serde(default)]
    pub recipient_ids: Vec<Snowflake<UserMarker>>,
    /// Group DM icon hash.
    pub icon: Option<String>,
    /// Owner of a group DM or thread.
    pub owner_id: Option<Snowflake<UserMarker>>,
    /// The category of a guild channel, or the parent channel of a thread.
    pub parent_id: Option<Snowflake<ChannelMarker>>,
    pub thread_metadata: Option<ThreadMetadata>,
    /// The current user's membership, on threads the user joined.
    #[serde(default, deserialize_with = "default_on_error")]
    pub member: Option<ThreadMember>,
    pub message_count: Option<u32>,
    pub member_count: Option<u32>,
    /// Some of a thread's members.
    #[serde(default, deserialize_with = "default_on_error")]
    pub member_ids_preview: Vec<Snowflake<UserMarker>>,
    /// A thread's owner; only thread search sends it.
    #[serde(default, deserialize_with = "default_on_error")]
    pub owner: Option<Box<GuildMember>>,
    #[serde(default)]
    pub is_message_request: bool,
    #[serde(default)]
    pub is_spam: bool,
    #[serde(default)]
    pub flags: u64,
    /// A forum or media channel's tags.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub available_tags: Vec<ForumTag>,
    /// The reaction a forum's post cards offer before they have any.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_reaction_emoji: Option<DefaultReaction>,
    /// 0 latest activity, 1 creation date.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_sort_order: Option<u8>,
    /// 0 not set, 1 list, 2 gallery.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_forum_layout: Option<u8>,
    /// `match_some` or `match_all`.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_tag_setting: Option<String>,
    /// Slowmode in seconds for the messages of a new post or thread.
    #[serde(default, deserialize_with = "default_on_error")]
    pub default_thread_rate_limit_per_user: Option<u32>,
    /// What a new post's message starts with.
    #[serde(default, deserialize_with = "default_on_error")]
    pub template: Option<String>,
    /// A post's tags.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub applied_tags: Vec<Snowflake<TagMarker>>,
    /// Every message ever sent in a thread, deleted ones included.
    #[serde(default, deserialize_with = "default_on_error")]
    pub total_message_sent: Option<u32>,
    /// THREAD_CREATE for a thread just created, not one the user has only just come to see.
    #[serde(default)]
    pub newly_created: bool,
}

/// Post Data's answer: each forum post's first message and owner, by the post's ID.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct PostData {
    #[serde(default, deserialize_with = "default_on_error")]
    pub threads: std::collections::HashMap<Snowflake<ChannelMarker>, PostDataEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct PostDataEntry {
    /// `null` once the first message is deleted.
    #[serde(default, deserialize_with = "default_on_error")]
    pub first_message: Option<Message>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub owner: Option<GuildMember>,
}

/// A forum or media channel's tag.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ForumTag {
    pub id: Snowflake<TagMarker>,
    pub name: String,
    /// Only members who can manage threads apply it.
    #[serde(default)]
    pub moderated: bool,
    #[serde(default, deserialize_with = "default_on_error")]
    pub emoji_id: Option<Snowflake<EmojiMarker>>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub emoji_name: Option<String>,
}

/// A forum's default reaction: a custom emoji's ID or a Unicode emoji.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct DefaultReaction {
    #[serde(default, deserialize_with = "default_on_error")]
    pub emoji_id: Option<Snowflake<EmojiMarker>>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub emoji_name: Option<String>,
}

/// Search Threads' answer. Discord answers 202 without `threads` while it hasn't indexed the
/// channel yet.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ThreadSearchResults {
    #[serde(default, deserialize_with = "skip_invalid_option")]
    pub threads: Option<Vec<Channel>>,
    /// The current user's members of the threads it joined.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub members: Vec<ThreadMember>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub has_more: bool,
    #[serde(default, deserialize_with = "default_on_error")]
    pub total_results: Option<u32>,
    /// The newest message of each thread that has one; undocumented, read by Discord's client.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub most_recent_messages: Vec<Message>,
    /// Forum posts' first messages.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub first_messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PermissionOverwrite {
    /// A role ID or a user ID, depending on `kind`.
    pub id: Snowflake<GenericMarker>,
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(default)]
    pub allow: Permissions,
    #[serde(default)]
    pub deny: Permissions,
}

/// A thread member: the current user's on a thread's `member` and in THREAD_MEMBER_UPDATE,
/// anyone's in THREAD_MEMBERS_UPDATE. Every field falls back to its default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThreadMember {
    /// The thread; some payloads leave it out on a thread's own `member`.
    #[serde(default, deserialize_with = "default_on_error")]
    pub id: Option<Snowflake<ChannelMarker>>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub user_id: Option<Snowflake<UserMarker>>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub join_timestamp: Option<Timestamp>,
    /// `HAS_INTERACTED` 1, `ALL_MESSAGES` 2, `ONLY_MENTIONS` 4, `NO_MESSAGES` 8.
    #[serde(default, deserialize_with = "default_on_error")]
    pub flags: u64,
    #[serde(default, deserialize_with = "default_on_error")]
    pub muted: bool,
    #[serde(default, deserialize_with = "default_on_error")]
    pub mute_config: Option<ThreadMuteConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ThreadMuteConfig {
    /// `None` for a mute without an end.
    #[serde(default, deserialize_with = "default_on_error")]
    pub end_time: Option<Timestamp>,
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
