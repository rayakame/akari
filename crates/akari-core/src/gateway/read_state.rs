use serde::Deserialize;

use crate::lenient::skip_invalid;
use crate::model::{ChannelId, GenericMarker, GuildId, Snowflake};

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
}
