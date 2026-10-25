use std::sync::Arc;

use akari_core::model::{
    ChannelId, ChannelType, EmojiId, MessageId, RoleId, StickerFormatType, StickerId, UserId,
};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, GUILD, VOICE, events_until, fixture, online};
use super::support::{USER, token};
use crate::account::{Account, MessageLoad};
use crate::formatted::{
    ChannelLink, FormattedText, GuildNavigation, MentionName, SpanKind, TextBlockKind,
    TextContainer, TextSpan, TimestampStyle, formatted_text,
};
use crate::images::ImageKey;
use crate::names::Names;
use crate::records::{Message, Sticker};

const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";
const MIRA: UserId = UserId::new(100_000_000_000_000_002);
const SECRET: ChannelId = ChannelId::new(300_000_000_000_000_004);
const MODERATORS: RoleId = RoleId::new(500_000_000_000_000_002);

fn shown(content: &str, start: u32, end: u32) -> String {
    let units: Vec<u16> = content.encode_utf16().collect();
    String::from_utf16(&units[start as usize..end as usize]).unwrap()
}

fn kinds(text: &FormattedText) -> Vec<SpanKind> {
    text.blocks
        .iter()
        .flat_map(|block| block.spans.iter().map(|span| span.kind.clone()))
        .collect()
}

fn span_showing<'a>(content: &str, text: &'a FormattedText, wanted: &str) -> &'a TextSpan {
    text.blocks
        .iter()
        .flat_map(|block| &block.spans)
        .find(|span| match span.kind {
            SpanKind::Text { start, end } | SpanKind::Code { start, end } => {
                shown(content, start, end) == wanted
            }
            _ => false,
        })
        .unwrap_or_else(|| panic!("no span shows {wanted:?}: {text:?}"))
}

#[test]
fn every_span_kind_crosses() {
    let content = "**b** *i* __u__ ~~s~~ `c` <@1> <@&2> <#3> @everyone @here </cmd sub:123456789012345678> \
                   <a:wave:5> <t:1800000000:R> <id:customize> <id:linked-roles:6>\nx";
    let text = formatted_text(content.to_owned());

    let styled = |wanted: &str| span_showing(content, &text, wanted);
    assert!(styled("b").bold && !styled("b").italic);
    assert!(styled("i").italic);
    assert!(styled("u").underline);
    assert!(styled("s").strike);
    assert!(matches!(styled("c").kind, SpanKind::Code { .. }));
    let kinds = kinds(&text);
    for expected in [
        SpanKind::User {
            id: UserId::new(1),
            name: MentionName::NotLoaded,
        },
        SpanKind::Role {
            id: RoleId::new(2),
            name: MentionName::NotLoaded,
            color: 0,
        },
        SpanKind::Channel {
            id: ChannelId::new(3),
            name: MentionName::NotLoaded,
            kind: None,
        },
        SpanKind::Everyone,
        SpanKind::Here,
        SpanKind::Command {
            name: "cmd sub".to_owned(),
            id: 123_456_789_012_345_678,
        },
        SpanKind::Emoji {
            name: "wave".to_owned(),
            id: 5,
            animated: true,
            key: ImageKey::Emoji {
                emoji_id: EmojiId::new(5),
                animated: true,
            },
        },
        SpanKind::Timestamp {
            seconds: 1_800_000_000,
            style: TimestampStyle::Relative,
        },
        SpanKind::Navigation {
            target: GuildNavigation::Customize,
        },
        SpanKind::Navigation {
            target: GuildNavigation::LinkedRoles {
                role_id: Some(RoleId::new(6)),
            },
        },
        SpanKind::Break,
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
}

#[test]
fn blocks_containers_spoilers_and_links_map_one_to_one() {
    let content = "> - a\n# h\n-# s\n```py\nx\n```\n||sp|| [t](https://example.com/ \"T\") \
                   https://a.co <https://b.co> https://discord.com/channels/@me/2/3";
    let text = formatted_text(content.to_owned());

    let blocks: Vec<_> = text.blocks.iter().map(|block| &block.kind).collect();
    assert_eq!(
        blocks,
        [
            &TextBlockKind::Paragraph,
            &TextBlockKind::Heading { level: 1 },
            &TextBlockKind::Subtext,
            &TextBlockKind::Code {
                language: Some("py".to_owned())
            },
            &TextBlockKind::Paragraph,
        ]
    );
    assert_eq!(
        text.blocks[0].containers,
        [
            TextContainer::Quote,
            TextContainer::ListItem {
                ordered: false,
                number: 1,
                marker: true
            }
        ]
    );
    let code = &text.blocks[3];
    assert_eq!(shown(content, code.start, code.end), "```py\nx\n```");
    assert!(
        matches!(code.spans[0].kind, SpanKind::Code { start, end } if shown(content, start, end) == "x")
    );
    assert_eq!(span_showing(content, &text, "sp").spoiler, Some(0));
    assert_eq!(span_showing(content, &text, "t").link, Some(0));

    let links: Vec<_> = text
        .links
        .iter()
        .map(|link| {
            (
                link.url.as_str(),
                link.masked,
                link.title.as_deref(),
                link.suppress_embed,
            )
        })
        .collect();
    assert_eq!(
        links,
        [
            ("https://example.com/", true, Some("T"), false),
            ("https://a.co/", false, None, false),
            ("https://b.co/", false, None, true),
            ("https://discord.com/channels/@me/2/3", false, None, false),
        ]
    );
    assert_eq!(text.links[0].channel, None);
    assert_eq!(
        text.links[3].channel,
        Some(ChannelLink {
            guild_id: None,
            channel_id: ChannelId::new(2),
            message_id: Some(MessageId::new(3)),
            name: MentionName::NotLoaded,
            kind: None,
        })
    );
    assert!(!text.jumbo);
    assert!(formatted_text("😀 🎉".to_owned()).jumbo);
}

#[test]
fn utf16_offsets_never_split_a_surrogate_pair() {
    let content = "😀 **日本** a\u{301} *🎉x* <@1> `👍` ||🙂|| [🔗a](https://example.com)\n𝒜";
    let units: Vec<u16> = content.encode_utf16().collect();
    let text = formatted_text(content.to_owned());
    let document = akari_markdown::parse(content);

    let on_boundary = |offset: u32| {
        units
            .get(offset as usize)
            .is_none_or(|unit| !(0xDC00..0xE000).contains(unit))
    };
    for (block, parsed) in text.blocks.iter().zip(document.blocks()) {
        assert!(on_boundary(block.start) && on_boundary(block.end));
        assert_eq!(block.spans.len(), parsed.inlines.len());
        for (span, inline) in block.spans.iter().zip(parsed.inlines.iter()) {
            assert!(on_boundary(span.start) && on_boundary(span.end), "{span:?}");
            assert_eq!(
                shown(content, span.start, span.end),
                &content[inline.source.range()]
            );
            if let (
                SpanKind::Text { start, end } | SpanKind::Code { start, end },
                akari_markdown::InlineKind::Text(display)
                | akari_markdown::InlineKind::Code(display),
            ) = (&span.kind, inline.kind)
            {
                assert_eq!(shown(content, *start, *end), &content[display.range()]);
            }
        }
    }
    assert_eq!(text.blocks.len(), document.blocks().len());
}

#[test]
fn formatted_text_has_no_names() {
    let text = formatted_text("<@1> <@&2> <#3>".to_owned());
    let names: Vec<_> = kinds(&text)
        .into_iter()
        .filter_map(|kind| match kind {
            SpanKind::User { name, .. }
            | SpanKind::Role { name, .. }
            | SpanKind::Channel { name, .. } => Some(name),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec![MentionName::NotLoaded; 3]);
}

fn page_message(id: u64, content: &str, mentions: Value) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["id"] = id.to_string().into();
    message["content"] = content.into();
    message["mentions"] = mentions;
    message
}

fn mentioned(id: u64, username: &str, global_name: Option<&str>) -> Value {
    json!({"id": id.to_string(), "username": username, "global_name": global_name,
           "avatar": null, "discriminator": "0", "public_flags": 0})
}

// The fixture's user owns guild 1 and holds an administrator role; here they're a plain member
// who can't see #secret.
fn with_a_secret_channel(ready: &mut Value) {
    let guild = &mut ready["guilds"][0];
    guild["properties"]["owner_id"] = "100000000000000099".into();
    let mut secret = guild["channels"][1].clone();
    secret["id"] = SECRET.get().to_string().into();
    secret["name"] = "secret".into();
    secret["permission_overwrites"] = json!([
        {"id": GUILD.get().to_string(), "type": 0, "allow": "0", "deny": "1024"}
    ]);
    guild["channels"].as_array_mut().unwrap().push(secret);
    ready["merged_members"][0][0]["roles"] = json!([]);
}

struct Loaded {
    account: Arc<Account>,
    messages: Vec<Message>,
    _server: MockServer,
    _ws: WebSocketStream<TcpStream>,
}

async fn loaded(messages: Vec<Value>, edit: impl FnOnce(&mut Value)) -> Loaded {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(messages)))
        .mount(&server)
        .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready_with(edit).await;
    events_until(&subscription, online).await;
    account
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 }, None)
        .await
        .unwrap();
    let store = account.store();
    let ids = store.window(GENERAL).unwrap().message_ids;
    let messages = store.messages(GENERAL, ids);
    Loaded {
        account,
        messages,
        _server: server,
        _ws: ws,
    }
}

fn mention_names(message: &Message) -> Vec<MentionName> {
    kinds(&message.formatted)
        .into_iter()
        .filter_map(|kind| match kind {
            SpanKind::User { name, .. }
            | SpanKind::Role { name, .. }
            | SpanKind::Channel { name, .. } => Some(name),
            _ => None,
        })
        .collect()
}

fn known(name: &str) -> MentionName {
    MentionName::Known {
        name: name.to_owned(),
    }
}

#[tokio::test]
async fn user_names_come_from_the_message() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "<@100000000000000020> <@100000000000000021>",
        json!([
            mentioned(100_000_000_000_000_020, "kai", Some("Kai")),
            mentioned(100_000_000_000_000_021, "noor", None)
        ]),
    )];
    let loaded = loaded(page, |_| {}).await;

    assert_eq!(
        mention_names(&loaded.messages[0]),
        [known("Kai"), known("noor")]
    );
    loaded.account.close();
}

#[tokio::test]
async fn a_dm_recipient_is_found_in_the_store() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "<@100000000000000002> <@100000000000000003> <@100000000000000001> <@100000000000000099>",
        json!([]),
    )];
    let loaded = loaded(page, |_| {}).await;

    assert_eq!(
        mention_names(&loaded.messages[0]),
        [
            known("Mira"),
            known("jun"),
            known("Akari Tester"),
            MentionName::Missing
        ]
    );
    loaded.account.close();
}

#[tokio::test]
async fn names_before_ready_are_not_loaded() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "<@100000000000000099> <@&500000000000000002> <#300000000000000003>",
        json!([]),
    )];
    let loaded = loaded(page, |_| {}).await;
    let before_ready = FakeGateway::start()
        .await
        .client()
        .account(token("t"), USER)
        .unwrap();
    let window = loaded.account.store().core().messages(GENERAL).unwrap();
    let message = &window.messages[0];

    let store = before_ready.store();
    let names = Names::new(store.core(), GENERAL);
    assert_eq!(
        names.user(UserId::new(100_000_000_000_000_099), message),
        MentionName::NotLoaded
    );
    assert_eq!(names.role(MODERATORS), (MentionName::NotLoaded, 0));
    assert_eq!(names.channel(VOICE), (MentionName::NotLoaded, None));
    loaded.account.close();
}

#[tokio::test]
async fn role_names_and_colors_cross() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "<@&500000000000000002> <@&200000000000000001> <@&999>",
        json!([]),
    )];
    let loaded = loaded(page, |_| {}).await;

    let roles: Vec<_> = kinds(&loaded.messages[0].formatted)
        .into_iter()
        .filter_map(|kind| match kind {
            SpanKind::Role { name, color, .. } => Some((name, color)),
            _ => None,
        })
        .collect();
    assert_eq!(
        roles,
        [
            (known("Moderators"), 3_447_003),
            (known("@everyone"), 0),
            (MentionName::Missing, 0)
        ]
    );
    loaded.account.close();
}

#[tokio::test]
async fn channel_names_kinds_and_access_cross() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "<#300000000000000003> <#300000000000000004> <#999> <#300000000000000010>",
        json!([]),
    )];
    let loaded = loaded(page, with_a_secret_channel).await;

    let channels: Vec<_> = kinds(&loaded.messages[0].formatted)
        .into_iter()
        .filter_map(|kind| match kind {
            SpanKind::Channel { name, kind, .. } => Some((name, kind)),
            _ => None,
        })
        .collect();
    assert_eq!(
        channels,
        [
            (known("Voice"), Some(ChannelType::GuildVoice)),
            (MentionName::NoAccess, Some(ChannelType::GuildText)),
            (MentionName::Missing, None),
            (known("Mira"), Some(ChannelType::Dm)),
        ]
    );
    loaded.account.close();
}

#[tokio::test]
async fn discord_links_carry_typed_ids_and_channel_names() {
    let page = vec![page_message(
        400_000_000_000_000_010,
        "https://discord.com/channels/200000000000000001/300000000000000002/400000000000000099 \
         https://discord.com/channels/200000000000000001/300000000000000004 \
         https://discord.com/channels/@me/999",
        json!([]),
    )];
    let loaded = loaded(page, with_a_secret_channel).await;

    let channels: Vec<_> = loaded.messages[0]
        .formatted
        .links
        .iter()
        .map(|link| link.channel.clone())
        .collect();
    assert_eq!(
        channels,
        [
            Some(ChannelLink {
                guild_id: Some(GUILD),
                channel_id: GENERAL,
                message_id: Some(MessageId::new(400_000_000_000_000_099)),
                name: known("general"),
                kind: Some(ChannelType::GuildText),
            }),
            Some(ChannelLink {
                guild_id: Some(GUILD),
                channel_id: SECRET,
                message_id: None,
                name: MentionName::NoAccess,
                kind: Some(ChannelType::GuildText),
            }),
            Some(ChannelLink {
                guild_id: None,
                channel_id: ChannelId::new(999),
                message_id: None,
                name: MentionName::Missing,
                kind: None,
            }),
        ]
    );
    loaded.account.close();
}

#[tokio::test]
async fn messages_carry_mentions_me() {
    let page = vec![
        page_message(
            400_000_000_000_000_010,
            "<@100000000000000001>",
            json!([mentioned(USER.get(), "akari_tester", Some("Akari Tester"))]),
        ),
        page_message(400_000_000_000_000_011, "<@100000000000000002>", json!([])),
    ];
    let loaded = loaded(page, |_| {}).await;

    let flags: Vec<_> = loaded
        .messages
        .iter()
        .map(|message| message.mentions_me)
        .collect();
    assert_eq!(flags, [true, false]);
    assert_eq!(loaded.messages[1].author.id, MIRA);
    loaded.account.close();
}

#[tokio::test]
async fn stickers_cross_with_their_keys() {
    let mut message = page_message(400_000_000_000_000_012, "", json!([]));
    message["sticker_items"] = json!([
        {"id": "700000000000000001", "name": "wave", "format_type": 2},
        {"id": "700000000000000002", "name": "dance", "format_type": 3}
    ]);
    let loaded = loaded(vec![message], |_| {}).await;

    assert_eq!(
        loaded.messages[0].stickers,
        vec![
            Sticker {
                id: StickerId::new(700_000_000_000_000_001),
                name: "wave".to_owned(),
                format: StickerFormatType::Apng,
                key: ImageKey::Sticker {
                    sticker_id: StickerId::new(700_000_000_000_000_001),
                    format: StickerFormatType::Apng,
                },
            },
            Sticker {
                id: StickerId::new(700_000_000_000_000_002),
                name: "dance".to_owned(),
                format: StickerFormatType::Lottie,
                key: ImageKey::Sticker {
                    sticker_id: StickerId::new(700_000_000_000_000_002),
                    format: StickerFormatType::Lottie,
                },
            },
        ]
    );
    loaded.account.close();
}
