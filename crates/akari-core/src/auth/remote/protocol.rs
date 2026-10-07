use serde::{Deserialize, Deserializer, Serialize};

use super::ScannedUser;
use crate::Secret;
use crate::model::Snowflake;

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(super) enum ServerPacket {
    Hello {
        heartbeat_interval: u64,
    },
    NonceProof {
        encrypted_nonce: String,
    },
    PendingRemoteInit {
        fingerprint: String,
    },
    PendingTicket {
        encrypted_user_payload: String,
    },
    PendingLogin {
        #[serde(deserialize_with = "secret")]
        ticket: Secret,
    },
    Cancel,
    HeartbeatAck,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(super) enum ClientPacket<'a> {
    Init { encoded_public_key: &'a str },
    NonceProof { nonce: &'a str },
    Heartbeat,
}

fn secret<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Secret, D::Error> {
    String::deserialize(deserializer).map(Secret::new)
}

// `id:discriminator:avatar:username`; the username comes last, so a colon in it survives.
pub(super) fn parse_user(payload: &str) -> Option<ScannedUser> {
    let mut parts = payload.splitn(4, ':');
    let id = parts.next()?.parse().ok()?;
    let discriminator = parts.next()?.to_owned();
    let avatar = parts.next()?;
    let username = parts.next()?.to_owned();
    Some(ScannedUser {
        id: Snowflake::new(id),
        discriminator,
        avatar: (avatar != "0").then(|| avatar.to_owned()),
        username,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::Snowflake;

    fn server(value: serde_json::Value) -> ServerPacket {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn server_packets_decode_by_op() {
        assert_eq!(
            server(json!({"op": "hello", "heartbeat_interval": 41250, "timeout_ms": 142637})),
            ServerPacket::Hello {
                heartbeat_interval: 41250
            }
        );
        assert_eq!(
            server(json!({"op": "nonce_proof", "encrypted_nonce": "abc"})),
            ServerPacket::NonceProof {
                encrypted_nonce: "abc".to_owned()
            }
        );
        assert_eq!(
            server(json!({"op": "pending_remote_init", "fingerprint": "fp"})),
            ServerPacket::PendingRemoteInit {
                fingerprint: "fp".to_owned()
            }
        );
        assert_eq!(
            server(json!({"op": "pending_ticket", "encrypted_user_payload": "u"})),
            ServerPacket::PendingTicket {
                encrypted_user_payload: "u".to_owned()
            }
        );
        assert!(matches!(
            server(json!({"op": "pending_login", "ticket": "t"})),
            ServerPacket::PendingLogin { ticket } if ticket.expose() == "t"
        ));
        assert_eq!(server(json!({"op": "cancel"})), ServerPacket::Cancel);
        assert_eq!(
            server(json!({"op": "heartbeat_ack"})),
            ServerPacket::HeartbeatAck
        );
        assert_eq!(
            server(json!({"op": "pending_finish", "encrypted_token": "x"})),
            ServerPacket::Unknown
        );
    }

    #[test]
    fn client_packets_encode_flat_with_op() {
        let encode = |packet: ClientPacket<'_>| serde_json::to_value(packet).unwrap();

        assert_eq!(
            encode(ClientPacket::Init {
                encoded_public_key: "key"
            }),
            json!({"op": "init", "encoded_public_key": "key"})
        );
        assert_eq!(
            encode(ClientPacket::NonceProof { nonce: "n" }),
            json!({"op": "nonce_proof", "nonce": "n"})
        );
        assert_eq!(encode(ClientPacket::Heartbeat), json!({"op": "heartbeat"}));
    }

    #[test]
    fn user_payload_has_four_fields() {
        let user = parse_user("852892297661906993:0:05145cc5646fbcba277b6d5ea2030610:dolfies");

        assert_eq!(
            user,
            Some(ScannedUser {
                id: Snowflake::new(852_892_297_661_906_993),
                discriminator: "0".to_owned(),
                avatar: Some("05145cc5646fbcba277b6d5ea2030610".to_owned()),
                username: "dolfies".to_owned(),
            })
        );
    }

    #[test]
    fn avatar_zero_means_none_and_colons_stay_in_the_username() {
        let user = parse_user("1:1234:0:weird:name").unwrap();

        assert_eq!(user.avatar, None);
        assert_eq!(user.discriminator, "1234");
        assert_eq!(user.username, "weird:name");
    }

    #[test]
    fn malformed_payloads_are_rejected() {
        assert_eq!(parse_user("1:0:avatar"), None);
        assert_eq!(parse_user("not-an-id:0:0:name"), None);
        assert_eq!(parse_user(""), None);
    }
}
