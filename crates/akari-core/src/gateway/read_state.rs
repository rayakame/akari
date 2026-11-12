use serde::Deserialize;

use crate::lenient::{default_on_error, skip_invalid};
use crate::model::{ChannelId, GenericMarker, GuildId, Snowflake, Timestamp};

/// An entry of READY's `read_state`: where the user stopped reading a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ReadStateEntry {
    /// The channel; for other types the guild or user they belong to.
    pub id: Snowflake<GenericMarker>,
    /// 0 for channels. The others (guild events, the notification center, …) aren't channels.
    #[serde(default)]
    pub read_state_type: u8,
    /// The last acknowledged message; it may no longer exist.
    pub last_message_id: Option<Snowflake<GenericMarker>>,
    #[serde(default)]
    pub mention_count: u32,
    /// `IS_GUILD_CHANNEL` 1 << 0, `IS_THREAD` 1 << 1, `IS_MENTION_LOW_IMPORTANCE` 1 << 2.
    #[serde(default)]
    pub flags: u64,
    /// The pin time the user acknowledged; Discord's epoch stands for never.
    #[serde(default, deserialize_with = "default_on_error")]
    pub last_pin_timestamp: Option<Timestamp>,
}

/// MESSAGE_ACK: the user read a channel up to a message, in this session or another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MessageAck {
    /// The read state type; 0 for channels.
    #[serde(default)]
    pub ack_type: u8,
    pub channel_id: ChannelId,
    /// It may no longer exist; in forum and media channels it is a thread.
    pub message_id: Snowflake<GenericMarker>,
    /// The position was set on purpose, also backwards (mark as unread).
    #[serde(default)]
    pub manual: bool,
    /// `None` keeps the channel's count.
    pub mention_count: Option<u32>,
    #[serde(default)]
    pub version: u64,
}

/// CHANNEL_PINS_UPDATE: a message was pinned or unpinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ChannelPinsUpdate {
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    /// The newest pin's time; `None` when Discord sent `null` or something else.
    #[serde(default, deserialize_with = "default_on_error")]
    pub last_pin_timestamp: Option<Timestamp>,
}

/// CHANNEL_PINS_ACK: the user saw a channel's pins, in this session or another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ChannelPinsAck {
    pub channel_id: ChannelId,
    /// The pin time acknowledged.
    #[serde(default, deserialize_with = "default_on_error")]
    pub timestamp: Option<Timestamp>,
    #[serde(default, deserialize_with = "default_on_error")]
    pub version: u64,
}

/// CHANNEL_UNREAD_UPDATE: the newest messages in a guild the session isn't subscribed to,
/// now only for directory channels (`PASSIVE_GUILD_UPDATE_V2` replaces it).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChannelUnreadUpdate {
    pub guild_id: GuildId,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub channel_unread_updates: Vec<ChannelUnread>,
}

/// PASSIVE_UPDATE_V2: the channels whose newest message changed in a guild the session
/// isn't subscribed to. Its voice states and members aren't modeled.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PassiveUpdateV2 {
    pub guild_id: GuildId,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub updated_channels: Vec<ChannelUnread>,
}

/// A channel's newest message, sent for guilds the session isn't subscribed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ChannelUnread {
    pub id: ChannelId,
    /// The last message, or the last thread in forum and media channels; may no longer exist.
    pub last_message_id: Option<Snowflake<GenericMarker>>,
    /// Only PASSIVE_UPDATE_V2 sends it.
    #[serde(default, deserialize_with = "default_on_error")]
    pub last_pin_timestamp: Option<Timestamp>,
}
