use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::media::ImageKey;
use crate::model::{EmojiId, GuildId, StickerFormatType, StickerId, StickerPackId};
use crate::state::{EmojiSource, StickerOrigin};

const EMOJI: u64 = 700_000_000_000_000_001;
const STICKER: u64 = 710_000_000_000_000_001;
const PACK: u64 = 720_000_000_000_000_001;
const SOURCE: u64 = 200_000_000_000_000_050;
const ICON: &str = "0123456789abcdef0123456789abcdef";

struct Online {
    account: Account,
    _server: MockServer,
    _subscription: Subscription,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_answering(route: &str, answer: ResponseTemplate) -> Online {
    online_answering_all(vec![(route.to_owned(), answer)]).await
}

async fn online_answering_all(answers: Vec<(String, ResponseTemplate)>) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    for (route, answer) in answers {
        Mock::given(method("GET"))
            .and(path(format!("/api/v9/{route}")))
            .respond_with(answer)
            .mount(&server)
            .await;
    }
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        _server: server,
        _subscription: subscription,
        _connection: connection,
        _fake: fake,
    }
}

fn source_guild(extra: Value) -> Value {
    let mut guild = json!({
        "id": SOURCE.to_string(), "name": "Emoji Hub", "icon": ICON, "description": "emoji",
        "features": ["DISCOVERABLE", "VERIFIED", "COMMUNITY"], "premium_tier": 2,
        "premium_subscription_count": 9, "approximate_member_count": 5000,
        "approximate_presence_count": 1234
    });
    for (key, value) in extra.as_object().unwrap() {
        guild[key] = value.clone();
    }
    guild
}

fn sticker_object(id: u64, extra: Value) -> Value {
    let mut sticker = json!({
        "id": id.to_string(), "name": "wave", "tags": "wave", "type": 2, "format_type": 1,
        "description": null, "available": true, "guild_id": SOURCE.to_string()
    });
    for (key, value) in extra.as_object().unwrap() {
        sticker[key] = value.clone();
    }
    sticker
}

// GET /emojis/{id}/source: the server an emoji is from, with what the popout shows.
#[tokio::test]
async fn an_emojis_source_server_crosses_with_its_emoji() {
    let body = json!({
        "type": "GUILD", "application": null,
        "guild": source_guild(json!({"emojis": [
            {"id": EMOJI.to_string(), "name": "wave", "animated": false},
            {"id": "700000000000000002", "name": "dance", "animated": true},
            {"id": "nope"}
        ]}))
    });
    let online = online_answering(
        &format!("emojis/{EMOJI}/source"),
        ResponseTemplate::new(200).set_body_json(body),
    )
    .await;

    let source = online
        .account
        .emoji_source(EmojiId::new(EMOJI))
        .await
        .unwrap();

    let EmojiSource::Guild(guild) = source else {
        panic!("a guild source, got {source:?}");
    };
    assert_eq!(guild.id, GuildId::new(SOURCE));
    assert_eq!(&*guild.name, "Emoji Hub");
    assert!(guild.discoverable && guild.verified && !guild.partnered);
    assert_eq!(guild.presence_count, Some(1234));
    assert_eq!(
        guild.icon_key(),
        Some(ImageKey::GuildIcon {
            guild: GuildId::new(SOURCE),
            hash: crate::state::ImageHash::parse(ICON),
        })
    );
    let emojis: Vec<(u64, &str, bool)> = guild
        .emojis
        .iter()
        .map(|e| (e.id.get(), &*e.name, e.animated))
        .collect();
    assert_eq!(
        emojis,
        [
            (EMOJI, "wave", false),
            (700_000_000_000_000_002, "dance", true)
        ]
    );
    assert_eq!(
        guild.emojis[1].image_key(),
        ImageKey::Emoji {
            id: EmojiId::new(700_000_000_000_000_002),
            animated: true
        }
    );
}

#[tokio::test]
async fn an_application_emoji_names_its_app_and_a_404_is_an_error() {
    let online = online_answering(
        &format!("emojis/{EMOJI}/source"),
        ResponseTemplate::new(200).set_body_json(json!({
            "type": "APPLICATION", "guild": null,
            "application": {"id": "800000000000000001", "name": "Emoji Bot"}
        })),
    )
    .await;
    let source = online
        .account
        .emoji_source(EmojiId::new(EMOJI))
        .await
        .unwrap();
    assert!(matches!(source, EmojiSource::Application { ref name } if &**name == "Emoji Bot"));

    let missing = online_answering(
        &format!("emojis/{EMOJI}/source"),
        ResponseTemplate::new(404)
            .set_body_json(json!({"message": "Unknown Emoji", "code": 10014})),
    )
    .await;
    let err = missing
        .account
        .emoji_source(EmojiId::new(EMOJI))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            RequestError::Discord {
                status: 404,
                code: 10014,
                ..
            }
        ),
        "{err:?}"
    );
}

// GET /stickers/{id}: a guild sticker names its server, a standard one its pack.
#[tokio::test]
async fn a_sticker_names_its_server_or_its_pack() {
    let online = online_answering(
        &format!("stickers/{STICKER}"),
        ResponseTemplate::new(200).set_body_json(sticker_object(STICKER, json!({}))),
    )
    .await;
    let details = online
        .account
        .sticker(StickerId::new(STICKER))
        .await
        .unwrap();
    assert_eq!(details.sticker.id, StickerId::new(STICKER));
    assert_eq!(&*details.sticker.name, "wave");
    assert_eq!(details.sticker.format, StickerFormatType::Png);
    assert_eq!(details.origin, StickerOrigin::Guild(GuildId::new(SOURCE)));

    let standard = online_answering(
        &format!("stickers/{STICKER}"),
        ResponseTemplate::new(200).set_body_json(sticker_object(
            STICKER,
            json!({"type": 1, "guild_id": null, "pack_id": PACK.to_string(), "format_type": 3}),
        )),
    )
    .await;
    let details = standard
        .account
        .sticker(StickerId::new(STICKER))
        .await
        .unwrap();
    assert_eq!(
        details.origin,
        StickerOrigin::Pack(StickerPackId::new(PACK))
    );
    assert_eq!(details.sticker.format, StickerFormatType::Lottie);
}

// GET /stickers/{id}/guild: the discoverable server with its stickers for "Show More".
#[tokio::test]
async fn a_stickers_server_crosses_with_its_stickers() {
    let body = source_guild(json!({
        "features": ["DISCOVERABLE", "PARTNERED"],
        "stickers": [sticker_object(STICKER, json!({})), {"id": "broken"},
                     sticker_object(710_000_000_000_000_002, json!({"name": "dance", "format_type": 2}))]
    }));
    let online = online_answering(
        &format!("stickers/{STICKER}/guild"),
        ResponseTemplate::new(200).set_body_json(body),
    )
    .await;

    let guild = online
        .account
        .sticker_guild(StickerId::new(STICKER))
        .await
        .unwrap();

    assert_eq!(guild.id, GuildId::new(SOURCE));
    assert!(guild.discoverable && guild.partnered && !guild.verified);
    let names: Vec<&str> = guild.stickers.iter().map(|s| &*s.name).collect();
    assert_eq!(names, ["wave", "dance"]);
    assert_eq!(guild.stickers[1].format, StickerFormatType::Apng);
}

// GET /sticker-packs/{id}: the pack's name and its stickers.
#[tokio::test]
async fn a_sticker_pack_crosses_with_its_stickers() {
    let body = json!({
        "id": PACK.to_string(), "name": "Wumpus Beyond", "sku_id": "1", "description": "",
        "cover_sticker_id": STICKER.to_string(),
        "stickers": [
            sticker_object(STICKER, json!({"type": 1, "guild_id": null, "pack_id": PACK.to_string()})),
            sticker_object(710_000_000_000_000_003, json!({"type": 1, "guild_id": null, "name": "hi"}))
        ]
    });
    let online = online_answering(
        &format!("sticker-packs/{PACK}"),
        ResponseTemplate::new(200).set_body_json(body),
    )
    .await;

    let pack = online
        .account
        .sticker_pack(StickerPackId::new(PACK))
        .await
        .unwrap();

    assert_eq!(pack.id, StickerPackId::new(PACK));
    assert_eq!(&*pack.name, "Wumpus Beyond");
    let names: Vec<&str> = pack.stickers.iter().map(|s| &*s.name).collect();
    assert_eq!(names, ["wave", "hi"]);
}

// The official client gives an emoji's source 5 s (the test timing's limit is shorter); a slow
// one gives up, and the next lookup doesn't wait behind it.
#[tokio::test]
async fn a_slow_source_gives_up_and_the_next_doesnt_wait() {
    let other = 700_000_000_000_000_002_u64;
    let online = online_answering_all(vec![
        (
            format!("emojis/{EMOJI}/source"),
            ResponseTemplate::new(200)
                .set_body_json(json!({"type": "APPLICATION", "guild": null,
                                      "application": {"id": "1", "name": "Slow"}}))
                .set_delay(Duration::from_secs(5)),
        ),
        (
            format!("emojis/{other}/source"),
            ResponseTemplate::new(200).set_body_json(json!({"type": "APPLICATION", "guild": null,
                                      "application": {"id": "1", "name": "Quick"}})),
        ),
    ])
    .await;
    let started = std::time::Instant::now();

    let slow = online.account.emoji_source(EmojiId::new(EMOJI)).await;
    let quick = online.account.emoji_source(EmojiId::new(other)).await;

    assert!(
        matches!(&slow, Err(RequestError::Network(err)) if err.kind() == crate::TransportErrorKind::Timeout),
        "{slow:?}"
    );
    assert!(matches!(quick, Ok(EmojiSource::Application { ref name }) if &**name == "Quick"));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

// A 404 is the popout's answer, nothing more: the account stays online.
#[tokio::test]
async fn a_404_leaves_the_connection_alone() {
    let online = online_answering(
        &format!("stickers/{STICKER}/guild"),
        ResponseTemplate::new(404)
            .set_body_json(json!({"message": "Unknown Guild", "code": 10004})),
    )
    .await;

    assert!(
        online
            .account
            .sticker_guild(StickerId::new(STICKER))
            .await
            .is_err()
    );

    assert!(matches!(
        online.account.store().connection(),
        crate::state::ConnectionState::Online
    ));
    assert!(
        online
            .account
            .sticker_guild(StickerId::new(STICKER))
            .await
            .is_err(),
        "and asks again"
    );
}
