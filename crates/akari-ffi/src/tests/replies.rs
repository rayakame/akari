use std::sync::{Arc, Mutex};

use akari_core::model::{MessageId, MessageType};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, MESSAGE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::{Account, ReplyTo};
use crate::errors::RequestError;
use crate::formatted::{MentionName, SpanKind, TextBlockKind};
use crate::records::{Delivery, RepliedTo, ReplyTarget};
use crate::subscription::{StoreEvent, StoreSubscription};

const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";
// message_reply.json: the user's reply and the message of Mira's it answers.
const REPLY: MessageId = MessageId::new(400_000_000_000_000_002);
const TARGET: MessageId = MessageId::new(400_000_000_000_000_001);

struct Viewing {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    ws: WebSocketStream<TcpStream>,
    sequence: u64,
    _server: MockServer,
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

// Online with #general viewed and Mira's message (MESSAGE) in its window.
async fn viewing(server: MockServer) -> Viewing {
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account.view_channel(GENERAL);
    let mut viewing = Viewing {
        account,
        subscription,
        ws,
        sequence: 1,
        _server: server,
    };
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message.as_object_mut().unwrap().remove("nonce");
    viewing.dispatch("MESSAGE_CREATE", message).await;
    viewing.until(inserted(MESSAGE)).await;
    viewing
}

fn inserted(id: MessageId) -> impl Fn(&StoreEvent) -> bool {
    move |event| matches!(event, StoreEvent::MessageInserted { message_id, .. } if *message_id == id)
}

fn target_changed(event: &StoreEvent) -> bool {
    matches!(event, StoreEvent::ReplyTargetChanged { .. })
}

// Answers every send as created, and keeps the bodies.
async fn created(server: &MockServer) -> Arc<Mutex<Vec<Value>>> {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = bodies.clone();
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            seen.lock().unwrap().push(body.clone());
            let mut message = fixture(include_str!(
                "../../../akari-core/tests/fixtures/message_create.json"
            ));
            message["id"] = "400000000000000050".into();
            message["type"] = 19.into();
            message["content"] = body["content"].clone();
            message["nonce"] = body["nonce"].clone();
            message["message_reference"] = body["message_reference"].clone();
            message["author"] = json!({"id": "100000000000000001", "username": "akari_tester"});
            ResponseTemplate::new(200).set_body_json(message)
        })
        .mount(server)
        .await;
    bodies
}

fn reply_with(content: &str) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_reply.json"
    ));
    message["guild_id"] = "200000000000000001".into();
    message["referenced_message"]["content"] = content.into();
    message
}

#[tokio::test]
async fn a_reply_is_queued_as_one_and_delivered_with_its_reference() {
    let server = MockServer::start().await;
    let bodies = created(&server).await;
    let viewing = viewing(server).await;
    let store = viewing.account.store();
    let reply = ReplyTo {
        message_id: MESSAGE,
        mention: false,
    };

    let pending = viewing
        .account
        .queue_message(GENERAL, "hello".to_owned(), Some(reply))
        .unwrap();

    let row = &store.messages(GENERAL, vec![pending])[0];
    assert_eq!(row.kind, MessageType::Reply);
    assert_eq!(row.delivery, Delivery::Pending);
    assert_eq!(
        row.replied_to,
        Some(RepliedTo {
            channel_id: GENERAL,
            message_id: MESSAGE,
            mentions_author: false,
        })
    );
    let ReplyTarget::Loaded { preview } = store.reply_target(GENERAL, MESSAGE) else {
        panic!("the target is loaded at once");
    };
    assert_eq!(preview.content, "hi");
    assert!(bodies.lock().unwrap().is_empty());

    let sent = viewing
        .account
        .deliver_message(GENERAL, pending)
        .await
        .unwrap();

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(
        bodies,
        [json!({
            "content": "hello",
            "nonce": pending.get().to_string(),
            "tts": false,
            "message_reference": {
                "guild_id": "200000000000000001",
                "channel_id": GENERAL.get().to_string(),
                "message_id": MESSAGE.get().to_string(),
            },
            "allowed_mentions": {"parse": ["users", "roles", "everyone"], "replied_user": false},
            "flags": 0,
        })]
    );
    let confirmed = &store.messages(GENERAL, vec![sent])[0];
    assert_eq!(confirmed.kind, MessageType::Reply);
    assert_eq!(
        confirmed.replied_to.map(|replied| replied.message_id),
        Some(MESSAGE)
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_pinging_reply_has_its_at_before_the_answer() {
    let server = MockServer::start().await;
    let bodies = created(&server).await;
    let viewing = viewing(server).await;
    let reply = ReplyTo {
        message_id: MESSAGE,
        mention: true,
    };

    let pending = viewing
        .account
        .queue_message(GENERAL, "hello".to_owned(), Some(reply))
        .unwrap();
    let row = &viewing.account.store().messages(GENERAL, vec![pending])[0];
    assert_eq!(
        row.replied_to.map(|replied| replied.mentions_author),
        Some(true)
    );
    viewing
        .account
        .send_message(GENERAL, "again".to_owned(), Some(reply))
        .await
        .unwrap();

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].get("allowed_mentions").is_none());
    assert_eq!(
        bodies[0]["message_reference"]["message_id"],
        MESSAGE.get().to_string()
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_reply_to_a_message_the_store_doesnt_have_is_refused() {
    let server = MockServer::start().await;
    let bodies = created(&server).await;
    let viewing = viewing(server).await;
    let reply = ReplyTo {
        message_id: MessageId::new(400_000_000_000_000_999),
        mention: true,
    };

    assert_eq!(
        viewing
            .account
            .queue_message(GENERAL, "hello".to_owned(), Some(reply)),
        Err(RequestError::InvalidRequest)
    );
    assert_eq!(
        viewing
            .account
            .send_message(GENERAL, "hello".to_owned(), Some(reply))
            .await,
        Err(RequestError::InvalidRequest)
    );
    let window = viewing.account.store().window(GENERAL).unwrap();
    assert!(window.pending_ids.is_empty());
    assert!(bodies.lock().unwrap().is_empty());
    viewing.account.close();
}

#[tokio::test]
async fn a_deleted_target_crosses_as_its_own_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {"message_reference": {"_errors": [
                {"code": "REPLIES_UNKNOWN_MESSAGE", "message": "Unknown message"}
            ]}}
        })))
        .mount(&server)
        .await;
    let viewing = viewing(server).await;
    let store = viewing.account.store();
    let reply = ReplyTo {
        message_id: MESSAGE,
        mention: true,
    };

    let failed = viewing
        .account
        .send_message(GENERAL, "hello".to_owned(), Some(reply))
        .await;

    assert_eq!(failed, Err(RequestError::ReplyTargetDeleted));
    assert_eq!(
        RequestError::ReplyTargetDeleted.to_string(),
        "the message replied to was deleted"
    );
    let pending = store.window(GENERAL).unwrap().pending_ids;
    let row = &store.messages(GENERAL, pending)[0];
    assert_eq!(row.delivery, Delivery::Failed);
    assert_eq!(row.content, "hello");
    assert_eq!(row.kind, MessageType::Reply);
    assert_eq!(store.reply_target(GENERAL, MESSAGE), ReplyTarget::Deleted);
    viewing.account.close();
}

#[tokio::test]
async fn a_preview_has_its_content_on_one_line() {
    let content = "# Title\n> **bold** <@100000000000000001>\n```\nlet a;\nlet b;\n```";
    let mut viewing = viewing(MockServer::start().await).await;
    viewing
        .dispatch("MESSAGE_CREATE", reply_with(content))
        .await;
    viewing.until(inserted(REPLY)).await;
    let store = viewing.account.store();

    let ReplyTarget::Loaded { preview } = store.reply_target(GENERAL, TARGET) else {
        panic!("the target came with the reply");
    };

    assert_eq!(preview.content, content);
    let line = preview.formatted.expect("content has a line");
    assert_eq!(
        line.text,
        "# Title > **bold** <@100000000000000001> ``` let a; let b; ```"
    );
    assert!(!line.formatted.jumbo);
    let [block] = &line.formatted.blocks[..] else {
        panic!("one paragraph: {:?}", line.formatted.blocks);
    };
    assert_eq!(block.kind, TextBlockKind::Paragraph);
    assert!(block.containers.is_empty());
    let units: Vec<u16> = line.text.encode_utf16().collect();
    let text =
        |start: u32, end: u32| String::from_utf16(&units[start as usize..end as usize]).unwrap();
    let shown: Vec<String> = block
        .spans
        .iter()
        .map(|span| match &span.kind {
            SpanKind::Text { start, end } if span.bold => format!("*{}*", text(*start, *end)),
            SpanKind::Text { start, end } => text(*start, *end),
            SpanKind::Code { start, end } => format!("`{}`", text(*start, *end)),
            SpanKind::User {
                name: MentionName::Known { name },
                ..
            } => format!("@{name}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        shown.concat(),
        "Title > *bold* @Akari Tester `let a; let b;`",
        "{shown:?}"
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_preview_is_parsed_once_and_again_after_an_edit() {
    let mut viewing = viewing(MockServer::start().await).await;
    viewing
        .dispatch("MESSAGE_CREATE", reply_with("**first**"))
        .await;
    viewing.until(inserted(REPLY)).await;
    let store = viewing.account.store();
    let line = |target: ReplyTarget| match target {
        ReplyTarget::Loaded { preview } => preview.formatted.map(|line| line.text),
        other => panic!("not loaded: {other:?}"),
    };

    let before = store.content_stats().parsed;
    assert_eq!(
        line(store.reply_target(GENERAL, TARGET)).as_deref(),
        Some("**first**")
    );
    let parsed = store.content_stats().parsed;
    assert_eq!(parsed, before + 1);
    let both = store.reply_targets(GENERAL, vec![TARGET, MESSAGE]);
    assert_eq!(line(both[0].clone()).as_deref(), Some("**first**"));
    assert_eq!(both[1], ReplyTarget::NotLoaded);
    assert_eq!(store.content_stats().parsed, parsed, "read from the cache");

    viewing
        .dispatch(
            "MESSAGE_UPDATE",
            json!({
                "id": TARGET.get().to_string(), "channel_id": GENERAL.get().to_string(),
                "content": "second\nline", "edited_timestamp": "2024-03-01T09:00:00.000000+00:00",
            }),
        )
        .await;
    viewing.until(target_changed).await;
    assert_eq!(
        line(store.reply_target(GENERAL, TARGET)).as_deref(),
        Some("second line")
    );
    assert_eq!(store.content_stats().parsed, parsed + 1);

    viewing
        .dispatch(
            "MESSAGE_UPDATE",
            json!({
                "id": TARGET.get().to_string(), "channel_id": GENERAL.get().to_string(),
                "content": "",
            }),
        )
        .await;
    viewing.until(target_changed).await;
    assert_eq!(line(store.reply_target(GENERAL, TARGET)), None);
    viewing.account.close();
}

#[tokio::test]
async fn a_held_message_is_answered_after_a_jump_left_it_behind() {
    let server = MockServer::start().await;
    let bodies = created(&server).await;
    let mut far = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    far["id"] = "400000000000000900".into();
    far.as_object_mut().unwrap().remove("nonce");
    Mock::given(method("GET"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([far])))
        .mount(&server)
        .await;
    let viewing = viewing(server).await;
    let store = viewing.account.store();
    let away = MessageId::new(400_000_000_000_000_900);

    assert!(viewing.account.hold_reply(GENERAL, MESSAGE));
    viewing
        .account
        .load_messages(
            GENERAL,
            crate::account::MessageLoad::Around {
                message_id: away,
                limit: 3,
            },
            None,
        )
        .await
        .unwrap();
    assert!(store.messages(GENERAL, vec![MESSAGE]).is_empty());
    assert!(matches!(
        store.reply_target(GENERAL, MESSAGE),
        ReplyTarget::Loaded { .. }
    ));

    let reply = ReplyTo {
        message_id: MESSAGE,
        mention: true,
    };
    let sent = viewing
        .account
        .send_message(GENERAL, "hello".to_owned(), Some(reply))
        .await
        .unwrap();

    assert_eq!(
        bodies.lock().unwrap()[0]["message_reference"]["message_id"],
        MESSAGE.get().to_string()
    );
    let confirmed = &store.messages(GENERAL, vec![sent])[0];
    assert_eq!(
        confirmed.replied_to.map(|replied| replied.message_id),
        Some(MESSAGE)
    );
    viewing.account.close();
}

#[tokio::test]
async fn a_released_message_is_refused_after_a_jump_left_it_behind() {
    let server = MockServer::start().await;
    let bodies = created(&server).await;
    let mut far = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    far["id"] = "400000000000000900".into();
    far.as_object_mut().unwrap().remove("nonce");
    Mock::given(method("GET"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([far])))
        .mount(&server)
        .await;
    let viewing = viewing(server).await;

    assert!(viewing.account.hold_reply(GENERAL, MESSAGE));
    assert!(!viewing.account.hold_reply(GENERAL, MessageId::new(42)));
    assert!(viewing.account.hold_reply(GENERAL, MESSAGE));
    viewing.account.release_reply(GENERAL);
    viewing
        .account
        .load_messages(
            GENERAL,
            crate::account::MessageLoad::Around {
                message_id: MessageId::new(400_000_000_000_000_900),
                limit: 3,
            },
            None,
        )
        .await
        .unwrap();

    let reply = ReplyTo {
        message_id: MESSAGE,
        mention: true,
    };
    let refused = viewing
        .account
        .queue_message(GENERAL, "hello".to_owned(), Some(reply));

    assert!(matches!(refused, Err(RequestError::InvalidRequest)));
    assert!(bodies.lock().unwrap().is_empty());
    viewing.account.close();
}
