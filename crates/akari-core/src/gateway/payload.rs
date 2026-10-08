use serde::Deserialize;
use serde_json::value::RawValue;

use super::hello::Hello;
use super::ready::Ready;

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
pub enum DispatchEvent {
    Ready(Box<Ready>),
    /// The replay after a resume is complete.
    Resumed,
    /// An event Akari doesn't parse yet, by name.
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid gateway payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("op {op} payload is missing `{field}`")]
    MissingField { op: u16, field: &'static str },
    /// A dispatch whose data didn't decode. `seq` still counts as received.
    #[error("invalid {event} dispatch")]
    Dispatch {
        seq: u64,
        event: String,
        #[source]
        source: serde_json::Error,
    },
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
    match name.as_str() {
        "READY" => match serde_json::from_str(data.get()) {
            Ok(ready) => Ok(DispatchEvent::Ready(Box::new(ready))),
            Err(source) => Err(DecodeError::Dispatch {
                seq,
                event: name,
                source,
            }),
        },
        "RESUMED" => Ok(DispatchEvent::Resumed),
        _ => Ok(DispatchEvent::Other(name)),
    }
}
