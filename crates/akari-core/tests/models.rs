use akari_core::model::{CurrentUser, Permissions, PremiumType, Snowflake, Timestamp, User};
use serde::de::DeserializeOwned;

#[track_caller]
fn parse<T: DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|err| panic!("failed to parse: {err}"))
}

#[test]
fn snowflakes_parse_from_strings_and_integers() {
    assert_eq!(
        parse::<Snowflake>(r#""100000000000000001""#),
        Snowflake(100_000_000_000_000_001)
    );
    assert_eq!(parse::<Snowflake>("373"), Snowflake(373));
    assert_eq!(
        parse::<Snowflake>(r#""18446744073709551615""#),
        Snowflake(u64::MAX)
    );
}

#[test]
fn values_that_are_not_snowflakes_are_rejected() {
    for json in [r#""abc""#, r#""""#, "-1", "1.5", "null"] {
        assert!(
            serde_json::from_str::<Snowflake>(json).is_err(),
            "{json} parsed"
        );
    }
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

    assert_eq!(user.id, Snowflake(100_000_000_000_000_002));
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
    assert_eq!(decoration.sku_id, Snowflake(900_000_000_000_000_001));
    assert_eq!(decoration.expires_at, None);
    let tag = user.primary_guild.unwrap();
    assert_eq!(
        tag.identity_guild_id,
        Some(Snowflake(200_000_000_000_000_001))
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

    assert_eq!(me.user.id, Snowflake(100_000_000_000_000_001));
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
