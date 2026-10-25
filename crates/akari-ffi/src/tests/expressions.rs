use akari_core::model::{EmojiId, GuildId, StickerFormatType, StickerId, StickerPackId};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GUILD, events_until, online};
use super::support::{USER, token};
use crate::formatted::emoji_shortcode;
use crate::images::ImageKey;
use crate::records::{EmojiSource, Nitro, Sticker, StickerOrigin};

const EMOJI: u64 = 700_000_000_000_000_001;
const STICKER: u64 = 710_000_000_000_000_001;
const PACK: u64 = 720_000_000_000_000_001;
const SOURCE: u64 = 200_000_000_000_000_050;
const ICON: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn unicode_emoji_names_cross() {
    assert_eq!(
        emoji_shortcode("👍🏽".to_owned()).as_deref(),
        Some("thumbsup_tone3")
    );
    assert_eq!(emoji_shortcode("🇩🇪".to_owned()).as_deref(), Some("flag_de"));
    assert_eq!(emoji_shortcode("hello".to_owned()), None);
}

fn sticker_object(id: u64, extra: Value) -> Value {
    let mut sticker = json!({
        "id": id.to_string(), "name": "wave", "tags": "wave", "type": 2, "format_type": 2,
        "description": null, "available": true, "guild_id": SOURCE.to_string()
    });
    for (key, value) in extra.as_object().unwrap() {
        sticker[key] = value.clone();
    }
    sticker
}

async fn mock(server: &MockServer, route: String, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v9/{route}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_lookups_and_the_emoji_index_cross() {
    let server = MockServer::start().await;
    mock(
        &server,
        format!("emojis/{EMOJI}/source"),
        json!({
            "type": "GUILD", "application": null,
            "guild": {"id": SOURCE.to_string(), "name": "Emoji Hub", "icon": ICON,
                      "features": ["DISCOVERABLE"], "approximate_presence_count": 12,
                      "emojis": [{"id": EMOJI.to_string(), "name": "wave", "animated": true}]}
        }),
    )
    .await;
    mock(
        &server,
        format!("stickers/{STICKER}"),
        sticker_object(STICKER, json!({})),
    )
    .await;
    mock(
        &server,
        format!("stickers/{STICKER}/guild"),
        json!({
            "id": SOURCE.to_string(), "name": "Emoji Hub", "icon": null, "features": ["VERIFIED"],
            "approximate_presence_count": null, "stickers": [sticker_object(STICKER, json!({}))]
        }),
    )
    .await;
    mock(
        &server,
        format!("sticker-packs/{PACK}"),
        json!({
            "id": PACK.to_string(), "name": "Pack", "sku_id": "1",
            "stickers": [sticker_object(STICKER, json!({"type": 1, "guild_id": null}))]
        }),
    )
    .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway
        .serve_ready_with(|ready| {
            ready["guilds"][0]["emojis"] =
                json!([{"id": "700000000000000009", "name": "own", "animated": false}]);
            ready["user"]["premium_type"] = 3.into();
        })
        .await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(
        store.emoji_guild(EmojiId::new(700_000_000_000_000_009)),
        Some(GUILD)
    );
    assert_eq!(store.emoji_guild(EmojiId::new(EMOJI)), None);
    assert_eq!(store.nitro(), Nitro::Basic);

    let EmojiSource::Guild { guild } = account.emoji_source(EmojiId::new(EMOJI)).await.unwrap()
    else {
        panic!("a guild source");
    };
    assert_eq!(guild.id, GuildId::new(SOURCE));
    assert_eq!(guild.name, "Emoji Hub");
    assert!(guild.discoverable);
    assert_eq!(guild.presence_count, Some(12));
    assert_eq!(
        guild.icon,
        Some(ImageKey::GuildIcon {
            guild_id: GuildId::new(SOURCE),
            hash: ICON.to_owned()
        })
    );
    assert_eq!(guild.emojis.len(), 1);
    assert_eq!(
        guild.emojis[0].key,
        ImageKey::Emoji {
            emoji_id: EmojiId::new(EMOJI),
            animated: true
        }
    );

    let wave = Sticker {
        id: StickerId::new(STICKER),
        name: "wave".to_owned(),
        format: StickerFormatType::Apng,
        key: ImageKey::Sticker {
            sticker_id: StickerId::new(STICKER),
            format: StickerFormatType::Apng,
        },
    };
    let details = account.sticker(StickerId::new(STICKER)).await.unwrap();
    assert_eq!(details.sticker, wave);
    assert_eq!(
        details.origin,
        StickerOrigin::Guild {
            guild_id: GuildId::new(SOURCE)
        }
    );

    let guild = account
        .sticker_guild(StickerId::new(STICKER))
        .await
        .unwrap();
    assert!(guild.verified && !guild.discoverable);
    assert_eq!(guild.icon, None);
    assert_eq!(guild.stickers, vec![wave.clone()]);

    let pack = account
        .sticker_pack(StickerPackId::new(PACK))
        .await
        .unwrap();
    assert_eq!(pack.id, StickerPackId::new(PACK));
    assert_eq!(pack.name, "Pack");
    assert_eq!(pack.stickers, vec![wave]);
    account.close();
}
