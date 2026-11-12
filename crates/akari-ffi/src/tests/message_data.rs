use std::sync::Arc;

use akari_core::model::{EmojiId, MessageId, MessageType, UserId};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;

use super::fake::{FakeGateway, GENERAL, MESSAGE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::Account;
use crate::images::ImageKey;
use crate::records::{
    MessageActions, MessageReactions, Reaction, ReactionEmoji, RepliedTo, ReplyTarget,
};
use crate::subscription::{StoreEvent, StoreSubscription};

const MIRA: UserId = UserId::new(100_000_000_000_000_002);
const WAVE: EmojiId = EmojiId::new(700_000_000_000_000_001);
// message_reply.json: the user's reply and the message of Mira's it answers.
const REPLY: MessageId = MessageId::new(400_000_000_000_000_002);
const TARGET: MessageId = MessageId::new(400_000_000_000_000_001);

struct Viewing {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    ws: WebSocketStream<TcpStream>,
    sequence: u64,
}

impl Viewing {
    async fn dispatch(&mut self, kind: &str, data: Value) {
        self.sequence += 1;
        send(
            &mut self.ws,
            json!({"op": 0, "s": self.sequence, "t": kind, "d": data}),
        )
        .await;
    }

    async fn until(&self, last: impl Fn(&StoreEvent) -> bool) -> Vec<StoreEvent> {
        events_until(&self.subscription, last).await
    }
}

async fn viewing() -> Viewing {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account.view_channel(GENERAL);
    Viewing {
        account,
        subscription,
        ws,
        sequence: 1,
    }
}

// Mira's message in #general, with what `edit` adds.
fn miras(edit: impl FnOnce(&mut Value)) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message.as_object_mut().unwrap().remove("nonce");
    edit(&mut message);
    message
}

fn reply() -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_reply.json"
    ));
    message["guild_id"] = "200000000000000001".into();
    message
}

fn inserted(id: MessageId) -> impl Fn(&StoreEvent) -> bool {
    move |event| matches!(event, StoreEvent::MessageInserted { message_id, .. } if *message_id == id)
}

fn reaction(user: UserId, emoji: Value) -> Value {
    json!({
        "channel_id": GENERAL.get().to_string(), "message_id": MESSAGE.get().to_string(),
        "guild_id": "200000000000000001", "user_id": user.get().to_string(),
        "emoji": emoji, "type": 0,
    })
}

fn unicode(name: &str) -> ReactionEmoji {
    ReactionEmoji {
        id: None,
        name: Some(name.to_owned()),
        animated: false,
        key: None,
    }
}

#[tokio::test]
async fn a_messages_reactions_cross_with_their_counts_and_keys() {
    let mut viewing = viewing().await;
    let message = miras(|message| {
        message["reactions"] = json!([
            {"emoji": {"id": null, "name": "👍"}, "count": 3,
             "count_details": {"normal": 2, "burst": 1}, "me": true, "me_burst": false},
            {"emoji": {"id": WAVE.get().to_string(), "name": "wave", "animated": true},
             "count": 1, "count_details": {"normal": 1, "burst": 0}, "me": false,
             "me_burst": false},
        ]);
    });
    viewing.dispatch("MESSAGE_CREATE", message).await;
    viewing.until(inserted(MESSAGE)).await;
    let store = viewing.account.store();

    let reactions = store.reactions(GENERAL, MESSAGE);

    assert_eq!(
        reactions,
        [
            Reaction {
                emoji: unicode("👍"),
                normal: 2,
                burst: 1,
                me: true,
                me_burst: false,
            },
            Reaction {
                emoji: ReactionEmoji {
                    id: Some(WAVE),
                    name: Some("wave".to_owned()),
                    animated: true,
                    key: Some(ImageKey::Emoji {
                        emoji_id: WAVE,
                        animated: true,
                    }),
                },
                normal: 1,
                burst: 0,
                me: false,
                me_burst: false,
            },
        ]
    );
    assert_eq!(store.reactions(GENERAL, TARGET), []);
    viewing.account.close();
}

#[tokio::test]
async fn a_reaction_reaches_the_subscription_and_the_read() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", miras(|_| {})).await;
    viewing.until(inserted(MESSAGE)).await;
    let changed = StoreEvent::MessageReactionsChanged {
        channel_id: GENERAL,
        message_id: MESSAGE,
    };

    viewing
        .dispatch(
            "MESSAGE_REACTION_ADD",
            reaction(USER, json!({"id": null, "name": "🔥"})),
        )
        .await;
    let events = viewing.until(|event| *event == changed).await;

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StoreEvent::MessageUpdated { .. })),
        "{events:?}"
    );
    let store = viewing.account.store();
    assert_eq!(
        store.reactions(GENERAL, MESSAGE),
        [Reaction {
            emoji: unicode("🔥"),
            normal: 1,
            burst: 0,
            me: true,
            me_burst: false,
        }]
    );

    viewing
        .dispatch(
            "MESSAGE_REACTION_REMOVE",
            reaction(USER, json!({"id": null, "name": "🔥"})),
        )
        .await;
    viewing.until(|event| *event == changed).await;

    assert_eq!(store.reactions(GENERAL, MESSAGE), []);
    viewing.account.close();
}

#[tokio::test]
async fn message_reactions_reads_many_messages_at_once() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", reply()).await;
    viewing.until(inserted(REPLY)).await;
    let message = miras(|message| {
        message["reactions"] = json!([
            {"emoji": {"id": null, "name": "👍"}, "count": 1, "me": false},
        ]);
    });
    viewing.dispatch("MESSAGE_CREATE", message).await;
    viewing.until(inserted(MESSAGE)).await;

    let read = viewing
        .account
        .store()
        .message_reactions(GENERAL, vec![MESSAGE, REPLY, TARGET]);

    assert_eq!(
        read,
        [MessageReactions {
            message_id: MESSAGE,
            reactions: vec![Reaction {
                emoji: unicode("👍"),
                normal: 1,
                burst: 0,
                me: false,
                me_burst: false,
            }],
        }]
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_reply_names_its_target_and_the_target_is_read_by_id() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", reply()).await;
    let events = viewing.until(inserted(REPLY)).await;
    let store = viewing.account.store();

    assert!(
        events.contains(&StoreEvent::ReplyTargetChanged {
            channel_id: GENERAL,
            target_id: TARGET,
        }),
        "{events:?}"
    );
    let messages = store.messages(GENERAL, vec![REPLY]);
    assert_eq!(
        messages[0].replied_to,
        Some(RepliedTo {
            channel_id: GENERAL,
            message_id: TARGET,
            mentions_author: true,
        })
    );
    let ReplyTarget::Loaded { preview } = store.reply_target(GENERAL, TARGET) else {
        panic!("the target came with the reply");
    };
    assert_eq!(preview.kind, MessageType::Default);
    assert_eq!(preview.author.id, MIRA);
    assert_eq!(preview.author.display_name, "Mira");
    assert_eq!(preview.content, "Release notes are up");
    assert!(!preview.from_webhook);
    assert!(!preview.edited);
    assert!(!preview.voice_message);
    assert!(!preview.has_attachments);
    assert!(!preview.has_embeds);
    assert!(!preview.has_sticker);
    assert!(!preview.forwarded);
    assert_eq!(store.reply_target(GENERAL, MESSAGE), ReplyTarget::NotLoaded);
    viewing.account.close();
}

#[tokio::test]
async fn a_reply_preview_carries_what_its_special_texts_need() {
    let mut viewing = viewing().await;
    let mut message = reply();
    let target = &mut message["referenced_message"];
    target["flags"] = (1 << 13).into();
    target["edited_timestamp"] = "2024-03-01T08:30:30.000000+00:00".into();
    target["webhook_id"] = "900000000000000001".into();
    target["attachments"] = json!([
        {"id": "600000000000000001", "filename": "voice-message.ogg", "size": 4096,
         "url": "https://cdn.discordapp.com/attachments/1/2/voice-message.ogg",
         "proxy_url": "https://media.discordapp.net/attachments/1/2/voice-message.ogg"},
    ]);
    target["sticker_items"] =
        json!([{"id": "800000000000000001", "name": "Wave", "format_type": 1}]);
    viewing.dispatch("MESSAGE_CREATE", message).await;
    viewing.until(inserted(REPLY)).await;

    let ReplyTarget::Loaded { preview } = viewing.account.store().reply_target(GENERAL, TARGET)
    else {
        panic!("the target came with the reply");
    };

    assert!(preview.from_webhook);
    assert!(preview.edited);
    assert!(preview.voice_message);
    assert!(preview.has_attachments);
    assert!(preview.has_sticker);
    assert!(!preview.has_embeds);
    viewing.account.close();
}

#[tokio::test]
async fn a_reply_that_didnt_ping_says_so() {
    let mut viewing = viewing().await;
    let mut message = reply();
    message["mentions"] = json!([]);
    viewing.dispatch("MESSAGE_CREATE", message).await;
    viewing.until(inserted(REPLY)).await;

    let messages = viewing.account.store().messages(GENERAL, vec![REPLY]);

    assert_eq!(
        messages[0].replied_to,
        Some(RepliedTo {
            channel_id: GENERAL,
            message_id: TARGET,
            mentions_author: false,
        })
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_message_that_answers_nothing_has_no_target() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", miras(|_| {})).await;
    viewing.until(inserted(MESSAGE)).await;

    let messages = viewing.account.store().messages(GENERAL, vec![MESSAGE]);

    assert_eq!(messages[0].replied_to, None);
    viewing.account.close();
}

#[tokio::test]
async fn a_deleted_target_reaches_the_subscription_and_the_reads() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", reply()).await;
    viewing.until(inserted(REPLY)).await;
    let changed = StoreEvent::ReplyTargetChanged {
        channel_id: GENERAL,
        target_id: TARGET,
    };

    viewing
        .dispatch(
            "MESSAGE_DELETE",
            json!({
                "id": TARGET.get().to_string(), "channel_id": GENERAL.get().to_string(),
                "guild_id": "200000000000000001",
            }),
        )
        .await;
    viewing.until(|event| *event == changed).await;
    let store = viewing.account.store();

    assert_eq!(store.reply_target(GENERAL, TARGET), ReplyTarget::Deleted);
    let targets = store.reply_targets(GENERAL, vec![MESSAGE, TARGET]);
    assert_eq!(targets, [ReplyTarget::NotLoaded, ReplyTarget::Deleted]);
    let messages = store.messages(GENERAL, vec![REPLY]);
    assert_eq!(
        messages[0].replied_to,
        Some(RepliedTo {
            channel_id: GENERAL,
            message_id: TARGET,
            mentions_author: false,
        })
    );
    viewing.account.close();
}

#[tokio::test]
async fn unread_pins_reach_the_subscription_and_the_read() {
    let mut viewing = viewing().await;
    let store = viewing.account.store();
    let changed = StoreEvent::PinsUnreadChanged {
        channel_id: GENERAL,
    };
    assert!(!store.pins_unread(GENERAL));

    viewing
        .dispatch(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": GENERAL.get().to_string(), "guild_id": "200000000000000001",
                "last_pin_timestamp": "2026-10-01T13:00:00.250000+00:00",
            }),
        )
        .await;
    viewing.until(|event| *event == changed).await;

    assert!(store.pins_unread(GENERAL));

    viewing
        .dispatch(
            "CHANNEL_PINS_ACK",
            json!({
                "channel_id": GENERAL.get().to_string(),
                "timestamp": "2026-10-01T13:00:00.250000+00:00", "version": 7,
            }),
        )
        .await;
    viewing.until(|event| *event == changed).await;

    assert!(!store.pins_unread(GENERAL));
    viewing.account.close();
}

#[tokio::test]
async fn message_actions_cross_field_by_field() {
    let mut viewing = viewing().await;
    viewing.dispatch("MESSAGE_CREATE", reply()).await;
    viewing.until(inserted(REPLY)).await;
    let mut pinned = miras(|message| message["pinned"] = true.into());
    pinned["guild_id"] = "200000000000000001".into();
    viewing.dispatch("MESSAGE_CREATE", pinned).await;
    viewing.until(inserted(MESSAGE)).await;
    let store = viewing.account.store();

    // The user owns the guild: every permission.
    assert_eq!(
        store.message_actions(GENERAL, MESSAGE),
        MessageActions {
            reply: true,
            reply_pings: true,
            mention_toggle: true,
            edit: false,
            delete: true,
            pin: false,
            unpin: true,
            react: true,
            toggle_reaction: true,
            remove_reactions: true,
            mark_unread: true,
            copy_link: true,
            create_thread: true,
            view_thread: false,
        }
    );
    assert_eq!(
        store.message_actions(GENERAL, REPLY),
        MessageActions {
            reply: true,
            reply_pings: false,
            mention_toggle: false,
            edit: true,
            delete: true,
            pin: true,
            unpin: false,
            react: true,
            toggle_reaction: true,
            remove_reactions: true,
            mark_unread: true,
            copy_link: true,
            create_thread: true,
            view_thread: false,
        }
    );
    assert_eq!(
        store.message_actions(GENERAL, TARGET),
        MessageActions {
            reply: false,
            reply_pings: false,
            mention_toggle: false,
            edit: false,
            delete: false,
            pin: false,
            unpin: false,
            react: false,
            toggle_reaction: false,
            remove_reactions: false,
            mark_unread: false,
            copy_link: false,
            create_thread: false,
            view_thread: false,
        }
    );
    viewing.account.close();
}
