use akari_core::gateway::{
    DecodeError, DispatchEvent, GatewayEvent, GatewayGuild, Hello, Ready, UnavailableGuild, decode,
};
use akari_core::model::{
    ChannelType, MessageNotificationLevel, NsfwLevel, Permissions, PremiumTier, PremiumType,
    Snowflake, Timestamp,
};
use serde_json::Value;

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

#[track_caller]
fn ready_with(edit: impl FnOnce(&mut Value)) -> Ready {
    let mut payload: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("fixture is not JSON: {err}"));
    edit(&mut payload);
    ready_from(&payload.to_string()).1
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
fn resumed_decodes() {
    assert_eq!(
        decode_ok(r#"{"op": 0, "s": 9, "t": "RESUMED", "d": {"_trace": []}}"#),
        GatewayEvent::Dispatch {
            seq: 9,
            event: DispatchEvent::Resumed,
        }
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
    assert_eq!(ready.user.user.id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(ready.user.premium_type, PremiumType::None);
    let user_ids: Vec<_> = ready.users.iter().map(|user| user.id).collect();
    assert_eq!(
        user_ids,
        [
            Snowflake::new(100_000_000_000_000_002),
            Snowflake::new(100_000_000_000_000_003)
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
    assert_eq!(lab.properties.id, Snowflake::new(200_000_000_000_000_001));
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
            id: Snowflake::new(200_000_000_000_000_002),
            geo_restricted: false,
        }
    );
}

#[test]
fn ready_members_and_private_channels_decode() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    assert_eq!(ready.merged_members.len(), ready.guilds.len());
    let me = &ready.merged_members[0][0];
    assert_eq!(me.user_id, Some(Snowflake::new(100_000_000_000_000_001)));
    assert_eq!(me.nick.as_deref(), Some("Tester"));
    assert!(ready.merged_members[1].is_empty());

    let [dm, group] = &ready.private_channels[..] else {
        panic!("unexpected private channels: {:?}", ready.private_channels);
    };
    assert_eq!(dm.kind, ChannelType::Dm);
    assert_eq!(dm.recipient_ids, [Snowflake::new(100_000_000_000_000_002)]);
    assert!(dm.recipients.is_empty());
    assert_eq!(group.kind, ChannelType::GroupDm);
    assert_eq!(group.name.as_deref(), Some("Weekend plans"));
    assert_eq!(group.recipient_ids.len(), 2);
}

#[test]
fn broken_guild_becomes_unavailable() {
    let ready = ready_with(|payload| payload["d"]["guilds"][0]["properties"]["name"] = 5.into());

    assert_eq!(
        ready.guilds[0],
        GatewayGuild::Unavailable(UnavailableGuild {
            id: Snowflake::new(200_000_000_000_000_001),
            geo_restricted: false,
        })
    );
    assert_eq!(ready.guilds.len(), ready.merged_members.len());
}

#[test]
fn broken_guild_without_top_level_id_becomes_unavailable() {
    let ready = ready_with(|payload| {
        let guild = &mut payload["d"]["guilds"][0];
        guild.as_object_mut().unwrap().remove("id");
        guild["properties"]["name"] = 5.into();
    });

    assert_eq!(
        ready.guilds[0],
        GatewayGuild::Unavailable(UnavailableGuild {
            id: Snowflake::new(200_000_000_000_000_001),
            geo_restricted: false,
        })
    );
}

#[test]
fn broken_properties_id_falls_back_to_the_top_level_id() {
    let without_id = ready_with(|payload| {
        payload["d"]["guilds"][0]["properties"]
            .as_object_mut()
            .unwrap()
            .remove("id");
    });
    let malformed_id =
        ready_with(|payload| payload["d"]["guilds"][0]["properties"]["id"] = "abc".into());

    for ready in [without_id, malformed_id] {
        assert_eq!(
            ready.guilds[0],
            GatewayGuild::Unavailable(UnavailableGuild {
                id: Snowflake::new(200_000_000_000_000_001),
                geo_restricted: false,
            })
        );
    }
}

#[test]
fn guild_without_properties_becomes_unavailable() {
    let ready = ready_with(|payload| {
        payload["d"]["guilds"][0]
            .as_object_mut()
            .unwrap()
            .remove("properties");
    });

    assert!(matches!(ready.guilds[0], GatewayGuild::Unavailable(_)));
    assert_eq!(
        ready.guilds[0].id(),
        Snowflake::new(200_000_000_000_000_001)
    );
}

#[test]
fn broken_channel_is_skipped() {
    let ready =
        ready_with(|payload| payload["d"]["guilds"][0]["channels"][1]["id"] = "general".into());

    let GatewayGuild::Available(lab) = &ready.guilds[0] else {
        panic!("guild became unavailable");
    };
    let ids: Vec<_> = lab.channels.iter().map(|channel| channel.id).collect();
    assert_eq!(
        ids,
        [
            Snowflake::new(300_000_000_000_000_001),
            Snowflake::new(300_000_000_000_000_003)
        ]
    );
}

#[test]
fn broken_private_channel_is_skipped() {
    let ready = ready_with(|payload| payload["d"]["private_channels"][0]["type"] = "dm".into());

    assert_eq!(ready.private_channels.len(), 1);
    assert_eq!(ready.private_channels[0].kind, ChannelType::GroupDm);
}

#[test]
fn broken_user_is_skipped() {
    let ready = ready_with(|payload| payload["d"]["users"][1]["username"] = Value::Null);

    let ids: Vec<_> = ready.users.iter().map(|user| user.id).collect();
    assert_eq!(ids, [Snowflake::new(100_000_000_000_000_002)]);
}

#[test]
fn broken_current_user_is_still_an_error() {
    let mut payload: Value = serde_json::from_str(include_str!("fixtures/ready.json")).unwrap();
    payload["d"]["user"]["id"] = Value::Null;

    assert!(matches!(
        decode(payload.to_string().as_bytes()),
        Err(DecodeError::Dispatch { seq: 1, ref event, .. }) if event == "READY"
    ));
}

#[test]
fn broken_member_is_skipped() {
    let ready =
        ready_with(|payload| payload["d"]["merged_members"][0][0]["roles"] = "moderators".into());

    assert_eq!(ready.merged_members.len(), ready.guilds.len());
    assert!(ready.merged_members[0].is_empty());
    assert!(matches!(ready.guilds[0], GatewayGuild::Available(_)));
}

#[test]
fn null_lists_count_as_empty() {
    let ready = ready_with(|payload| {
        let data = &mut payload["d"];
        for list in ["users", "private_channels", "merged_members"] {
            data[list] = Value::Null;
        }
        data["guilds"][0]["channels"] = Value::Null;
        data["guilds"][0]["threads"] = Value::Null;
    });

    assert!(ready.users.is_empty());
    assert!(ready.private_channels.is_empty());
    assert!(ready.merged_members.is_empty());
    let GatewayGuild::Available(lab) = &ready.guilds[0] else {
        panic!("guild became unavailable");
    };
    assert!(lab.channels.is_empty());
    assert!(lab.threads.is_empty());
}

#[test]
fn null_member_list_keeps_alignment() {
    let ready = ready_with(|payload| payload["d"]["merged_members"][0] = Value::Null);

    assert_eq!(ready.merged_members.len(), ready.guilds.len());
    assert!(ready.merged_members[0].is_empty());
}

#[test]
fn guild_with_sparse_properties_stays_available() {
    let ready = ready_with(|payload| {
        let guild = &mut payload["d"]["guilds"][0];
        let properties = guild["properties"].as_object_mut().unwrap();
        for field in [
            "owner_id",
            "afk_timeout",
            "preferred_locale",
            "default_message_notifications",
            "nsfw_level",
            "premium_tier",
            "features",
            "icon",
            "description",
        ] {
            properties.remove(field);
        }
        for role in guild["roles"].as_array_mut().unwrap() {
            let role = role.as_object_mut().unwrap();
            for field in [
                "name",
                "color",
                "colors",
                "hoist",
                "managed",
                "mentionable",
                "flags",
            ] {
                role.remove(field);
            }
        }
    });

    let GatewayGuild::Available(lab) = &ready.guilds[0] else {
        panic!("guild became unavailable");
    };
    let properties = &lab.properties;
    assert_eq!(properties.name, "Akari \u{2728} Lab");
    assert_eq!(properties.owner_id, None);
    assert_eq!(properties.afk_timeout, None);
    assert_eq!(properties.preferred_locale, "en-US");
    assert_eq!(
        properties.default_message_notifications,
        MessageNotificationLevel::AllMessages
    );
    assert_eq!(properties.nsfw_level, NsfwLevel::Default);
    assert_eq!(properties.premium_tier, PremiumTier::None);
    assert!(properties.features.is_empty());
    assert_eq!(lab.roles.len(), 2);
    assert_eq!(lab.roles[1].name, "");
    assert!(!lab.roles[1].hoist);
    assert_eq!(lab.roles[1].permissions, Permissions(1_099_511_627_775));
}

#[test]
#[ignore = "needs AKARI_READY_FIXTURE, see docs/protocol/ready.md"]
fn captured_ready_decodes_completely() {
    let path = std::env::var("AKARI_READY_FIXTURE")
        .expect("AKARI_READY_FIXTURE must point to a captured READY message");
    let json = std::fs::read_to_string(&path).expect("AKARI_READY_FIXTURE is not readable");
    let raw: Value = serde_json::from_str(&json).expect("AKARI_READY_FIXTURE is not JSON");
    let (_, ready) = ready_from(&json);

    let data = &raw["d"];
    let empty = Vec::new();
    let list = |value: &Value| value.as_array().unwrap_or(&empty).len();
    assert_eq!(
        ready.users.len(),
        list(&data["users"]),
        "users were skipped"
    );
    assert_eq!(
        ready.private_channels.len(),
        list(&data["private_channels"]),
        "private channels were skipped"
    );

    let raw_guilds = data["guilds"].as_array().unwrap_or(&empty);
    assert_eq!(ready.guilds.len(), raw_guilds.len());
    for (guild, raw_guild) in ready.guilds.iter().zip(raw_guilds) {
        let id = guild.id().get();
        match guild {
            GatewayGuild::Available(guild) => {
                let channels = list(&raw_guild["channels"]);
                let threads = list(&raw_guild["threads"]);
                assert_eq!(
                    guild.channels.len(),
                    channels,
                    "channels of guild {id} were skipped"
                );
                assert_eq!(
                    guild.threads.len(),
                    threads,
                    "threads of guild {id} were skipped"
                );
            }
            GatewayGuild::Unavailable(_) => {
                assert_eq!(raw_guild["unavailable"], true, "guild {id} failed to parse");
            }
        }
    }

    assert_eq!(
        ready.merged_members.len(),
        ready.guilds.len(),
        "merged_members is not aligned with guilds"
    );
    let raw_members = data["merged_members"].as_array().unwrap_or(&empty);
    for (members, raw) in ready.merged_members.iter().zip(raw_members) {
        assert_eq!(members.len(), list(raw), "members were skipped");
    }
}
