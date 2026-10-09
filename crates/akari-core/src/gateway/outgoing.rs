use std::collections::BTreeMap;

use serde::Serialize;

use crate::Token;
use crate::model::{ChannelId, GuildId};
use crate::properties::ClientProperties;

/// A command for [`Gateway::send`](super::Gateway::send).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GatewayCommand {
    /// Sets this session's status, without activities. Discord allows 5 updates per 20 s.
    UpdatePresence { status: PresenceStatus },
    /// Op 37, as the official client sends it when a channel is opened: typing, activities
    /// and threads, and optionally member lists. Without it, large guilds send no live
    /// messages.
    SubscribeGuilds { guilds: Vec<GuildSubscription> },
}

/// One guild's entry in [`GatewayCommand::SubscribeGuilds`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GuildSubscription {
    pub guild_id: GuildId,
    /// `Some` also subscribes these member lists and drops all others in the guild. In very
    /// large guilds, live messages only come for channels whose member list is subscribed.
    pub member_lists: Option<MemberLists>,
}

impl GuildSubscription {
    pub fn new(guild_id: GuildId) -> Self {
        Self {
            guild_id,
            member_lists: None,
        }
    }
}

/// Member lists to subscribe: the first 100 entries of each channel's list, as the official
/// client loads them, and each thread's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemberLists {
    pub channels: Vec<ChannelId>,
    pub threads: Vec<ChannelId>,
}

impl MemberLists {
    pub fn new(channels: Vec<ChannelId>, threads: Vec<ChannelId>) -> Self {
        Self { channels, threads }
    }
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
        match self {
            Self::UpdatePresence { status } => json(3, Presence::new(*status)),
            Self::SubscribeGuilds { guilds } => {
                let payload = json(
                    37,
                    Subscriptions {
                        subscriptions: guilds
                            .iter()
                            .map(|guild| (guild.guild_id.get().to_string(), Entry::new(guild)))
                            .collect(),
                    },
                );
                tracing::debug!(target: "akari_core::subscriptions", %payload, "sending op 37");
                payload
            }
        }
    }

    pub(crate) fn subscribe_guilds(guilds: &[GuildSubscription]) -> Vec<Self> {
        let mut commands = Vec::new();
        let mut batch = Vec::new();
        let mut size = EMPTY_SUBSCRIPTIONS;
        for guild in guilds {
            let entry = entry_size(guild);
            if size + entry > MAX_PAYLOAD && !batch.is_empty() {
                commands.push(Self::SubscribeGuilds {
                    guilds: std::mem::take(&mut batch),
                });
                size = EMPTY_SUBSCRIPTIONS;
            }
            batch.push(guild.clone());
            size += entry;
        }
        if !batch.is_empty() {
            commands.push(Self::SubscribeGuilds { guilds: batch });
        }
        commands
    }
}

// `{"op":37,"d":{"subscriptions":{}}}`
const EMPTY_SUBSCRIPTIONS: usize = 34;

// `"<guild id>":<entry>,`
fn entry_size(guild: &GuildSubscription) -> usize {
    let entry = serde_json::to_string(&Entry::new(guild)).map_or(0, |entry| entry.len());
    guild.guild_id.get().to_string().len() + entry + 4
}

#[derive(Serialize)]
struct Subscriptions {
    subscriptions: BTreeMap<String, Entry>,
}

#[derive(Serialize)]
struct Entry {
    typing: bool,
    activities: bool,
    threads: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<BTreeMap<String, [[u32; 2]; 1]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_member_lists: Option<Vec<String>>,
}

impl Entry {
    fn new(guild: &GuildSubscription) -> Self {
        let lists = guild.member_lists.as_ref();
        Self {
            typing: true,
            activities: true,
            threads: true,
            channels: lists.map(|lists| {
                lists
                    .channels
                    .iter()
                    .map(|channel| (channel.get().to_string(), FIRST_HUNDRED))
                    .collect()
            }),
            thread_member_lists: lists.map(|lists| {
                lists
                    .threads
                    .iter()
                    .map(|thread| thread.get().to_string())
                    .collect()
            }),
        }
    }
}

const FIRST_HUNDRED: [[u32; 2]; 1] = [[0, 99]];

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

// Discord closes the connection with 4002 for anything larger.
pub(crate) const MAX_PAYLOAD: usize = 15 * 1024;

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
    use crate::model::Snowflake;
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

    #[test]
    fn a_guild_subscription_matches_the_official_clients_shape() {
        let command = GatewayCommand::SubscribeGuilds {
            guilds: vec![GuildSubscription::new(Snowflake::new(
                200_000_000_000_000_001,
            ))],
        };

        let payload: Value = serde_json::from_str(&command.to_payload()).unwrap();

        assert_eq!(
            payload,
            json!({"op": 37, "d": {"subscriptions": {"200000000000000001": {
                "typing": true, "activities": true, "threads": true
            }}}})
        );
    }

    #[test]
    fn member_lists_ask_for_the_first_hundred_of_each_channel() {
        let mut lists = GuildSubscription::new(Snowflake::new(200_000_000_000_000_001));
        lists.member_lists = Some(MemberLists::new(
            vec![
                Snowflake::new(300_000_000_000_000_002),
                Snowflake::new(300_000_000_000_000_003),
            ],
            vec![Snowflake::new(300_000_000_000_000_020)],
        ));
        let mut cleared = GuildSubscription::new(Snowflake::new(200_000_000_000_000_002));
        cleared.member_lists = Some(MemberLists::default());
        let command = GatewayCommand::SubscribeGuilds {
            guilds: vec![lists, cleared],
        };

        let payload: Value = serde_json::from_str(&command.to_payload()).unwrap();

        assert_eq!(
            payload,
            json!({"op": 37, "d": {"subscriptions": {
                "200000000000000001": {
                    "typing": true, "activities": true, "threads": true,
                    "channels": {
                        "300000000000000002": [[0, 99]],
                        "300000000000000003": [[0, 99]]
                    },
                    "thread_member_lists": ["300000000000000020"]
                },
                "200000000000000002": {
                    "typing": true, "activities": true, "threads": true,
                    "channels": {},
                    "thread_member_lists": []
                }
            }}})
        );
    }

    #[test]
    fn guild_subscriptions_split_below_the_payload_limit() {
        let guilds: Vec<_> = (0..1000)
            .map(|index| {
                let mut guild =
                    GuildSubscription::new(Snowflake::new(200_000_000_000_000_000 + index));
                if index % 2 == 0 {
                    guild.member_lists = Some(MemberLists::new(
                        (0..10)
                            .map(|channel| Snowflake::new(300_000_000_000_000_000 + channel))
                            .collect(),
                        vec![Snowflake::new(300_000_000_000_000_100)],
                    ));
                }
                guild
            })
            .collect();

        let commands = GatewayCommand::subscribe_guilds(&guilds);

        assert!(commands.len() > 1);
        let mut seen = Vec::new();
        for command in &commands {
            let payload = command.to_payload();
            assert!(payload.len() <= MAX_PAYLOAD, "{} bytes", payload.len());
            let payload: Value = serde_json::from_str(&payload).unwrap();
            seen.extend(
                payload["d"]["subscriptions"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(|id| id.parse::<u64>().unwrap()),
            );
        }
        seen.sort_unstable();
        assert_eq!(
            seen,
            guilds
                .iter()
                .map(|guild| guild.guild_id.get())
                .collect::<Vec<_>>()
        );
        assert!(GatewayCommand::subscribe_guilds(&[]).is_empty());
    }
}
