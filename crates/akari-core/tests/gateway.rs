use akari_core::gateway::{
    ChannelDelete, ChannelUnread, ChannelUnreadUpdate, DecodeError, DispatchEvent, GatewayEvent,
    GatewayGuild, GuildDelete, GuildRoleDelete, Hello, MessageAck, MessageDelete,
    MessageDeleteBulk, MuteConfig, NotificationSettings, PassiveUpdateV2, ReadStateEntry, Ready,
    Relationship, RelationshipRemove, RelationshipUpdate, SettingsProto, SettingsProtoUpdate,
    UnavailableGuild, Versioned, decode,
};
use akari_core::model::{
    ChannelType, MessageNotificationLevel, NsfwLevel, Permissions, PremiumTier, PremiumType,
    RelationshipType, Snowflake, Timestamp,
};
use serde_json::Value;

#[track_caller]
fn decode_ok(json: &str) -> GatewayEvent {
    decode(json.as_bytes()).unwrap_or_else(|err| panic!("failed to decode: {}", chain(&err)))
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
fn ready_reads_the_user_guild_settings() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    let settings = &ready.user_guild_settings;
    assert!(!settings.partial);
    assert_eq!(settings.version, 1234);
    let [guild, dms] = &settings.entries[..] else {
        panic!("expected two entries, got {:?}", settings.entries);
    };

    assert_eq!(
        guild.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    assert!(!guild.muted);
    assert_eq!(guild.mute_config, None);
    assert_eq!(
        guild.message_notifications,
        MessageNotificationLevel::OnlyMentions
    );
    assert!(guild.suppress_everyone);
    assert!(!guild.suppress_roles);
    assert!(guild.hide_muted_channels);
    assert_eq!(guild.flags, 1 << 12);
    let [category, channel] = &guild.channel_overrides[..] else {
        panic!("expected two overrides, got {:?}", guild.channel_overrides);
    };
    assert_eq!(category.channel_id, Snowflake::new(300_000_000_000_000_001));
    assert!(category.collapsed);
    assert_eq!(
        category.message_notifications,
        MessageNotificationLevel::Inherit
    );
    assert!(!category.muted);
    assert_eq!(category.mute_config, None);
    assert_eq!(category.flags, 0);
    assert_eq!(channel.channel_id, Snowflake::new(300_000_000_000_000_002));
    assert!(!channel.collapsed);
    assert_eq!(
        channel.message_notifications,
        MessageNotificationLevel::NoMessages
    );
    assert!(channel.muted);
    let mute = channel.mute_config.expect("a timed mute");
    assert_eq!(
        mute.end_time.map(Timestamp::unix_millis),
        Some(1_790_856_000_000)
    );
    assert_eq!(mute.selected_time_window, 3600);
    assert_eq!(channel.flags, 1 << 10);

    assert_eq!(dms.guild_id, None);
    let [dm] = &dms.channel_overrides[..] else {
        panic!("expected one override, got {:?}", dms.channel_overrides);
    };
    assert_eq!(dm.channel_id, Snowflake::new(300_000_000_000_000_010));
    assert!(dm.muted);
    assert_eq!(
        dm.mute_config,
        Some(MuteConfig {
            end_time: None,
            selected_time_window: -1,
        })
    );
    assert_eq!(dm.flags, 0, "a missing flags field is 0");

    assert_eq!(ready.user_settings_proto.as_deref(), Some(""));
    assert_eq!(
        ready.notification_settings,
        Some(NotificationSettings { flags: 16 })
    );
}

#[test]
fn a_broken_guild_settings_entry_is_skipped() {
    let ready = ready_with(|payload| {
        payload["d"]["user_guild_settings"]["entries"][0]["muted"] = "yes".into();
    });

    let guilds: Vec<_> = ready
        .user_guild_settings
        .entries
        .iter()
        .map(|entry| entry.guild_id)
        .collect();
    assert_eq!(guilds, [None]);
    assert_eq!(ready.user_guild_settings.version, 1234);
}

#[test]
fn a_broken_channel_override_is_skipped() {
    let ready = ready_with(|payload| {
        payload["d"]["user_guild_settings"]["entries"][0]["channel_overrides"][0]["collapsed"] =
            "yes".into();
    });

    let guild = &ready.user_guild_settings.entries[0];
    assert_eq!(
        guild.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    let channels: Vec<_> = guild
        .channel_overrides
        .iter()
        .map(|channel| channel.channel_id)
        .collect();
    assert_eq!(channels, [Snowflake::new(300_000_000_000_000_002)]);
}

#[test]
fn a_broken_settings_part_leaves_ready_intact() {
    let ready = ready_with(|payload| {
        let data = &mut payload["d"];
        data["user_guild_settings"] = 5.into();
        data["notification_settings"] = serde_json::json!({"flags": "16"});
        data["user_settings_proto"] = 7.into();
    });

    assert_eq!(ready.user_guild_settings, Versioned::default());
    assert_eq!(ready.notification_settings, None);
    assert_eq!(ready.user_settings_proto, None);
    assert_eq!(ready.guilds.len(), 2);
}

#[test]
fn missing_or_null_settings_parts_are_defaults() {
    let missing = ready_with(|payload| {
        let data = payload["d"].as_object_mut().unwrap();
        for part in [
            "user_guild_settings",
            "notification_settings",
            "user_settings_proto",
        ] {
            data.remove(part);
        }
    });
    let null = ready_with(|payload| {
        let data = &mut payload["d"];
        for part in [
            "user_guild_settings",
            "notification_settings",
            "user_settings_proto",
        ] {
            data[part] = Value::Null;
        }
    });

    for ready in [missing, null] {
        assert_eq!(ready.user_guild_settings, Versioned::default());
        assert_eq!(ready.notification_settings, None);
        assert_eq!(ready.user_settings_proto, None);
    }
}

#[test]
fn ready_reads_the_read_states() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    let read_states = &ready.read_state;
    assert!(!read_states.partial);
    assert_eq!(read_states.version, 4321);
    assert_eq!(
        read_states.entries,
        [
            ReadStateEntry {
                id: Snowflake::new(300_000_000_000_000_002),
                read_state_type: 0,
                last_message_id: Some(Snowflake::new(400_000_000_000_000_002)),
                mention_count: 2,
                flags: 1,
            },
            ReadStateEntry {
                id: Snowflake::new(300_000_000_000_000_020),
                read_state_type: 0,
                last_message_id: Some(Snowflake::new(400_000_000_000_000_030)),
                mention_count: 0,
                flags: 3,
            },
            ReadStateEntry {
                id: Snowflake::new(300_000_000_000_000_010),
                read_state_type: 0,
                last_message_id: Some(Snowflake::new(400_000_000_000_000_020)),
                mention_count: 0,
                flags: 0,
            },
            ReadStateEntry {
                id: Snowflake::new(200_000_000_000_000_001),
                read_state_type: 1,
                last_message_id: None,
                mention_count: 0,
                flags: 0,
            },
        ]
    );
}

#[test]
fn a_broken_read_state_entry_is_skipped() {
    let ready = ready_with(|payload| {
        payload["d"]["read_state"]["entries"][0]["mention_count"] = "two".into();
    });

    let ids: Vec<u64> = ready
        .read_state
        .entries
        .iter()
        .map(|entry| entry.id.get())
        .collect();
    assert_eq!(
        ids,
        [
            300_000_000_000_000_020,
            300_000_000_000_000_010,
            200_000_000_000_000_001
        ]
    );
    assert_eq!(ready.read_state.version, 4321);
}

#[test]
fn broken_read_states_leave_ready_intact() {
    let ready = ready_with(|payload| payload["d"]["read_state"] = "nope".into());

    assert_eq!(ready.read_state, Versioned::default());
    assert_eq!(ready.relationships.len(), 2);
}

#[test]
fn ready_reads_the_relationships() {
    let (_, ready) = ready_from(include_str!("fixtures/ready.json"));

    assert_eq!(
        ready.relationships,
        [
            Relationship {
                id: Snowflake::new(100_000_000_000_000_003),
                kind: RelationshipType::Friend,
                user_ignored: true,
            },
            Relationship {
                id: Snowflake::new(100_000_000_000_000_004),
                kind: RelationshipType::Blocked,
                user_ignored: false,
            },
        ]
    );
}

#[test]
fn a_broken_relationship_is_skipped() {
    let ready = ready_with(|payload| {
        payload["d"]["relationships"][0]["type"] = "friend".into();
    });

    let ids: Vec<u64> = ready
        .relationships
        .iter()
        .map(|relationship| relationship.id.get())
        .collect();
    assert_eq!(ids, [100_000_000_000_000_004]);
}

#[test]
fn missing_or_null_read_states_and_relationships_are_empty() {
    let missing = ready_with(|payload| {
        let data = payload["d"].as_object_mut().unwrap();
        data.remove("read_state");
        data.remove("relationships");
    });
    let null = ready_with(|payload| {
        payload["d"]["read_state"] = Value::Null;
        payload["d"]["relationships"] = Value::Null;
    });

    for ready in [missing, null] {
        assert_eq!(ready.read_state, Versioned::default());
        assert!(ready.relationships.is_empty());
    }
}

#[test]
fn null_lists_in_guild_settings_count_as_empty() {
    let ready = ready_with(|payload| {
        payload["d"]["user_guild_settings"]["entries"][0]["channel_overrides"] = Value::Null;
    });

    assert_eq!(ready.user_guild_settings.entries.len(), 2);
    assert!(
        ready.user_guild_settings.entries[0]
            .channel_overrides
            .is_empty()
    );
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

    let raw_settings = data["user_guild_settings"]["entries"]
        .as_array()
        .unwrap_or(&empty);
    assert_eq!(
        ready.user_guild_settings.entries.len(),
        raw_settings.len(),
        "guild settings entries were skipped"
    );
    for (settings, raw) in ready.user_guild_settings.entries.iter().zip(raw_settings) {
        assert_eq!(
            settings.channel_overrides.len(),
            list(&raw["channel_overrides"]),
            "channel overrides were skipped"
        );
    }
    assert_eq!(
        ready.read_state.entries.len(),
        list(&data["read_state"]["entries"]),
        "read states were skipped"
    );
    assert_eq!(
        ready.relationships.len(),
        list(&data["relationships"]),
        "relationships were skipped"
    );
    assert_eq!(
        ready.notification_settings.is_some(),
        data["notification_settings"].is_object(),
        "notification_settings didn't parse"
    );
    assert_eq!(
        ready.user_settings_proto.is_some(),
        data["user_settings_proto"].is_string(),
        "user_settings_proto didn't parse"
    );
}

#[track_caller]
fn dispatch(name: &str, data: &str) -> DispatchEvent {
    match decode_ok(&format!(
        r#"{{"op": 0, "s": 7, "t": "{name}", "d": {data}}}"#
    )) {
        GatewayEvent::Dispatch { seq: 7, event } => event,
        other => panic!("expected a dispatch, got {other:?}"),
    }
}

#[track_caller]
fn edited(fixture: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value =
        serde_json::from_str(fixture).unwrap_or_else(|err| panic!("fixture is not JSON: {err}"));
    edit(&mut value);
    value.to_string()
}

#[test]
fn ready_supplemental_decodes() {
    let DispatchEvent::ReadySupplemental(supplemental) = dispatch(
        "READY_SUPPLEMENTAL",
        include_str!("fixtures/ready_supplemental.json"),
    ) else {
        panic!("expected READY_SUPPLEMENTAL");
    };

    assert_eq!(
        supplemental
            .guilds
            .iter()
            .map(|guild| guild.id)
            .collect::<Vec<_>>(),
        [
            Snowflake::new(200_000_000_000_000_001),
            Snowflake::new(200_000_000_000_000_002)
        ]
    );
    assert_eq!(supplemental.merged_members.len(), 2);
    assert_eq!(
        supplemental.merged_members[0][0].user_id,
        Some(Snowflake::new(100_000_000_000_000_002))
    );
    assert!(supplemental.merged_members[1].is_empty());
    assert_eq!(supplemental.lazy_private_channels.len(), 1);
    assert_eq!(
        supplemental.lazy_private_channels[0].recipients[0].username,
        "sol"
    );
}

#[test]
fn ready_supplemental_without_lazy_channels_decodes() {
    let data = edited(include_str!("fixtures/ready_supplemental.json"), |value| {
        value
            .as_object_mut()
            .unwrap()
            .remove("lazy_private_channels");
    });

    let DispatchEvent::ReadySupplemental(supplemental) = dispatch("READY_SUPPLEMENTAL", &data)
    else {
        panic!("expected READY_SUPPLEMENTAL");
    };

    assert!(supplemental.lazy_private_channels.is_empty());
}

#[test]
fn guild_create_decodes_like_a_ready_guild() {
    let DispatchEvent::GuildCreate(guild) =
        dispatch("GUILD_CREATE", include_str!("fixtures/guild_create.json"))
    else {
        panic!("expected GUILD_CREATE");
    };
    let GatewayGuild::Available(guild) = *guild else {
        panic!("expected an available guild");
    };

    assert_eq!(guild.properties.name, "Garden");
    assert_eq!(guild.channels.len(), 1);
    assert_eq!(guild.roles.len(), 1);
    assert_eq!(guild.member_count, Some(3));
    let member = &guild.members[0];
    assert_eq!(
        member.user.as_ref().map(|user| user.id),
        Some(Snowflake::new(100_000_000_000_000_001))
    );
}

#[test]
fn an_unavailable_guild_create_decodes() {
    let event = dispatch(
        "GUILD_CREATE",
        r#"{"id": "200000000000000003", "unavailable": true}"#,
    );

    assert_eq!(
        event,
        DispatchEvent::GuildCreate(Box::new(GatewayGuild::Unavailable(UnavailableGuild {
            id: Snowflake::new(200_000_000_000_000_003),
            geo_restricted: false,
        })))
    );
}

#[test]
fn guild_update_is_partial() {
    let DispatchEvent::GuildUpdate(update) = dispatch(
        "GUILD_UPDATE",
        r#"{"id": "200000000000000001", "name": "Renamed"}"#,
    ) else {
        panic!("expected GUILD_UPDATE");
    };

    assert_eq!(update.id, Snowflake::new(200_000_000_000_000_001));
    assert_eq!(update.name.as_deref(), Some("Renamed"));
    assert_eq!(update.icon, None);
    assert_eq!(update.owner_id, None);
    assert_eq!(update.roles, None);
}

#[test]
fn a_full_guild_update_separates_null_from_missing() {
    let DispatchEvent::GuildUpdate(update) =
        dispatch("GUILD_UPDATE", include_str!("fixtures/guild_update.json"))
    else {
        panic!("expected GUILD_UPDATE");
    };

    assert_eq!(update.name.as_deref(), Some("Akari Lab"));
    assert_eq!(update.icon, Some(None));
    assert_eq!(
        update.banner,
        Some(Some("0123456789abcdef0123456789abcdef".to_owned()))
    );
    assert_eq!(update.roles.as_ref().map(Vec::len), Some(2));
}

#[test]
fn a_guild_update_reads_properties_too() {
    let event = dispatch(
        "GUILD_UPDATE",
        r#"{"id": "200000000000000001", "properties": {"id": "200000000000000001", "name": "Nested", "icon": null}}"#,
    );
    let DispatchEvent::GuildUpdate(update) = event else {
        panic!("expected GUILD_UPDATE");
    };

    assert_eq!(update.name.as_deref(), Some("Nested"));
    assert_eq!(update.icon, Some(None));
}

#[test]
fn a_guild_update_reads_the_default_level() {
    for data in [
        r#"{"id": "200000000000000001", "default_message_notifications": 0}"#,
        r#"{"id": "200000000000000001", "properties": {"default_message_notifications": 0}}"#,
    ] {
        let DispatchEvent::GuildUpdate(update) = dispatch("GUILD_UPDATE", data) else {
            panic!("expected GUILD_UPDATE");
        };
        assert_eq!(
            update.default_message_notifications,
            Some(MessageNotificationLevel::AllMessages)
        );
    }
    let DispatchEvent::GuildUpdate(update) =
        dispatch("GUILD_UPDATE", r#"{"id": "200000000000000001"}"#)
    else {
        panic!("expected GUILD_UPDATE");
    };
    assert_eq!(update.default_message_notifications, None);
}

#[test]
fn guild_delete_tells_leaving_from_an_outage() {
    let left = dispatch("GUILD_DELETE", r#"{"id": "200000000000000001"}"#);
    let down = dispatch(
        "GUILD_DELETE",
        r#"{"id": "200000000000000001", "unavailable": true}"#,
    );

    let id = Snowflake::new(200_000_000_000_000_001);
    assert_eq!(
        left,
        DispatchEvent::GuildDelete(GuildDelete {
            id,
            unavailable: false
        })
    );
    assert_eq!(
        down,
        DispatchEvent::GuildDelete(GuildDelete {
            id,
            unavailable: true
        })
    );
}

#[test]
fn role_events_decode() {
    let created = dispatch(
        "GUILD_ROLE_CREATE",
        r#"{"guild_id": "200000000000000001", "role": {"id": "500000000000000003", "name": "Helpers", "permissions": "2048", "position": 2}}"#,
    );
    let deleted = dispatch(
        "GUILD_ROLE_DELETE",
        r#"{"guild_id": "200000000000000001", "role_id": "500000000000000003"}"#,
    );

    let DispatchEvent::GuildRoleCreate(created) = created else {
        panic!("expected GUILD_ROLE_CREATE");
    };
    assert_eq!(created.guild_id, Snowflake::new(200_000_000_000_000_001));
    assert_eq!(created.role.name, "Helpers");
    assert_eq!(created.role.permissions, Permissions(2048));
    assert_eq!(
        deleted,
        DispatchEvent::GuildRoleDelete(GuildRoleDelete {
            guild_id: Snowflake::new(200_000_000_000_000_001),
            role_id: Snowflake::new(500_000_000_000_000_003),
        })
    );
    assert!(matches!(
        dispatch(
            "GUILD_ROLE_UPDATE",
            r#"{"guild_id": "200000000000000001", "role": {"id": "500000000000000003", "permissions": "0", "position": 2}}"#,
        ),
        DispatchEvent::GuildRoleUpdate(_)
    ));
}

#[test]
fn guild_member_update_separates_missing_from_null() {
    let fixture = include_str!("fixtures/guild_member_update.json");
    let DispatchEvent::GuildMemberUpdate(missing) = dispatch("GUILD_MEMBER_UPDATE", fixture) else {
        panic!("expected GUILD_MEMBER_UPDATE");
    };
    let cleared = edited(fixture, |value| value["nick"] = Value::Null);
    let DispatchEvent::GuildMemberUpdate(cleared) = dispatch("GUILD_MEMBER_UPDATE", &cleared)
    else {
        panic!("expected GUILD_MEMBER_UPDATE");
    };

    assert_eq!(missing.guild_id, Snowflake::new(200_000_000_000_000_001));
    assert_eq!(missing.user.id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(missing.nick, None);
    assert_eq!(cleared.nick, Some(None));
    assert_eq!(
        missing.roles.as_deref(),
        Some(&[Snowflake::new(500_000_000_000_000_002)][..])
    );
    assert!(matches!(
        missing.communication_disabled_until,
        Some(Some(_))
    ));
}

#[test]
fn channel_create_decodes_a_dm_with_recipients() {
    let DispatchEvent::ChannelCreate(channel) = dispatch(
        "CHANNEL_CREATE",
        include_str!("fixtures/channel_create_dm.json"),
    ) else {
        panic!("expected CHANNEL_CREATE");
    };

    assert_eq!(channel.kind, ChannelType::Dm);
    assert_eq!(channel.recipients[0].username, "ren");
}

#[test]
fn channel_update_is_partial() {
    let DispatchEvent::ChannelUpdate(update) =
        dispatch("CHANNEL_UPDATE", r#"{"id": "300000000000000002"}"#)
    else {
        panic!("expected CHANNEL_UPDATE");
    };

    assert_eq!(update.id, Snowflake::new(300_000_000_000_000_002));
    assert_eq!(update.name, None);
    assert_eq!(update.topic, None);
    assert_eq!(update.permission_overwrites, None);
}

#[test]
fn a_full_channel_update_separates_null_from_missing() {
    let DispatchEvent::ChannelUpdate(update) = dispatch(
        "CHANNEL_UPDATE",
        include_str!("fixtures/channel_update.json"),
    ) else {
        panic!("expected CHANNEL_UPDATE");
    };

    assert_eq!(update.kind, Some(ChannelType::GuildText));
    assert_eq!(update.name, Some(Some("general-chat".to_owned())));
    assert_eq!(update.topic, Some(None));
    assert_eq!(
        update.parent_id,
        Some(Some(Snowflake::new(300_000_000_000_000_001)))
    );
    assert_eq!(update.rate_limit_per_user, Some(5));
    assert_eq!(update.permission_overwrites.as_ref().map(Vec::len), Some(1));
}

#[test]
fn channel_delete_decodes_a_partial_dm() {
    let event = dispatch(
        "CHANNEL_DELETE",
        r#"{"id": "300000000000000010", "type": 1}"#,
    );

    assert_eq!(
        event,
        DispatchEvent::ChannelDelete(ChannelDelete {
            id: Snowflake::new(300_000_000_000_000_010),
            guild_id: None,
            parent_id: None,
        })
    );
}

#[test]
fn thread_create_and_update_decode() {
    let DispatchEvent::ThreadCreate(thread) =
        dispatch("THREAD_CREATE", include_str!("fixtures/thread_create.json"))
    else {
        panic!("expected THREAD_CREATE");
    };
    let archived = edited(include_str!("fixtures/thread_create.json"), |value| {
        value["thread_metadata"]["archived"] = true.into();
    });
    let DispatchEvent::ThreadUpdate(update) = dispatch("THREAD_UPDATE", &archived) else {
        panic!("expected THREAD_UPDATE");
    };

    assert_eq!(thread.kind, ChannelType::PublicThread);
    assert_eq!(thread.name.as_deref(), Some("Bug triage"));
    assert_eq!(update.thread_metadata.map(|meta| meta.archived), Some(true));
}

#[test]
fn thread_member_update_decodes() {
    let DispatchEvent::ThreadMemberUpdate(update) = dispatch(
        "THREAD_MEMBER_UPDATE",
        include_str!("fixtures/thread_member_update.json"),
    ) else {
        panic!("expected THREAD_MEMBER_UPDATE");
    };

    assert_eq!(
        update.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    let member = update.member;
    assert_eq!(member.id, Some(Snowflake::new(300_000_000_000_000_021)));
    assert_eq!(
        member.user_id,
        Some(Snowflake::new(100_000_000_000_000_001))
    );
    assert_eq!(member.flags, 5);
    assert!(member.muted);
    assert_eq!(
        member
            .mute_config
            .and_then(|mute| mute.end_time)
            .map(Timestamp::unix_millis),
        Some(1_791_461_700_000)
    );
}

#[test]
fn thread_members_update_decodes() {
    let DispatchEvent::ThreadMembersUpdate(update) = dispatch(
        "THREAD_MEMBERS_UPDATE",
        include_str!("fixtures/thread_members_update.json"),
    ) else {
        panic!("expected THREAD_MEMBERS_UPDATE");
    };

    assert_eq!(update.id, Snowflake::new(300_000_000_000_000_021));
    assert_eq!(
        update.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    assert_eq!(update.member_count, Some(3));
    assert_eq!(update.added_members.len(), 1);
    assert_eq!(
        update.added_members[0].user_id,
        Some(Snowflake::new(100_000_000_000_000_001))
    );
    assert_eq!(update.added_members[0].flags, 2);
    assert!(update.added_members[0].mute_config.is_none());
    assert_eq!(
        update.removed_member_ids,
        [Snowflake::new(100_000_000_000_000_002)]
    );
}

#[test]
fn a_thread_member_in_another_shape_still_decodes() {
    let drifted = edited(include_str!("fixtures/thread_create.json"), |value| {
        value["member"] = serde_json::json!({
            "id": "300000000000000021", "user_id": "100000000000000001",
            "muted": null, "mute_config": "bad", "flags": 1
        });
    });
    let DispatchEvent::ThreadCreate(thread) = dispatch("THREAD_CREATE", &drifted) else {
        panic!("expected THREAD_CREATE");
    };

    let member = thread.member.expect("the member is kept");
    assert!(!member.muted);
    assert!(member.mute_config.is_none());
    assert!(member.join_timestamp.is_none());
    assert_eq!(member.flags, 1);
}

#[test]
fn a_members_update_with_unknown_fields_decodes() {
    let drifted = edited(
        include_str!("fixtures/thread_members_update.json"),
        |value| {
            let good = value["added_members"][0].clone();
            value["added_members"] = serde_json::json!(["bad", good]);
            value["removed_member_ids"] = Value::Null;
            value["member_ids_preview"] = serde_json::json!({"changed": "shape"});
        },
    );
    let DispatchEvent::ThreadMembersUpdate(update) = dispatch("THREAD_MEMBERS_UPDATE", &drifted)
    else {
        panic!("expected THREAD_MEMBERS_UPDATE");
    };

    assert_eq!(update.added_members.len(), 1);
    assert_eq!(
        update.added_members[0].user_id,
        Some(Snowflake::new(100_000_000_000_000_001))
    );
    assert!(update.removed_member_ids.is_empty());
}

#[test]
fn thread_delete_decodes_its_four_fields() {
    let event = dispatch(
        "THREAD_DELETE",
        r#"{"id": "300000000000000021", "guild_id": "200000000000000001", "parent_id": "300000000000000002", "type": 11}"#,
    );

    assert_eq!(
        event,
        DispatchEvent::ThreadDelete(ChannelDelete {
            id: Snowflake::new(300_000_000_000_000_021),
            guild_id: Some(Snowflake::new(200_000_000_000_000_001)),
            parent_id: Some(Snowflake::new(300_000_000_000_000_002)),
        })
    );
}

#[test]
fn message_create_decodes() {
    let DispatchEvent::MessageCreate(message) = dispatch(
        "MESSAGE_CREATE",
        include_str!("fixtures/message_create.json"),
    ) else {
        panic!("expected MESSAGE_CREATE");
    };

    assert_eq!(message.id, Snowflake::new(400_000_000_000_000_003));
    assert_eq!(message.content, "hi");
}

#[test]
fn message_update_is_partial() {
    let DispatchEvent::MessageUpdate(update) = dispatch(
        "MESSAGE_UPDATE",
        r#"{"id": "400000000000000001", "channel_id": "300000000000000002", "embeds": []}"#,
    ) else {
        panic!("expected MESSAGE_UPDATE");
    };

    assert_eq!(update.id, Snowflake::new(400_000_000_000_000_001));
    assert_eq!(update.channel_id, Snowflake::new(300_000_000_000_000_002));
    assert_eq!(update.content, None);
    assert_eq!(update.edited_timestamp, None);
    assert_eq!(update.embeds.as_ref().map(Vec::len), Some(0));
    assert_eq!(update.author, None);
}

#[test]
fn a_full_message_update_decodes() {
    let DispatchEvent::MessageUpdate(update) = dispatch(
        "MESSAGE_UPDATE",
        include_str!("fixtures/message_update.json"),
    ) else {
        panic!("expected MESSAGE_UPDATE");
    };

    assert!(update.content.as_deref().unwrap().ends_with("(fixed link)"));
    assert!(matches!(update.edited_timestamp, Some(Some(_))));
    assert_eq!(update.embeds.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        update.author.as_ref().map(|author| author.id),
        Some(Snowflake::new(100_000_000_000_000_002))
    );
}

#[test]
fn a_broken_embed_in_a_message_update_drops_only_the_embed() {
    let data = edited(include_str!("fixtures/message_update.json"), |value| {
        value["embeds"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"color": "red"}));
    });

    let DispatchEvent::MessageUpdate(update) = dispatch("MESSAGE_UPDATE", &data) else {
        panic!("expected MESSAGE_UPDATE");
    };

    assert_eq!(update.embeds.as_ref().map(Vec::len), Some(1));
}

#[test]
fn message_delete_and_bulk_delete_decode() {
    let single = dispatch(
        "MESSAGE_DELETE",
        r#"{"id": "400000000000000001", "channel_id": "300000000000000002", "guild_id": "200000000000000001"}"#,
    );
    let bulk = dispatch(
        "MESSAGE_DELETE_BULK",
        r#"{"ids": ["400000000000000001", "400000000000000002"], "channel_id": "300000000000000010"}"#,
    );

    assert_eq!(
        single,
        DispatchEvent::MessageDelete(MessageDelete {
            id: Snowflake::new(400_000_000_000_000_001),
            channel_id: Snowflake::new(300_000_000_000_000_002),
            guild_id: Some(Snowflake::new(200_000_000_000_000_001)),
        })
    );
    assert_eq!(
        bulk,
        DispatchEvent::MessageDeleteBulk(MessageDeleteBulk {
            ids: vec![
                Snowflake::new(400_000_000_000_000_001),
                Snowflake::new(400_000_000_000_000_002)
            ],
            channel_id: Snowflake::new(300_000_000_000_000_010),
            guild_id: None,
        })
    );
}

#[test]
fn user_update_decodes() {
    let DispatchEvent::UserUpdate(update) =
        dispatch("USER_UPDATE", include_str!("fixtures/user_update.json"))
    else {
        panic!("expected USER_UPDATE");
    };

    assert_eq!(update.id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(update.global_name, Some(Some("Akari".to_owned())));
    assert_eq!(update.premium_type, Some(PremiumType::Tier2));
    assert_eq!(update.mfa_enabled, Some(true));
    assert!(!format!("{update:?}").contains("example.invalid"));
}

#[test]
fn a_dispatch_with_a_broken_payload_is_a_dispatch_error() {
    let data = edited(include_str!("fixtures/message_create.json"), |value| {
        value.as_object_mut().unwrap().remove("author");
    });

    let err =
        decode(format!(r#"{{"op": 0, "s": 8, "t": "MESSAGE_CREATE", "d": {data}}}"#).as_bytes())
            .unwrap_err();

    assert!(
        matches!(&err, DecodeError::Dispatch { seq: 8, event, .. } if event == "MESSAGE_CREATE"),
        "{err:?}"
    );
}

#[test]
fn the_settings_dispatches_decode() {
    let DispatchEvent::UserGuildSettingsUpdate(settings) = dispatch(
        "USER_GUILD_SETTINGS_UPDATE",
        r#"{
            "guild_id": "200000000000000001",
            "muted": true,
            "mute_config": {"end_time": "2026-10-01T13:00:00+00:00", "selected_time_window": 3600},
            "message_notifications": 3,
            "suppress_everyone": false,
            "suppress_roles": true,
            "notify_highlights": 0,
            "mobile_push": true,
            "hide_muted_channels": false,
            "mute_scheduled_events": false,
            "flags": 2048,
            "version": 8,
            "channel_overrides": [{
                "channel_id": "300000000000000002",
                "collapsed": false,
                "message_notifications": 3,
                "muted": false,
                "mute_config": null,
                "flags": 66560
            }]
        }"#,
    ) else {
        panic!("expected USER_GUILD_SETTINGS_UPDATE");
    };
    assert_eq!(
        settings.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    assert!(settings.muted);
    assert_eq!(
        settings
            .mute_config
            .and_then(|mute| mute.end_time)
            .map(Timestamp::unix_millis),
        Some(1_790_859_600_000)
    );
    assert_eq!(
        settings.message_notifications,
        MessageNotificationLevel::Inherit
    );
    assert!(settings.suppress_roles);
    assert_eq!(settings.flags, 1 << 11);
    assert_eq!(
        settings.channel_overrides[0].flags,
        (1 << 16) | (1 << 10),
        "an unknown bit is kept"
    );

    assert_eq!(
        dispatch("NOTIFICATION_SETTINGS_UPDATE", r#"{"flags": 48}"#),
        DispatchEvent::NotificationSettingsUpdate(NotificationSettings { flags: 48 })
    );

    let DispatchEvent::UserSettingsProtoUpdate(update) = dispatch(
        "USER_SETTINGS_PROTO_UPDATE",
        r#"{"settings": {"type": 1, "proto": "cgA="}, "partial": true}"#,
    ) else {
        panic!("expected USER_SETTINGS_PROTO_UPDATE");
    };
    assert_eq!(update.settings.kind, 1);
    assert_eq!(update.settings.proto, "cgA=");
    assert!(update.partial);
}

#[test]
fn the_proto_update_keeps_its_type_and_partial() {
    let DispatchEvent::UserSettingsProtoUpdate(update) = dispatch(
        "USER_SETTINGS_PROTO_UPDATE",
        r#"{"settings": {"type": 2, "proto": ""}, "partial": false}"#,
    ) else {
        panic!("expected USER_SETTINGS_PROTO_UPDATE");
    };

    assert_eq!(
        *update,
        SettingsProtoUpdate {
            settings: SettingsProto {
                kind: 2,
                proto: String::new(),
            },
            partial: false,
        }
    );
}

#[test]
fn passive_update_v2_decodes() {
    let DispatchEvent::PassiveUpdateV2(update) = dispatch(
        "PASSIVE_UPDATE_V2",
        r#"{
            "guild_id": "200000000000000001",
            "updated_channels": [
                {
                    "id": "300000000000000002",
                    "last_message_id": "400000000000000003",
                    "last_pin_timestamp": "2026-10-01T13:00:00+00:00"
                },
                {"id": "300000000000000004", "last_message_id": null}
            ],
            "updated_voice_states": [{
                "user_id": "100000000000000005",
                "channel_id": "300000000000000006",
                "session_id": "abc"
            }],
            "removed_voice_states": ["100000000000000007"],
            "updated_members": []
        }"#,
    ) else {
        panic!("expected PASSIVE_UPDATE_V2");
    };

    assert_eq!(
        update,
        PassiveUpdateV2 {
            guild_id: Snowflake::new(200_000_000_000_000_001),
            updated_channels: vec![
                ChannelUnread {
                    id: Snowflake::new(300_000_000_000_000_002),
                    last_message_id: Some(Snowflake::new(400_000_000_000_000_003)),
                },
                ChannelUnread {
                    id: Snowflake::new(300_000_000_000_000_004),
                    last_message_id: None,
                },
            ],
        }
    );
}

#[test]
fn a_broken_updated_channel_is_skipped() {
    let DispatchEvent::PassiveUpdateV2(update) = dispatch(
        "PASSIVE_UPDATE_V2",
        r#"{
            "guild_id": "200000000000000001",
            "updated_channels": [
                {"id": "not a snowflake", "last_message_id": "400000000000000003"},
                {"id": "300000000000000004", "last_message_id": "400000000000000005"}
            ]
        }"#,
    ) else {
        panic!("expected PASSIVE_UPDATE_V2");
    };

    assert_eq!(
        update.updated_channels,
        [ChannelUnread {
            id: Snowflake::new(300_000_000_000_000_004),
            last_message_id: Some(Snowflake::new(400_000_000_000_000_005)),
        }]
    );
}

#[test]
fn a_passive_update_with_only_voice_changes_decodes() {
    for channels in [r#""updated_channels": null,"#, ""] {
        let DispatchEvent::PassiveUpdateV2(update) = dispatch(
            "PASSIVE_UPDATE_V2",
            &format!(
                r#"{{{channels} "guild_id": "200000000000000001",
                    "removed_voice_states": ["100000000000000007"]}}"#
            ),
        ) else {
            panic!("expected PASSIVE_UPDATE_V2");
        };

        assert!(update.updated_channels.is_empty(), "{channels}");
    }
}

#[test]
fn message_ack_decodes() {
    let DispatchEvent::MessageAck(ack) = dispatch(
        "MESSAGE_ACK",
        r#"{
            "channel_id": "300000000000000002",
            "message_id": "400000000000000004",
            "mention_count": 1,
            "flags": 1,
            "last_viewed": 4302,
            "version": 4322
        }"#,
    ) else {
        panic!("expected MESSAGE_ACK");
    };

    assert_eq!(
        ack,
        MessageAck {
            ack_type: 0,
            channel_id: Snowflake::new(300_000_000_000_000_002),
            message_id: Snowflake::new(400_000_000_000_000_004),
            manual: false,
            mention_count: Some(1),
            version: 4322,
        }
    );
}

#[test]
fn a_manual_ack_without_a_count_keeps_the_count() {
    for count in [r#""mention_count": null,"#, ""] {
        let DispatchEvent::MessageAck(ack) = dispatch(
            "MESSAGE_ACK",
            &format!(
                r#"{{{count} "ack_type": 0, "channel_id": "300000000000000002",
                    "message_id": "400000000000000001", "manual": true, "version": 4323}}"#
            ),
        ) else {
            panic!("expected MESSAGE_ACK");
        };

        assert!(ack.manual, "{count}");
        assert_eq!(ack.mention_count, None, "{count}");
    }
}

#[test]
fn an_ack_of_another_read_state_type_keeps_its_type() {
    let DispatchEvent::MessageAck(ack) = dispatch(
        "MESSAGE_ACK",
        r#"{"ack_type": 1, "channel_id": "200000000000000001",
            "message_id": "500000000000000002", "version": 4324}"#,
    ) else {
        panic!("expected MESSAGE_ACK");
    };

    assert_eq!(ack.ack_type, 1);
}

#[test]
fn an_ack_of_a_thread_id_in_a_forum_decodes() {
    let DispatchEvent::MessageAck(ack) = dispatch(
        "MESSAGE_ACK",
        r#"{"channel_id": "300000000000000003", "message_id": "300000000000000021",
            "version": 4325}"#,
    ) else {
        panic!("expected MESSAGE_ACK");
    };

    assert_eq!(ack.message_id.get(), 300_000_000_000_000_021);
}

#[test]
fn channel_unread_update_decodes() {
    let DispatchEvent::ChannelUnreadUpdate(update) = dispatch(
        "CHANNEL_UNREAD_UPDATE",
        r#"{
            "guild_id": "200000000000000001",
            "channel_unread_updates": [
                {
                    "id": "300000000000000002",
                    "last_message_id": "400000000000000005",
                    "last_pin_timestamp": null
                },
                {"id": "not a snowflake", "last_message_id": "400000000000000006"},
                {"id": "300000000000000003", "last_message_id": null}
            ]
        }"#,
    ) else {
        panic!("expected CHANNEL_UNREAD_UPDATE");
    };

    assert_eq!(
        update,
        ChannelUnreadUpdate {
            guild_id: Snowflake::new(200_000_000_000_000_001),
            channel_unread_updates: vec![
                ChannelUnread {
                    id: Snowflake::new(300_000_000_000_000_002),
                    last_message_id: Some(Snowflake::new(400_000_000_000_000_005)),
                },
                ChannelUnread {
                    id: Snowflake::new(300_000_000_000_000_003),
                    last_message_id: None,
                },
            ],
        }
    );
}

#[test]
fn relationship_events_decode() {
    let DispatchEvent::RelationshipAdd(added) = dispatch(
        "RELATIONSHIP_ADD",
        r#"{
            "id": "100000000000000005",
            "type": 2,
            "user": {"id": "100000000000000005", "username": "kai", "discriminator": "0", "avatar": null},
            "nickname": null,
            "user_ignored": false,
            "should_notify": false
        }"#,
    ) else {
        panic!("expected RELATIONSHIP_ADD");
    };
    assert_eq!(
        added,
        Relationship {
            id: Snowflake::new(100_000_000_000_000_005),
            kind: RelationshipType::Blocked,
            user_ignored: false,
        }
    );

    let DispatchEvent::RelationshipUpdate(updated) = dispatch(
        "RELATIONSHIP_UPDATE",
        r#"{"id": "100000000000000003", "type": 1, "nickname": "pal", "user_ignored": false}"#,
    ) else {
        panic!("expected RELATIONSHIP_UPDATE");
    };
    assert_eq!(
        updated,
        RelationshipUpdate {
            id: Snowflake::new(100_000_000_000_000_003),
            user_ignored: Some(false),
        }
    );

    let DispatchEvent::RelationshipRemove(removed) = dispatch(
        "RELATIONSHIP_REMOVE",
        r#"{"id": "100000000000000004", "type": 2, "nickname": null}"#,
    ) else {
        panic!("expected RELATIONSHIP_REMOVE");
    };
    assert_eq!(
        removed,
        RelationshipRemove {
            id: Snowflake::new(100_000_000_000_000_004),
        }
    );
}

#[test]
fn a_relationship_update_without_the_ignored_flag_leaves_it_unknown() {
    let DispatchEvent::RelationshipUpdate(updated) = dispatch(
        "RELATIONSHIP_UPDATE",
        r#"{"id": "100000000000000003", "type": 1, "nickname": "pal"}"#,
    ) else {
        panic!("expected RELATIONSHIP_UPDATE");
    };

    assert_eq!(updated.user_ignored, None);
}

#[test]
fn unknown_dispatches_stay_other() {
    assert_eq!(
        dispatch("TYPING_START", r#"{"channel_id": "300000000000000002"}"#),
        DispatchEvent::Other("TYPING_START".to_owned())
    );
}

fn chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[test]
fn decode_errors_never_quote_the_payload() {
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json")).unwrap();
    ready["d"]["user"]["id"] = "leak-me".into();
    let dispatch = decode(ready.to_string().as_bytes()).unwrap_err();
    let envelope = decode(br#"{"op": "leak-me"}"#).unwrap_err();

    for err in [dispatch, envelope] {
        let shown = format!("{err:?} {}", chain(&err));
        assert!(!shown.contains("leak-me"), "{shown}");
        assert!(shown.contains("line 1"), "{shown}");
    }
}
