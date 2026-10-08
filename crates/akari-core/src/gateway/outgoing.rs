use std::collections::BTreeMap;

use serde::Serialize;

use crate::Token;
use crate::properties::ClientProperties;

/// A command for [`Gateway::send`](super::Gateway::send).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GatewayCommand {
    /// Sets this session's status, without activities. Discord allows 5 updates per 20 s.
    UpdatePresence { status: PresenceStatus },
}

/// A status the user can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Online,
    Idle,
    #[serde(rename = "dnd")]
    DoNotDisturb,
    /// Shown to others as offline.
    Invisible,
}

impl GatewayCommand {
    pub(crate) fn to_payload(&self) -> String {
        match *self {
            Self::UpdatePresence { status } => json(3, Presence::new(status)),
        }
    }
}

const LAZY_USER_NOTES: u64 = 1 << 0;
const VERSIONED_READ_STATES: u64 = 1 << 2;
const VERSIONED_USER_GUILD_SETTINGS: u64 = 1 << 3;
const DEDUPE_USER_OBJECTS: u64 = 1 << 4;
const PRIORITIZED_READY_PAYLOAD: u64 = 1 << 5;
const USER_SETTINGS_PROTO: u64 = 1 << 9;
const CLIENT_STATE_V2: u64 = 1 << 10;

// The shape READY's models expect; see docs/protocol/ready.md.
pub(crate) const CAPABILITIES: u64 = LAZY_USER_NOTES
    | VERSIONED_READ_STATES
    | VERSIONED_USER_GUILD_SETTINGS
    | DEDUPE_USER_OBJECTS
    | PRIORITIZED_READY_PAYLOAD
    | USER_SETTINGS_PROTO
    | CLIENT_STATE_V2;

#[derive(Serialize)]
struct Payload<D> {
    op: u8,
    d: D,
}

#[derive(Serialize)]
struct Identify<'a> {
    token: &'a str,
    capabilities: u64,
    properties: &'a ClientProperties,
    presence: Presence<&'static str>,
    compress: bool,
    client_state: ClientState,
}

#[derive(Serialize)]
struct Presence<S> {
    status: S,
    since: u64,
    activities: [(); 0],
    afk: bool,
}

impl<S> Presence<S> {
    fn new(status: S) -> Self {
        Self {
            status,
            since: 0,
            activities: [],
            afk: false,
        }
    }
}

#[derive(Serialize)]
struct ClientState {
    guild_versions: BTreeMap<String, u64>,
    api_code_version: u8,
}

#[derive(Serialize)]
struct Resume<'a> {
    token: &'a str,
    session_id: &'a str,
    seq: u64,
}

fn json<D: Serialize>(op: u8, d: D) -> String {
    // Serializing strings, numbers and maps can't fail.
    serde_json::to_string(&Payload { op, d }).unwrap_or_default()
}

pub(crate) fn identify(token: &Token, properties: &ClientProperties) -> String {
    json(
        2,
        Identify {
            token: token.expose(),
            capabilities: CAPABILITIES,
            properties,
            // "unknown" lets the gateway assign the initial status.
            presence: Presence::new("unknown"),
            compress: false,
            client_state: ClientState {
                guild_versions: BTreeMap::new(),
                api_code_version: 0,
            },
        },
    )
}

pub(crate) fn resume(token: &Token, session_id: &str, seq: u64) -> String {
    json(
        6,
        Resume {
            token: token.expose(),
            session_id,
            seq,
        },
    )
}

pub(crate) fn heartbeat(seq: Option<u64>) -> String {
    json(1, seq)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::properties::{Arch, ClientBuild, DesktopOs, HostInfo};

    fn properties() -> ClientProperties {
        let host = HostInfo {
            os: DesktopOs::Linux,
            os_version: "6.8.0".to_owned(),
            arch: Arch::X64,
            system_locale: "en-US".to_owned(),
        };
        ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::Linux))
    }

    fn parse(payload: &str) -> Value {
        serde_json::from_str(payload).unwrap()
    }

    #[test]
    fn identify_presents_the_client_properties() {
        let properties = properties();

        let identify = parse(&identify(&Token::new("t0k".to_owned()), &properties));

        assert_eq!(identify["op"], 2);
        assert_eq!(identify["d"]["token"], "t0k");
        assert_eq!(identify["d"]["capabilities"], 1597);
        assert_eq!(
            identify["d"]["properties"],
            serde_json::to_value(&properties).unwrap()
        );
        assert_eq!(
            identify["d"]["presence"],
            json!({"status": "unknown", "since": 0, "activities": [], "afk": false})
        );
        assert_eq!(identify["d"]["compress"], false);
        assert_eq!(
            identify["d"]["client_state"],
            json!({"guild_versions": {}, "api_code_version": 0})
        );
    }

    #[test]
    fn capabilities_are_the_ones_ready_md_lists() {
        assert_eq!(CAPABILITIES, 1597);
        assert_eq!(
            CAPABILITIES & (1 << 8),
            0,
            "AUTH_TOKEN_REFRESH must stay off"
        );
    }

    #[test]
    fn resume_and_heartbeat_payloads() {
        assert_eq!(
            parse(&resume(&Token::new("t0k".to_owned()), "abc", 42)),
            json!({"op": 6, "d": {"token": "t0k", "session_id": "abc", "seq": 42}})
        );
        assert_eq!(parse(&heartbeat(None)), json!({"op": 1, "d": null}));
        assert_eq!(parse(&heartbeat(Some(7))), json!({"op": 1, "d": 7}));
    }

    #[test]
    fn presence_updates_carry_only_the_status() {
        for (status, name) in [
            (PresenceStatus::Online, "online"),
            (PresenceStatus::Idle, "idle"),
            (PresenceStatus::DoNotDisturb, "dnd"),
            (PresenceStatus::Invisible, "invisible"),
        ] {
            let command = GatewayCommand::UpdatePresence { status };

            assert_eq!(
                parse(&command.to_payload()),
                json!({"op": 3, "d": {"since": 0, "activities": [], "status": name, "afk": false}})
            );
        }
    }
}
