use akari_core::gateway::{
    DecodeError, DispatchEvent, GatewayEvent, GatewayGuild, Hello, Ready, UnavailableGuild, decode,
};
use akari_core::model::{ChannelType, PremiumType, Snowflake, Timestamp};

#[track_caller]
fn decode_ok(json: &str) -> GatewayEvent {
    decode(json.as_bytes()).unwrap_or_else(|err| panic!("failed to decode: {err}"))
}

#[track_caller]
fn ready_from(json: &str) -> (u64, Ready) {
    match decode_ok(json) {
        GatewayEvent::Dispatch {
            seq,
            event: DispatchEvent::Ready(ready),
        } => (seq, *ready),
        other => panic!("expected READY, got {other:?}"),
    }
}

#[test]
fn hello_decodes() {
    assert_eq!(
        decode_ok(include_str!("fixtures/hello.json")),
        GatewayEvent::Hello(Hello {
            heartbeat_interval: 41_250,
            trace: vec![r#"["gateway-prd-us-east1-c-6w69",{"micros":0.0}]"#.to_owned()],
        })
    );
}

#[test]
fn control_opcodes_decode() {
    assert_eq!(
        decode_ok(r#"{"op": 1, "d": null, "s": null, "t": null}"#),
        GatewayEvent::Heartbeat
    );
    assert_eq!(
        decode_ok(r#"{"op": 7, "d": null, "s": null, "t": null}"#),
        GatewayEvent::Reconnect
    );
    assert_eq!(
        decode_ok(r#"{"op": 11, "d": null, "s": null, "t": null}"#),
        GatewayEvent::HeartbeatAck
    );
    assert_eq!(decode_ok(r#"{"op": 11}"#), GatewayEvent::HeartbeatAck);
}

#[test]
fn invalid_session_carries_resumability() {
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": true, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: true }
    );
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": false, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: false }
    );
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": null, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: false }
    );
}

#[test]
fn unknown_opcodes_and_events_are_not_errors() {
    assert_eq!(
        decode_ok(r#"{"op": 99, "d": {"x": 1}, "s": null, "t": null}"#),
        GatewayEvent::Unknown { op: 99 }
    );
    assert_eq!(
        decode_ok(r#"{"op": 0, "d": {"x": 1}, "s": 42, "t": "SOMETHING_NEW"}"#),
        GatewayEvent::Dispatch {
            seq: 42,
            event: DispatchEvent::Other("SOMETHING_NEW".to_owned()),
        }
    );
}

#[test]
fn malformed_payloads_are_errors() {
    assert!(matches!(
        decode(br#"{"op": 0, "d": {}, "t": "READY"}"#),
        Err(DecodeError::MissingField { op: 0, field: "s" })
    ));
    assert!(matches!(
        decode(br#"{"op": 10, "d": null, "s": null, "t": null}"#),
        Err(DecodeError::MissingField { op: 10, field: "d" })
    ));
    assert!(matches!(decode(b"not json"), Err(DecodeError::Json(_))));
    assert!(matches!(
        decode(br#"{"op": 10, "d": {"heartbeat_interval": "soon"}}"#),
        Err(DecodeError::Json(_))
    ));
}

#[test]
fn ready_decodes() {
    let (seq, ready) = ready_from(include_str!("fixtures/ready.json"));

    assert_eq!(seq, 1);
    assert_eq!(ready.v, 9);
    assert_eq!(ready.session_id, "0123456789abcdef0123456789abcdef");
    assert_eq!(
        ready.resume_gateway_url,
        "wss://gateway-us-east1-b.discord.gg"
    );
    assert_eq!(ready.user.user.id, Snowflake(100_000_000_000_000_001));
    assert_eq!(ready.user.premium_type, PremiumType::None);
    let user_ids: Vec<_> = ready.users.iter().map(|user| user.id).collect();
    assert_eq!(
        user_ids,
        [
            Snowflake(100_000_000_000_000_002),
            Snowflake(100_000_000_000_000_003)
        ]
    );
}

#[test]
fn ready_guilds_decode() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    let [
        GatewayGuild::Available(lab),
        GatewayGuild::Unavailable(down),
    ] = &ready.guilds[..]
    else {
        panic!("unexpected guilds: {:?}", ready.guilds);
    };
    assert_eq!(lab.properties.id, Snowflake(200_000_000_000_000_001));
    assert_eq!(lab.properties.name, "Akari \u{2728} Lab");
    assert_eq!(lab.member_count, Some(3));
    assert_eq!(lab.premium_subscription_count, 2);
    assert_eq!(
        lab.joined_at.map(Timestamp::unix_millis),
        Some(1_704_110_400_000)
    );
    let kinds: Vec<_> = lab.channels.iter().map(|channel| channel.kind).collect();
    assert_eq!(
        kinds,
        [
            ChannelType::GuildCategory,
            ChannelType::GuildText,
            ChannelType::GuildVoice
        ]
    );
    assert_eq!(lab.threads[0].kind, ChannelType::PublicThread);
    assert_eq!(lab.roles.len(), 2);
    assert_eq!(
        down,
        &UnavailableGuild {
            id: Snowflake(200_000_000_000_000_002),
            geo_restricted: false,
        }
    );
}

#[test]
fn ready_members_and_private_channels_decode() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    assert_eq!(ready.merged_members.len(), ready.guilds.len());
    let me = &ready.merged_members[0][0];
    assert_eq!(me.user_id, Some(Snowflake(100_000_000_000_000_001)));
    assert_eq!(me.nick.as_deref(), Some("Tester"));
    assert!(ready.merged_members[1].is_empty());

    let [dm, group] = &ready.private_channels[..] else {
        panic!("unexpected private channels: {:?}", ready.private_channels);
    };
    assert_eq!(dm.kind, ChannelType::Dm);
    assert_eq!(dm.recipient_ids, [Snowflake(100_000_000_000_000_002)]);
    assert!(dm.recipients.is_empty());
    assert_eq!(group.kind, ChannelType::GroupDm);
    assert_eq!(group.name.as_deref(), Some("Weekend plans"));
    assert_eq!(group.recipient_ids.len(), 2);
}
