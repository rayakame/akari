use akari_core::gateway::{
    ChannelDelete, DecodeError, DispatchEvent, GatewayEvent, GatewayGuild, GuildDelete,
    GuildRoleDelete, Hello, MessageDelete, MessageDeleteBulk, Ready, UnavailableGuild, decode,
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
fn unknown_dispatches_stay_other() {
    assert_eq!(
        dispatch("TYPING_START", r#"{"channel_id": "300000000000000002"}"#),
        DispatchEvent::Other("TYPING_START".to_owned())
    );
}
