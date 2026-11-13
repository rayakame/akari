use std::sync::Arc;
use std::time::Duration;

use akari_core::model::{EmojiId, MessageId, UserId};
use akari_core::state;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, MESSAGE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::{Account, ReactionKind, ReactionLocation};
use crate::errors::{ReactionError, RequestError};
use crate::images::ImageKey;
use crate::records::ReactionEmoji;
use crate::subscription::{StoreEvent, StoreSubscription};

const MIRA: UserId = UserId::new(100_000_000_000_000_002);
const WAVE: EmojiId = EmojiId::new(700_000_000_000_000_001);
const BASE: &str = "/api/v9/channels/300000000000000002/messages/400000000000000003/reactions";
const HEART: &str = "❤\u{fe0f}";
const HEART_PATH: &str = "%E2%9D%A4%EF%B8%8F";

struct Reacting {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    server: MockServer,
    _ws: WebSocketStream<TcpStream>,
}

// Online with Mira's message in #general's list.
async fn reacting() -> Reacting {
    let server = MockServer::start().await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account.view_channel(GENERAL);
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message.as_object_mut().unwrap().remove("nonce");
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;
    events_until(&subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    Reacting {
        account,
        subscription,
        server,
        _ws: ws,
    }
}

impl Reacting {
    async fn answer(&self, answer: ResponseTemplate) {
        Mock::given(path_regex("/reactions"))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    async fn lines(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| {
                let query = request
                    .url
                    .query()
                    .map(|query| format!("?{query}"))
                    .unwrap_or_default();
                format!("{} {}{query}", request.method, request.url.path())
            })
            .collect()
    }

    fn mine(&self) -> Vec<(Option<String>, bool)> {
        self.account
            .store()
            .reactions(GENERAL, MESSAGE)
            .into_iter()
            .map(|reaction| (reaction.emoji.name, reaction.me))
            .collect()
    }
}

fn heart() -> ReactionEmoji {
    ReactionEmoji {
        id: None,
        name: Some(HEART.to_owned()),
        animated: false,
        key: None,
    }
}

fn wave() -> ReactionEmoji {
    ReactionEmoji {
        id: Some(WAVE),
        name: Some("wave".to_owned()),
        animated: true,
        key: Some(ImageKey::Emoji {
            emoji_id: WAVE,
            animated: true,
        }),
    }
}

fn refusal(status: u16, code: u32) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({"message": "No", "code": code}))
}

#[tokio::test]
async fn reacting_and_taking_it_back_cross_with_their_event() {
    let reacting = reacting().await;
    reacting.answer(ResponseTemplate::new(204)).await;

    reacting
        .account
        .react(GENERAL, MESSAGE, heart(), ReactionLocation::InlineButton)
        .await
        .unwrap();

    assert_eq!(reacting.mine(), [(Some(HEART.to_owned()), true)]);
    events_until(&reacting.subscription, |event| {
        *event
            == StoreEvent::MessageReactionsChanged {
                channel_id: GENERAL,
                message_id: MESSAGE,
            }
    })
    .await;

    reacting
        .account
        .unreact(GENERAL, MESSAGE, heart(), ReactionLocation::HoverBar)
        .await
        .unwrap();
    reacting
        .account
        .react(GENERAL, MESSAGE, wave(), ReactionLocation::Picker)
        .await
        .unwrap();

    assert_eq!(reacting.mine(), [(Some("wave".to_owned()), true)]);
    assert_eq!(
        reacting.lines().await,
        [
            format!("PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"),
            format!("DELETE {BASE}/{HEART_PATH}/0/@me?location=Message%20Hover%20Bar&burst=false"),
            format!(
                "PUT {BASE}/wave:700000000000000001/@me?location=Message%20Reaction%20Picker&type=0"
            ),
        ]
    );
    reacting.account.close();
}

#[tokio::test]
async fn a_refused_reaction_crosses_as_its_own_error() {
    let cases = [
        (refusal(403, 30010), ReactionError::TooMany),
        (refusal(403, 90001), ReactionError::Blocked),
        (refusal(403, 50013), ReactionError::Forbidden),
        (refusal(400, 10014), ReactionError::UnknownEmoji),
        (
            refusal(404, 10008),
            ReactionError::Request {
                error: RequestError::Discord {
                    status: 404,
                    code: 10008,
                    message: "No".to_owned(),
                },
            },
        ),
    ];
    for (answer, expected) in cases {
        let reacting = reacting().await;
        reacting.answer(answer).await;

        let err = reacting
            .account
            .react(GENERAL, MESSAGE, heart(), ReactionLocation::ContextMenu)
            .await
            .unwrap_err();

        assert_eq!(err, expected);
        assert_eq!(reacting.mine(), [], "{expected:?}: taken out again");
        reacting.account.close();
    }
}

#[tokio::test]
async fn a_reaction_refused_before_asking_is_an_invalid_request() {
    let reacting = reacting().await;
    let unknown = MessageId::new(MESSAGE.get() + 50);
    let invalid = ReactionError::Request {
        error: RequestError::InvalidRequest,
    };

    let added = reacting
        .account
        .react(GENERAL, unknown, heart(), ReactionLocation::Message)
        .await;
    let nothing = ReactionEmoji {
        name: None,
        ..heart()
    };
    let empty = reacting
        .account
        .react(GENERAL, MESSAGE, nothing, ReactionLocation::Message)
        .await;
    let all = reacting
        .account
        .remove_reactions(GENERAL, unknown, None)
        .await;

    assert_eq!(added, Err(invalid.clone()));
    assert_eq!(empty, Err(invalid.clone()));
    assert_eq!(all, Err(invalid));
    assert_eq!(reacting.lines().await, [] as [&str; 0]);
    reacting.account.close();
}

#[test]
fn reaction_errors_read_as_akari_cores() {
    let pairs = [
        (
            akari_core::ReactionError::TooMany,
            ReactionError::TooMany,
            "the message has too many reactions",
        ),
        (
            akari_core::ReactionError::Blocked,
            ReactionError::Blocked,
            "the message's author blocked the user",
        ),
        (
            akari_core::ReactionError::Forbidden,
            ReactionError::Forbidden,
            "reacting isn't allowed here",
        ),
        (
            akari_core::ReactionError::UnknownEmoji,
            ReactionError::UnknownEmoji,
            "Discord doesn't know the emoji",
        ),
        (
            akari_core::ReactionError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
            },
            ReactionError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
            },
            "rate limited by Discord",
        ),
        (
            akari_core::ReactionError::Request(akari_core::RequestError::Closed),
            ReactionError::Request {
                error: RequestError::Closed,
            },
            "the account is closed",
        ),
    ];
    for (core, expected, text) in pairs {
        assert_eq!(core.to_string(), text);
        let crossed = ReactionError::from(core);
        assert_eq!(crossed, expected);
        assert_eq!(crossed.to_string(), text);
    }
}

#[tokio::test]
async fn a_moderators_removals_cross() {
    let reacting = reacting().await;
    reacting.answer(ResponseTemplate::new(204)).await;
    let account = &reacting.account;

    account
        .remove_reaction(GENERAL, MESSAGE, heart(), ReactionKind::Normal, MIRA)
        .await
        .unwrap();
    account
        .remove_reaction(GENERAL, MESSAGE, wave(), ReactionKind::Burst, MIRA)
        .await
        .unwrap();
    account
        .remove_reactions(GENERAL, MESSAGE, Some(heart()))
        .await
        .unwrap();
    account
        .remove_reactions(GENERAL, MESSAGE, None)
        .await
        .unwrap();

    assert_eq!(
        reacting.lines().await,
        [
            format!("DELETE {BASE}/{HEART_PATH}/0/100000000000000002?location=Message&burst=false"),
            format!(
                "DELETE {BASE}/wave:700000000000000001/1/100000000000000002?location=Message&burst=true"
            ),
            format!("DELETE {BASE}/{HEART_PATH}"),
            format!("DELETE {BASE}"),
        ]
    );
    reacting.account.close();
}

#[tokio::test]
async fn reactors_cross_as_users_by_page() {
    let reacting = reacting().await;
    let users: Value = json!([
        {"id": MIRA.get().to_string(), "username": "mira", "global_name": "Mira",
         "discriminator": "0", "avatar": "0123456789abcdef0123456789abcdef"},
        {"id": "100000000000000050", "username": "ada", "discriminator": "0", "avatar": null},
    ]);
    Mock::given(method("GET"))
        .and(path_regex("/reactions/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(users))
        .mount(&reacting.server)
        .await;
    let account = &reacting.account;

    let full = account
        .reactors(GENERAL, MESSAGE, heart(), ReactionKind::Normal, None, 2)
        .await
        .unwrap();
    let short = account
        .reactors(
            GENERAL,
            MESSAGE,
            wave(),
            ReactionKind::Burst,
            Some(MIRA),
            100,
        )
        .await
        .unwrap();

    assert_eq!(
        reacting.lines().await,
        [
            format!("GET {BASE}/{HEART_PATH}?limit=2&type=0"),
            format!("GET {BASE}/wave:700000000000000001?limit=100&after=100000000000000002&type=1"),
        ]
    );
    assert!(full.has_more);
    assert!(!short.has_more);
    let names: Vec<(&str, &str)> = full
        .users
        .iter()
        .map(|user| (user.username.as_str(), user.display_name.as_str()))
        .collect();
    assert_eq!(names, [("mira", "Mira"), ("ada", "ada")]);
    assert!(matches!(full.users[0].avatar, ImageKey::Avatar { user_id, .. } if user_id == MIRA));
    assert!(matches!(
        full.users[1].avatar,
        ImageKey::DefaultAvatar { .. }
    ));
    reacting.account.close();
}

#[test]
fn an_emoji_crosses_back_by_its_id_or_its_name() {
    let custom = state::ReactionEmoji::from(wave());
    assert_eq!(
        (custom.id, custom.name.as_deref(), custom.animated),
        (Some(WAVE), Some("wave"), true)
    );
    let deleted = state::ReactionEmoji::from(ReactionEmoji {
        name: None,
        ..wave()
    });
    assert_eq!((deleted.id, deleted.name), (Some(WAVE), None));
    let unicode = state::ReactionEmoji::from(heart());
    assert_eq!(
        (unicode.id, unicode.name.as_deref(), unicode.animated),
        (None, Some(HEART), false)
    );
}
