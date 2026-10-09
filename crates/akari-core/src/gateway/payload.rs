use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

use super::dispatch::{
    ChannelDelete, GuildDelete, GuildRoleDelete, GuildRoleEvent, MessageDelete, MessageDeleteBulk,
    ReadySupplemental,
};
use super::guild::GatewayGuild;
use super::hello::Hello;
use super::partial::{ChannelUpdate, GuildMemberUpdate, GuildUpdate, MessageUpdate, UserUpdate};
use super::ready::Ready;
use crate::JsonError;
use crate::model::{Channel, GenericMarker, Message, Snowflake};

/// A message received from the gateway.
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayEvent {
    /// Opcode 0. Keep `seq` for heartbeats and resuming.
    Dispatch { seq: u64, event: DispatchEvent },
    /// Opcode 1: send a heartbeat now.
    Heartbeat,
    /// Opcode 7: reconnect and resume.
    Reconnect,
    /// Opcode 9.
    InvalidSession { resumable: bool },
    /// Opcode 10.
    Hello(Hello),
    /// Opcode 11.
    HeartbeatAck,
    /// An opcode Akari doesn't handle.
    Unknown { op: u16 },
}

/// The event inside a dispatch.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DispatchEvent {
    Ready(Box<Ready>),
    ReadySupplemental(Box<ReadySupplemental>),
    /// The replay after a resume is complete.
    Resumed,
    /// The user joined a guild, or it is available again.
    GuildCreate(Box<GatewayGuild>),
    GuildUpdate(Box<GuildUpdate>),
    GuildDelete(GuildDelete),
    GuildRoleCreate(Box<GuildRoleEvent>),
    GuildRoleUpdate(Box<GuildRoleEvent>),
    GuildRoleDelete(GuildRoleDelete),
    GuildMemberUpdate(Box<GuildMemberUpdate>),
    ChannelCreate(Box<Channel>),
    ChannelUpdate(Box<ChannelUpdate>),
    ChannelDelete(ChannelDelete),
    ThreadCreate(Box<Channel>),
    ThreadUpdate(Box<ChannelUpdate>),
    ThreadDelete(ChannelDelete),
    MessageCreate(Box<Message>),
    MessageUpdate(Box<MessageUpdate>),
    MessageDelete(MessageDelete),
    MessageDeleteBulk(MessageDeleteBulk),
    UserUpdate(Box<UserUpdate>),
    /// An event Akari doesn't parse yet, by name.
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid gateway payload")]
    Json(#[source] JsonError),
    #[error("op {op} payload is missing `{field}`")]
    MissingField { op: u16, field: &'static str },
    /// A dispatch whose data didn't decode. `seq` still counts as received.
    #[error("invalid {event} dispatch")]
    Dispatch {
        seq: u64,
        event: String,
        #[source]
        source: JsonError,
    },
}

impl From<serde_json::Error> for DecodeError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err.into())
    }
}

impl DecodeError {
    pub(crate) fn dispatch(&self) -> Option<(u64, &str)> {
        match self {
            Self::Dispatch { seq, event, .. } => Some((*seq, event)),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct RawPayload<'a> {
    op: u16,
    #[serde(borrow)]
    d: Option<&'a RawValue>,
    s: Option<u64>,
    t: Option<String>,
}

/// Decodes one gateway message. `input` is JSON, already decompressed.
pub fn decode(input: &[u8]) -> Result<GatewayEvent, DecodeError> {
    let RawPayload { op, d, s, t } = serde_json::from_slice(input)?;
    let missing = |field| DecodeError::MissingField { op, field };
    Ok(match op {
        0 => {
            let seq = s.ok_or_else(|| missing("s"))?;
            let name = t.ok_or_else(|| missing("t"))?;
            let data = d.ok_or_else(|| missing("d"))?;
            GatewayEvent::Dispatch {
                seq,
                event: decode_dispatch(seq, name, data)?,
            }
        }
        1 => GatewayEvent::Heartbeat,
        7 => GatewayEvent::Reconnect,
        9 => GatewayEvent::InvalidSession {
            resumable: match d {
                Some(data) => serde_json::from_str(data.get())?,
                None => false,
            },
        },
        10 => GatewayEvent::Hello(serde_json::from_str(d.ok_or_else(|| missing("d"))?.get())?),
        11 => GatewayEvent::HeartbeatAck,
        op => GatewayEvent::Unknown { op },
    })
}

fn decode_dispatch(seq: u64, name: String, data: &RawValue) -> Result<DispatchEvent, DecodeError> {
    use DispatchEvent as E;

    let json = data.get();
    // Names and guild IDs only: payloads hold message content and personal data.
    if tracing::enabled!(target: "akari_core::dispatches", tracing::Level::DEBUG) {
        tracing::debug!(
            target: "akari_core::dispatches",
            event = %name,
            seq,
            guild_id = guild_of(&name, json),
            "dispatch"
        );
    }
    let event = match name.as_str() {
        "READY" => parse(json).map(|ready| E::Ready(Box::new(ready))),
        "READY_SUPPLEMENTAL" => parse(json).map(|data| E::ReadySupplemental(Box::new(data))),
        "RESUMED" => Ok(E::Resumed),
        "GUILD_CREATE" => parse(json).map(|guild| E::GuildCreate(Box::new(guild))),
        "GUILD_UPDATE" => parse(json).map(|update| E::GuildUpdate(Box::new(update))),
        "GUILD_DELETE" => parse(json).map(E::GuildDelete),
        "GUILD_ROLE_CREATE" => parse(json).map(|role| E::GuildRoleCreate(Box::new(role))),
        "GUILD_ROLE_UPDATE" => parse(json).map(|role| E::GuildRoleUpdate(Box::new(role))),
        "GUILD_ROLE_DELETE" => parse(json).map(E::GuildRoleDelete),
        "GUILD_MEMBER_UPDATE" => parse(json).map(|update| E::GuildMemberUpdate(Box::new(update))),
        "CHANNEL_CREATE" => parse(json).map(|channel| E::ChannelCreate(Box::new(channel))),
        "CHANNEL_UPDATE" => parse(json).map(|update| E::ChannelUpdate(Box::new(update))),
        "CHANNEL_DELETE" => parse(json).map(E::ChannelDelete),
        "THREAD_CREATE" => parse(json).map(|thread| E::ThreadCreate(Box::new(thread))),
        "THREAD_UPDATE" => parse(json).map(|update| E::ThreadUpdate(Box::new(update))),
        "THREAD_DELETE" => parse(json).map(E::ThreadDelete),
        "MESSAGE_CREATE" => parse(json).map(|message| E::MessageCreate(Box::new(message))),
        "MESSAGE_UPDATE" => parse(json).map(|update| E::MessageUpdate(Box::new(update))),
        "MESSAGE_DELETE" => parse(json).map(E::MessageDelete),
        "MESSAGE_DELETE_BULK" => parse(json).map(E::MessageDeleteBulk),
        "USER_UPDATE" => parse(json).map(|update| E::UserUpdate(Box::new(update))),
        _ => return Ok(E::Other(name)),
    };
    event.map_err(|source| DecodeError::Dispatch {
        seq,
        event: name,
        source: source.into(),
    })
}

// Parsed only for the dispatch log, so it costs nothing otherwise.
fn guild_of(name: &str, json: &str) -> Option<u64> {
    #[derive(Deserialize)]
    struct Guild {
        guild_id: Option<Snowflake<GenericMarker>>,
    }
    #[derive(Deserialize)]
    struct Own {
        id: Option<Snowflake<GenericMarker>>,
    }
    let id = match name {
        "GUILD_CREATE" | "GUILD_UPDATE" | "GUILD_DELETE" => parse::<Own>(json).ok()?.id,
        _ => parse::<Guild>(json).ok()?.guild_id,
    };
    id.map(Snowflake::get)
}

fn parse<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(json)
}
