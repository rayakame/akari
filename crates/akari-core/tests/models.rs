use std::collections::{BTreeSet, HashSet};

use akari_core::model::{
    Channel, ChannelMarker, ChannelType, CurrentUser, GenericMarker, Guild, GuildMarker,
    GuildMember, Message, MessageNotificationLevel, MessageReferenceType, MessageType, NsfwLevel,
    OverwriteType, PartialEmoji, PermissionOverwrite, Permissions, PremiumTier, PremiumType,
    ReactionCountDetails, Role, RoleColors, RoleMarker, Snowflake, StickerFormatType, Timestamp,
    User, UserMarker,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[track_caller]
fn parse<T: DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|err| panic!("failed to parse: {err}"))
}

#[test]
fn ids_parse_from_strings_and_integers() {
    assert_eq!(
        parse::<Snowflake<UserMarker>>(r#""100000000000000001""#),
        Snowflake::new(100_000_000_000_000_001)
    );
    assert_eq!(parse::<Snowflake<GuildMarker>>("373").get(), 373);
    assert_eq!(
        parse::<Snowflake<ChannelMarker>>(r#""18446744073709551615""#).get(),
        u64::MAX
    );
}

#[test]
fn values_that_are_not_ids_are_rejected() {
    for json in [r#""abc""#, r#""""#, "-1", "1.5", "null"] {
        assert!(
            serde_json::from_str::<Snowflake<UserMarker>>(json).is_err(),
            "{json} parsed"
        );
    }
}

#[test]
fn ids_are_ordered_hashable_and_castable() {
    let guild: Snowflake<GuildMarker> = Snowflake::new(200_000_000_000_000_001);
    let everyone: Snowflake<RoleMarker> = guild.cast();
    assert_eq!(everyone.get(), guild.get());

    let sorted: BTreeSet<Snowflake<UserMarker>> =
        [Snowflake::new(3), Snowflake::new(1), Snowflake::new(2)].into();
    assert_eq!(
        sorted.into_iter().map(Snowflake::get).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let unique: HashSet<Snowflake<UserMarker>> = [Snowflake::new(1), Snowflake::new(1)].into();
    assert_eq!(unique.len(), 1);
}

#[test]
fn timestamps_parse_with_and_without_fraction() {
    let precise: Timestamp = parse(r#""2023-02-17T19:52:19.184000+00:00""#);
    let whole: Timestamp = parse(r#""2023-02-17T09:22:28+00:00""#);

    assert_eq!(precise.unix_millis(), 1_676_663_539_184);
    assert_eq!(whole.unix_millis(), 1_676_625_748_000);
}

#[test]
fn values_that_are_not_timestamps_are_rejected() {
    for json in [r#""yesterday""#, r#""2023-02-17""#, "1676625748"] {
        assert!(
            serde_json::from_str::<Timestamp>(json).is_err(),
            "{json} parsed"
        );
    }
}

#[test]
fn permissions_parse_from_strings() {
    assert_eq!(
        parse::<Permissions>(r#""110917634608832""#),
        Permissions(110_917_634_608_832)
    );
}

#[test]
fn user_parses() {
    let user: User = parse(include_str!("fixtures/user.json"));

    assert_eq!(user.id, Snowflake::new(100_000_000_000_000_002));
    assert_eq!(user.username, "mira");
    assert_eq!(user.discriminator, "0");
    assert_eq!(user.global_name.as_deref(), Some("Mira"));
    assert_eq!(
        user.avatar.as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    assert_eq!(user.accent_color, Some(5_793_266));
    assert_eq!(user.public_flags, 64);
    assert!(!user.bot);
    let decoration = user.avatar_decoration_data.unwrap();
    assert_eq!(decoration.sku_id, Snowflake::new(900_000_000_000_000_001));
    assert_eq!(decoration.expires_at, None);
    let tag = user.primary_guild.unwrap();
    assert_eq!(
        tag.identity_guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    assert_eq!(tag.tag.as_deref(), Some("AKRI"));
}

#[test]
fn webhook_author_without_optional_fields_parses() {
    let user: User = parse(
        r#"{"id": "100000000000000099", "username": "Release Bot", "avatar": null,
            "discriminator": "0000", "bot": true}"#,
    );

    assert_eq!(user.global_name, None);
    assert_eq!(user.avatar, None);
    assert_eq!(user.primary_guild, None);
    assert_eq!(user.public_flags, 0);
    assert!(user.bot);
}

#[test]
fn current_user_parses() {
    let me: CurrentUser = parse(include_str!("fixtures/current_user.json"));

    assert_eq!(me.user.id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(me.user.username, "akari_tester");
    assert_eq!(me.premium_type, PremiumType::Tier2);
    assert_eq!(me.nsfw_allowed, Some(true));
    assert!(me.mfa_enabled);
    assert!(me.verified);
    assert_eq!(me.flags, 96);
}

#[test]
fn current_user_with_only_required_fields_parses() {
    let me: CurrentUser = parse(r#"{"id": "100000000000000001", "username": "akari_tester"}"#);

    assert_eq!(me.premium_type, PremiumType::None);
    assert_eq!(me.nsfw_allowed, None);
    assert!(!me.verified);
    assert_eq!(me.flags, 0);
}

#[test]
fn guild_text_channel_parses() {
    let channel: Channel = parse(include_str!("fixtures/channel_guild_text.json"));

    assert_eq!(channel.id, Snowflake::new(300_000_000_000_000_002));
    assert_eq!(channel.kind, ChannelType::GuildText);
    assert_eq!(
        channel.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    assert_eq!(
        channel.parent_id,
        Some(Snowflake::new(300_000_000_000_000_001))
    );
    assert_eq!(channel.name.as_deref(), Some("general"));
    assert_eq!(channel.topic.as_deref(), Some("Anything goes"));
    assert_eq!(channel.position, Some(1));
    assert_eq!(channel.rate_limit_per_user, Some(2));
    assert_eq!(
        channel.last_pin_timestamp.map(Timestamp::unix_millis),
        Some(1_676_625_748_000)
    );
    assert_eq!(
        channel.permission_overwrites,
        [
            PermissionOverwrite {
                id: Snowflake::new(200_000_000_000_000_001),
                kind: OverwriteType::Role,
                allow: Permissions(0),
                deny: Permissions(2048),
            },
            PermissionOverwrite {
                id: Snowflake::new(100_000_000_000_000_002),
                kind: OverwriteType::Member,
                allow: Permissions(2048),
                deny: Permissions(0),
            },
        ]
    );
}

#[test]
fn dm_channel_parses() {
    let channel: Channel = parse(include_str!("fixtures/channel_dm.json"));

    assert_eq!(channel.kind, ChannelType::Dm);
    assert_eq!(channel.guild_id, None);
    assert_eq!(channel.name, None);
    assert_eq!(channel.position, None);
    assert_eq!(channel.recipients.len(), 1);
    assert_eq!(channel.recipients[0].username, "mira");
    assert!(channel.recipient_ids.is_empty());
    assert!(!channel.is_message_request);
}

#[test]
fn thread_parses() {
    let channel: Channel = parse(include_str!("fixtures/channel_thread.json"));

    assert_eq!(channel.kind, ChannelType::PublicThread);
    assert_eq!(
        channel.parent_id,
        Some(Snowflake::new(300_000_000_000_000_002))
    );
    assert_eq!(
        channel.owner_id,
        Some(Snowflake::new(100_000_000_000_000_002))
    );
    assert_eq!(channel.message_count, Some(12));
    let metadata = channel.thread_metadata.unwrap();
    assert!(!metadata.archived);
    assert_eq!(metadata.auto_archive_duration, 1440);
    assert_eq!(metadata.archive_timestamp.unix_millis(), 1_709_281_800_250);
    assert_eq!(metadata.invitable, None);
}

#[test]
fn forum_last_message_id_holds_a_thread_id() {
    let forum: Channel = parse(
        r#"{"id": "300000000000000030", "type": 15, "last_message_id": "300000000000000031"}"#,
    );
    let last: Option<Snowflake<GenericMarker>> = forum.last_message_id;

    assert_eq!(forum.kind, ChannelType::GuildForum);
    assert_eq!(
        last.map(Snowflake::cast::<ChannelMarker>),
        Some(Snowflake::new(300_000_000_000_000_031))
    );
}

#[test]
fn unknown_channel_type_is_kept() {
    let channel: Channel = parse(r#"{"id": "300000000000000099", "type": 99}"#);

    assert_eq!(channel.kind, ChannelType::Unknown(99));
}

#[test]
fn guild_properties_parse() {
    let guild: Guild = parse(include_str!("fixtures/guild.json"));

    assert_eq!(guild.id, Snowflake::new(200_000_000_000_000_001));
    assert_eq!(guild.name, "Akari Lab");
    assert_eq!(
        guild.owner_id,
        Some(Snowflake::new(100_000_000_000_000_001))
    );
    assert_eq!(
        guild.features,
        [
            "COMMUNITY",
            "NEWS",
            "THREADS_ENABLED",
            "SOME_FUTURE_FEATURE"
        ]
    );
    assert_eq!(guild.afk_timeout, Some(300));
    assert_eq!(
        guild.system_channel_id,
        Some(Snowflake::new(300_000_000_000_000_002))
    );
    assert_eq!(guild.preferred_locale, "en-US");
    assert_eq!(
        guild.default_message_notifications,
        MessageNotificationLevel::OnlyMentions
    );
    assert_eq!(guild.nsfw_level, NsfwLevel::Default);
    assert_eq!(guild.premium_tier, PremiumTier::Tier1);
}

#[test]
fn guild_with_only_required_fields_parses() {
    let guild: Guild = parse(r#"{"id": "200000000000000001", "name": "Akari Lab"}"#);

    assert_eq!(guild.owner_id, None);
    assert_eq!(guild.afk_timeout, None);
    assert_eq!(guild.preferred_locale, "en-US");
    assert_eq!(
        guild.default_message_notifications,
        MessageNotificationLevel::AllMessages
    );
    assert_eq!(guild.nsfw_level, NsfwLevel::Default);
    assert_eq!(guild.premium_tier, PremiumTier::None);
    assert!(guild.features.is_empty());
}

#[test]
fn role_parses() {
    let role: Role = parse(include_str!("fixtures/role.json"));

    assert_eq!(role.id, Snowflake::new(500_000_000_000_000_002));
    assert_eq!(role.name, "Moderators");
    assert_eq!(role.permissions, Permissions(1_099_511_627_775));
    assert_eq!(role.position, 3);
    assert!(role.hoist);
    assert_eq!(role.unicode_emoji.as_deref(), Some("\u{2B50}"));
    assert_eq!(
        role.colors,
        Some(RoleColors {
            primary_color: 3_447_003,
            secondary_color: Some(16_759_788),
            tertiary_color: None,
        })
    );
}

#[test]
fn role_with_only_required_fields_parses() {
    let role: Role =
        parse(r#"{"id": "200000000000000001", "position": 0, "permissions": "104324673"}"#);

    assert_eq!(role.name, "");
    assert_eq!(role.colors, None);
    assert!(!role.hoist);
    assert!(!role.managed);
    assert!(!role.mentionable);
    assert_eq!(role.permissions, Permissions(104_324_673));
}

#[test]
fn guild_member_parses() {
    let member: GuildMember = parse(include_str!("fixtures/guild_member.json"));

    assert_eq!(
        member.user.map(|user| user.id),
        Some(Snowflake::new(100_000_000_000_000_002))
    );
    assert_eq!(member.user_id, None);
    assert_eq!(member.nick.as_deref(), Some("Mimi"));
    assert_eq!(member.roles, [Snowflake::new(500_000_000_000_000_002)]);
    assert_eq!(
        member.joined_at.map(Timestamp::unix_millis),
        Some(1_704_110_400_000)
    );
    assert_eq!(
        member.premium_since.map(Timestamp::unix_millis),
        Some(1_709_281_800_250)
    );
    assert_eq!(member.communication_disabled_until, None);
    assert_eq!(member.flags, 2);
}

#[test]
fn deduplicated_member_parses() {
    let member: GuildMember = parse(
        r#"{"user_id": "100000000000000001", "roles": [],
            "joined_at": "2024-01-01T12:00:00.000000+00:00", "flags": 0}"#,
    );

    assert_eq!(member.user, None);
    assert_eq!(
        member.user_id,
        Some(Snowflake::new(100_000_000_000_000_001))
    );
}

#[test]
fn message_parses() {
    let message: Message = parse(include_str!("fixtures/message.json"));

    assert_eq!(message.id, Snowflake::new(400_000_000_000_000_001));
    assert_eq!(message.channel_id, Snowflake::new(300_000_000_000_000_002));
    assert_eq!(message.kind, MessageType::Default);
    assert_eq!(message.author.username, "mira");
    assert_eq!(
        message.content,
        "Release notes are up <@100000000000000001> \u{1F389}\nSee the embed."
    );
    assert_eq!(message.timestamp.unix_millis(), 1_709_281_800_250);
    assert_eq!(message.edited_timestamp, None);
    assert_eq!(
        message.mentions[0].id,
        Snowflake::new(100_000_000_000_000_001)
    );
    assert_eq!(message.guild_id, None);
    assert_eq!(message.member, None);
    assert_eq!(message.referenced_message, None);

    let attachment = &message.attachments[0];
    assert_eq!(attachment.filename, "screenshot.png");
    assert_eq!(attachment.size, 48_213);
    assert_eq!(
        (attachment.width, attachment.height),
        (Some(1280), Some(720))
    );
    assert_eq!(attachment.content_type.as_deref(), Some("image/png"));

    let embed = &message.embeds[0];
    assert_eq!(embed.kind.as_deref(), Some("rich"));
    assert_eq!(embed.color, Some(5_793_266));
    assert_eq!(
        embed.footer.as_ref().map(|footer| footer.text.as_str()),
        Some("Akari")
    );
    assert_eq!(
        embed.thumbnail.as_ref().and_then(|media| media.width),
        Some(128)
    );
    assert_eq!(embed.fields.len(), 2);
    assert!(embed.fields[0].inline);
    assert!(!embed.fields[1].inline);

    let [thumbs_up, custom] = &message.reactions[..] else {
        panic!("expected two reactions, got {:?}", message.reactions);
    };
    assert_eq!(
        thumbs_up.emoji,
        PartialEmoji {
            id: None,
            name: Some("\u{1F44D}".to_owned()),
            animated: false,
        }
    );
    assert_eq!(
        thumbs_up.count_details,
        ReactionCountDetails {
            normal: 2,
            burst: 1
        }
    );
    assert!(thumbs_up.me);
    assert_eq!(
        custom.emoji.id,
        Some(Snowflake::new(800_000_000_000_000_001))
    );
    assert!(custom.emoji.animated);

    assert_eq!(message.sticker_items[0].format_type, StickerFormatType::Png);
}

#[test]
fn reply_parses() {
    let message: Message = parse(include_str!("fixtures/message_reply.json"));

    assert_eq!(message.kind, MessageType::Reply);
    assert_eq!(
        message.edited_timestamp.map(Timestamp::unix_millis),
        Some(1_709_281_920_000)
    );
    let reference = message.message_reference.unwrap();
    assert_eq!(reference.kind, MessageReferenceType::Default);
    assert_eq!(
        reference.message_id,
        Some(Snowflake::new(400_000_000_000_000_001))
    );
    let replied_to = message.referenced_message.unwrap().unwrap();
    assert_eq!(replied_to.content, "Release notes are up");
}

#[test]
fn reply_to_deleted_message_parses() {
    let mut value: Value = parse(include_str!("fixtures/message_reply.json"));
    value["referenced_message"] = Value::Null;
    let message: Message = parse(&value.to_string());

    assert_eq!(message.referenced_message, Some(None));
}

#[test]
fn reply_without_referenced_message_parses() {
    let mut value: Value = parse(include_str!("fixtures/message_reply.json"));
    value.as_object_mut().unwrap().remove("referenced_message");
    let message: Message = parse(&value.to_string());

    assert_eq!(message.referenced_message, None);
    assert!(message.message_reference.is_some());
}

#[test]
fn gateway_message_parses() {
    let message: Message = parse(include_str!("fixtures/message_create.json"));

    assert_eq!(
        message.guild_id,
        Some(Snowflake::new(200_000_000_000_000_001))
    );
    let member = message.member.unwrap();
    assert_eq!(member.user, None);
    assert_eq!(member.nick.as_deref(), Some("Mimi"));
    assert_eq!(
        message.mentions[0].id,
        Snowflake::new(100_000_000_000_000_001)
    );
}

#[test]
fn unknown_message_type_is_kept() {
    let mut value: Value = parse(include_str!("fixtures/message.json"));
    value["type"] = 999.into();
    let message: Message = parse(&value.to_string());

    assert_eq!(message.kind, MessageType::Unknown(999));
}

#[test]
fn broken_list_entries_drop_only_themselves() {
    let mut value: Value = parse(include_str!("fixtures/message.json"));
    value["embeds"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type": "rich", "color": "red"}));
    value["attachments"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id": "600000000000000002"}));
    value["mentions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id": "100000000000000009"}));
    value["mention_roles"] = serde_json::json!(["500000000000000002", "not-an-id"]);
    value["sticker_items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id": "1"}));
    value["reactions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"emoji": {}}));

    let message: Message = parse(&value.to_string());

    assert_eq!(message.embeds.len(), 1);
    assert_eq!(message.embeds[0].title.as_deref(), Some("Akari 0.1"));
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.mentions.len(), 1);
    assert_eq!(
        message.mention_roles,
        [Snowflake::new(500_000_000_000_000_002)]
    );
    assert_eq!(message.sticker_items.len(), 1);
    assert_eq!(message.reactions.len(), 2);
}
