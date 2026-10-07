use serde::Deserialize;

use super::guild::GatewayGuild;
use super::lenient::{skip_invalid, skip_invalid_in_each};
use crate::model::{Channel, CurrentUser, GuildMember, User};

/// The READY dispatch, in the shape described in `docs/protocol/ready.md`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ready {
    /// API version.
    pub v: u8,
    pub user: CurrentUser,
    /// Every other user the payload refers to by ID.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub users: Vec<User>,
    pub guilds: Vec<GatewayGuild>,
    /// One list per entry in `guilds`, in the same order. READY only carries the current
    /// user's member; READY_SUPPLEMENTAL brings the rest.
    #[serde(default, deserialize_with = "skip_invalid_in_each")]
    pub merged_members: Vec<Vec<GuildMember>>,
    /// DMs and group DMs, with `recipient_ids` instead of `recipients`.
    #[serde(default, deserialize_with = "skip_invalid")]
    pub private_channels: Vec<Channel>,
    pub session_id: String,
    pub resume_gateway_url: String,
}
