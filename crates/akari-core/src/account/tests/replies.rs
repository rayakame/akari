use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::ReplyTo;
use crate::model::MessageType;
use crate::state::ReplyTarget;

const GUILD: &str = "200000000000000001";
const TARGET: u64 = 400_000_000_000_000_003;
const MIRA: u64 = 100_000_000_000_000_002;
const DM: u64 = 300_000_000_000_000_010;
const DM_MESSAGES: &str = "/api/v9/channels/300000000000000010/messages";

fn target() -> MessageId {
    Snowflake::new(TARGET)
}

fn reply(mention: bool) -> Option<ReplyTo> {
    Some(ReplyTo {
        message: target(),
        mention,
    })
}

fn pending_row(account: &Account, channel: ChannelId) -> Arc<crate::state::Message> {
    account.store().messages(channel).unwrap().pending[0].clone()
}

// Online with #general viewed and Mira's message in its window.
async fn replying(
    fake: &mut FakeGateway,
    server: &MockServer,
) -> (Account, Subscription, FakeConnection) {
    let (account, subscription, mut connection) = sending(fake, server).await;
    connection.send(message(2, TARGET)).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;
    (account, subscription, connection)
}

async fn refuse(server: &MockServer, status: u16, body: Value) {
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

fn unknown_reference() -> Value {
    json!({
        "code": 50035,
        "message": "Invalid Form Body",
        "errors": {"message_reference": {"_errors": [
            {"code": "REPLIES_UNKNOWN_MESSAGE", "message": "Unknown message"}
        ]}}
    })
}

#[tokio::test]
async fn a_pinging_reply_names_its_target_and_no_allowed_mentions() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    let sent = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap();

    let bodies = sends.bodies();
    assert_eq!(
        bodies,
        [json!({
            "content": "hello",
            "nonce": bodies[0]["nonce"],
            "tts": false,
            "message_reference": {
                "guild_id": GUILD,
                "channel_id": GENERAL.to_string(),
                "message_id": TARGET.to_string(),
            },
            "flags": 0,
        })]
    );
    assert_eq!(ids(&account), [TARGET, sent.get()]);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_reply_without_the_ping_allows_every_mention_but_the_replied_user() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    account
        .send_message(general(), "hello".to_owned(), reply(false))
        .await
        .unwrap();

    let bodies = sends.bodies();
    assert_eq!(
        bodies,
        [json!({
            "content": "hello",
            "nonce": bodies[0]["nonce"],
            "tts": false,
            "message_reference": {
                "guild_id": GUILD,
                "channel_id": GENERAL.to_string(),
                "message_id": TARGET.to_string(),
            },
            "allowed_mentions": {"parse": ["users", "roles", "everyone"], "replied_user": false},
            "flags": 0,
        })]
    );
}

#[tokio::test]
async fn a_message_that_is_no_reply_sends_neither_field() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    account
        .send_message(general(), "hello".to_owned(), None)
        .await
        .unwrap();

    let bodies = sends.bodies();
    assert_eq!(
        bodies,
        [json!({"content": "hello", "nonce": bodies[0]["nonce"], "tts": false, "flags": 0})]
    );
}

#[tokio::test]
async fn a_reply_in_a_dm_leaves_the_guild_out() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let bodies: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
    let seen = bodies.clone();
    Mock::given(method("POST"))
        .and(path(DM_MESSAGES))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            seen.lock().unwrap().push(body.clone());
            let mut answer = created(400_000_000_000_000_050, &body);
            answer["channel_id"] = DM.to_string().into();
            answer.as_object_mut().unwrap().remove("guild_id");
            ResponseTemplate::new(200).set_body_json(answer)
        })
        .mount(&server)
        .await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;
    let dm: ChannelId = Snowflake::new(DM);
    account.view_channel(dm);
    let mut direct = message(2, TARGET);
    direct["d"]["channel_id"] = DM.to_string().into();
    direct["d"].as_object_mut().unwrap().remove("guild_id");
    direct["d"].as_object_mut().unwrap().remove("member");
    connection.send(direct).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;

    account
        .send_message(dm, "hello".to_owned(), reply(true))
        .await
        .unwrap();

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(
        bodies,
        [json!({
            "content": "hello",
            "nonce": bodies[0]["nonce"],
            "tts": false,
            "message_reference": {
                "channel_id": DM.to_string(),
                "message_id": TARGET.to_string(),
            },
            "flags": 0,
        })]
    );
}

#[tokio::test]
async fn a_queued_reply_shows_as_a_reply_with_its_preview_at_once() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;
    sync(&mut connection, &subscription, 3).await;

    let pending = account
        .queue_message(general(), "hello", reply(true))
        .unwrap();

    let row = pending_row(&account, general());
    assert_eq!(row.id, pending);
    assert_eq!(row.kind, MessageType::Reply);
    assert_eq!(row.delivery, Delivery::Pending);
    assert_eq!(row.replied_to(), Some((general(), target())));
    let reference = row.reference.as_deref().unwrap();
    assert_eq!(reference.kind, crate::model::MessageReferenceType::Default);
    assert_eq!(reference.channel_id, Some(general()));
    assert_eq!(
        reference.guild_id.map(Snowflake::get),
        Some(GUILD.parse().unwrap())
    );
    let mentioned: Vec<u64> = row.mentions.iter().map(|user| user.id.get()).collect();
    assert_eq!(mentioned, [MIRA]);
    assert!(account.store().reply_mentions_author(&row));
    let ReplyTarget::Loaded(preview) = account.store().reply_target(general(), target()) else {
        panic!("the target isn't loaded");
    };
    assert_eq!(&*preview.content, "hi");
    assert_eq!(preview.author.id.get(), MIRA);
    let events = sync(&mut connection, &subscription, 4).await;
    assert_eq!(
        events,
        [
            format!(
                "ReplyTargetChanged {{ channel_id: Snowflake({GENERAL}), target_id: Snowflake({TARGET}) }}"
            ),
            format!("MessageInserted({})", pending.get()),
        ]
    );
}

#[tokio::test]
async fn a_queued_reply_without_the_ping_mentions_no_one() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    let pending = account
        .queue_message(general(), "hello", reply(false))
        .unwrap();

    let row = pending_row(&account, general());
    assert_eq!(row.kind, MessageType::Reply);
    assert!(row.mentions.is_empty());
    assert!(!account.store().reply_mentions_author(&row));
    assert!(sends.bodies().is_empty());
    account.deliver_message(general(), pending).await.unwrap();
    let bodies = sends.bodies();
    assert_eq!(
        bodies[0]["allowed_mentions"],
        json!({"parse": ["users", "roles", "everyone"], "replied_user": false})
    );
    assert_eq!(
        bodies[0]["message_reference"]["message_id"],
        TARGET.to_string()
    );
}

#[tokio::test]
async fn a_retry_keeps_the_reference_the_ping_choice_and_the_nonce() {
    for mention in [false, true] {
        let mut fake = FakeGateway::start().await;
        let server = MockServer::start().await;
        let sends = Sends::new(&[403, 200]);
        sends.mount(&server).await;
        let (account, _subscription, _connection) = replying(&mut fake, &server).await;

        account
            .send_message(general(), "hello".to_owned(), reply(mention))
            .await
            .unwrap_err();
        let failed = pending_row(&account, general());
        assert_eq!(failed.delivery, Delivery::Failed);
        assert_eq!(failed.kind, MessageType::Reply);
        assert_eq!(failed.replied_to(), Some((general(), target())));
        assert!(matches!(
            account.store().reply_target(general(), target()),
            ReplyTarget::Loaded(_)
        ));
        let sent = account.retry_message(general(), failed.id).await.unwrap();

        let bodies = sends.bodies();
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], bodies[1]);
        assert_eq!(bodies[1]["nonce"], failed.id.get().to_string());
        assert_eq!(
            bodies[1]["message_reference"]["message_id"],
            TARGET.to_string()
        );
        assert_eq!(bodies[1].get("allowed_mentions").is_some(), !mention);
        assert_eq!(ids(&account), [TARGET, sent.get()]);
    }
}

#[tokio::test]
async fn the_answer_replaces_the_pending_reply() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;

    let sent = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap();

    let events = sync(&mut connection, &subscription, 3).await;
    assert_eq!(
        events.last().unwrap(),
        &format!("MessageReplaced({})", sent.get())
    );
    let confirmed = account.store().message(general(), sent).unwrap();
    assert_eq!(confirmed.kind, MessageType::Reply);
    assert_eq!(confirmed.replied_to(), Some((general(), target())));
    assert!(matches!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Loaded(_)
    ));
}

#[tokio::test]
async fn a_reply_to_what_may_not_be_answered_is_refused_without_a_request() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200, 403]);
    sends.mount(&server).await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;
    let mut pin = message(3, 400_000_000_000_000_004);
    pin["d"]["type"] = 6.into();
    connection.send(pin).await;
    events_until(&subscription, "MessageInserted(400000000000000004)").await;
    let to = |message: u64| {
        Some(ReplyTo {
            message: Snowflake::new(message),
            mention: true,
        })
    };

    // A message the store doesn't have, a pin's system line, and a message of another channel.
    for refused in [to(400_000_000_000_000_999), to(400_000_000_000_000_004)] {
        assert!(matches!(
            account.queue_message(general(), "hello", refused),
            Err(RequestError::InvalidRequest)
        ));
        assert!(matches!(
            account
                .send_message(general(), "hello".to_owned(), refused)
                .await,
            Err(RequestError::InvalidRequest)
        ));
    }
    let dm: ChannelId = Snowflake::new(DM);
    account.view_channel(dm);
    assert!(matches!(
        account.queue_message(dm, "hello", reply(true)),
        Err(RequestError::InvalidRequest)
    ));
    assert!(outbox(&account).is_empty());
    assert!(account.store().messages(dm).unwrap().pending.is_empty());
    let unviewed: ChannelId = Snowflake::new(300_000_000_000_000_011);
    assert!(matches!(
        account.queue_message(unviewed, "hello", reply(true)),
        Err(RequestError::InvalidRequest)
    ));
    assert!(
        account.store().messages(unviewed).is_none(),
        "a refused reply views nothing"
    );
    assert!(sends.bodies().is_empty());

    // A failed row isn't a message yet.
    account
        .send_message(general(), "one".to_owned(), None)
        .await
        .unwrap();
    account
        .send_message(general(), "two".to_owned(), None)
        .await
        .unwrap_err();
    let failed = pending_row(&account, general()).id;
    assert!(matches!(
        account.queue_message(
            general(),
            "hello",
            Some(ReplyTo {
                message: failed,
                mention: true
            })
        ),
        Err(RequestError::InvalidRequest)
    ));
    assert_eq!(sends.bodies().len(), 2);
}

#[tokio::test]
async fn a_deleted_target_fails_the_reply_with_its_own_error_and_keeps_the_text() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    refuse(&server, 400, unknown_reference()).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    let error = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap_err();

    assert!(
        matches!(error, RequestError::ReplyTargetDeleted),
        "{error:?}"
    );
    let failed = pending_row(&account, general());
    assert_eq!(failed.delivery, Delivery::Failed);
    assert_eq!(&*failed.content, "hello");
    assert_eq!(failed.replied_to(), Some((general(), target())));
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
}

#[tokio::test]
async fn unknown_message_is_the_deleted_target_only_for_a_reply() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    refuse(
        &server,
        404,
        json!({"code": 10008, "message": "Unknown Message"}),
    )
    .await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    let reply_error = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap_err();
    let plain_error = account
        .send_message(general(), "hello".to_owned(), None)
        .await
        .unwrap_err();

    assert!(
        matches!(reply_error, RequestError::ReplyTargetDeleted),
        "{reply_error:?}"
    );
    assert!(
        matches!(plain_error, RequestError::Discord { code: 10008, .. }),
        "{plain_error:?}"
    );
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
}

#[tokio::test]
async fn a_deleted_target_among_other_field_errors_is_still_the_deleted_target() {
    let unknown = json!({"_errors": [
        {"code": "REPLIES_UNKNOWN_MESSAGE", "message": "Unknown message"}
    ]});
    let required = json!({"_errors": [
        {"code": "BASE_TYPE_REQUIRED", "message": "This field is required"}
    ]});
    let refusal =
        |errors: Value| json!({"code": 50035, "message": "Invalid Form Body", "errors": errors});
    let answers = [
        (
            refusal(json!({"content": required, "message_reference": unknown})),
            true,
        ),
        (
            refusal(json!({"message_reference": unknown, "content": required})),
            true,
        ),
        (refusal(json!({"content": unknown})), false),
    ];
    for (answer, deleted) in answers {
        let mut fake = FakeGateway::start().await;
        let server = MockServer::start().await;
        refuse(&server, 400, answer.clone()).await;
        let (account, _subscription, _connection) = replying(&mut fake, &server).await;

        let error = account
            .send_message(general(), "hello".to_owned(), reply(true))
            .await
            .unwrap_err();

        if deleted {
            assert!(
                matches!(error, RequestError::ReplyTargetDeleted),
                "{error:?} for {answer}"
            );
        } else {
            assert!(
                matches!(error, RequestError::Discord { code: 50035, .. }),
                "{error:?} for {answer}"
            );
            assert!(matches!(
                account.store().reply_target(general(), target()),
                ReplyTarget::Loaded(_)
            ));
        }
    }
}

#[tokio::test]
async fn a_reply_refused_for_its_deleted_target_is_refused_again_on_a_retry() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let bodies: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
    let seen = bodies.clone();
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(move |request: &wiremock::Request| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&request.body).unwrap());
            ResponseTemplate::new(400).set_body_json(unknown_reference())
        })
        .mount(&server)
        .await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;
    account
        .send_message(general(), "hello".to_owned(), reply(false))
        .await
        .unwrap_err();
    let failed = pending_row(&account, general());

    let again = account.retry_message(general(), failed.id).await;

    assert!(
        matches!(again, Err(RequestError::ReplyTargetDeleted)),
        "{again:?}"
    );
    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(
        bodies[1]["message_reference"]["message_id"],
        TARGET.to_string()
    );
    let row = pending_row(&account, general());
    assert_eq!(row.id, failed.id);
    assert_eq!(row.delivery, Delivery::Failed);
    assert_eq!(row.replied_to(), Some((general(), target())));
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
}

#[tokio::test]
async fn a_reply_discord_makes_to_a_message_deleted_meanwhile_says_so() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let mut answer = created(400_000_000_000_000_050, &body);
            answer["referenced_message"] = Value::Null;
            ResponseTemplate::new(200).set_body_json(answer)
        })
        .mount(&server)
        .await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;
    sync(&mut connection, &subscription, 3).await;

    let sent = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap();

    let confirmed = account.store().message(general(), sent).unwrap();
    assert_eq!(confirmed.kind, MessageType::Reply);
    assert_eq!(confirmed.replied_to(), Some((general(), target())));
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
    let events = sync(&mut connection, &subscription, 4).await;
    let changed = format!(
        "ReplyTargetChanged {{ channel_id: Snowflake({GENERAL}), target_id: Snowflake({TARGET}) }}"
    );
    assert_eq!(
        events.iter().filter(|event| **event == changed).count(),
        2,
        "loaded when queued, deleted with the answer: {events:?}"
    );
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_reply_starts_the_cooldown_and_a_refused_one_does_not() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, subscription, mut connection) =
        slowmode_sending(&mut fake, &server, json!([])).await;
    connection.send(message(2, TARGET)).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;
    let unknown = Some(ReplyTo {
        message: Snowflake::new(4242),
        mention: true,
    });

    let refused = account.queue_message(general(), "hello", unknown);

    assert!(matches!(refused, Err(RequestError::InvalidRequest)));
    assert_eq!(slowmode_until(&account), None);

    account
        .queue_message(general(), "hello", reply(true))
        .unwrap();

    assert!(slowmode_until(&account).is_some());
}

#[tokio::test]
async fn a_failed_reply_is_not_delivered_again_without_a_retry() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[403]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;
    account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap_err();
    let failed = pending_row(&account, general());

    let delivered = account.deliver_message(general(), failed.id).await;

    assert!(
        matches!(delivered, Err(RequestError::InvalidRequest)),
        "{delivered:?}"
    );
    assert_eq!(sends.bodies().len(), 1);
    assert_eq!(pending_row(&account, general()).delivery, Delivery::Failed);
}

#[tokio::test]
async fn another_refusal_of_the_reference_stays_discords_error() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    refuse(
        &server,
        400,
        json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {"message_reference": {"_errors": [
                {"code": "REPLIES_CANNOT_REPLY_TO_SYSTEM_MESSAGE", "message": "Cannot reply to a system message"}
            ]}}
        }),
    )
    .await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;

    let error = account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap_err();

    assert!(
        matches!(error, RequestError::Discord { code: 50035, .. }),
        "{error:?}"
    );
    assert!(matches!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Loaded(_)
    ));
}

fn delete(seq: u64, id: u64) -> Value {
    json!({"op": 0, "s": seq, "t": "MESSAGE_DELETE", "d": {
        "id": id.to_string(), "channel_id": GENERAL.to_string(), "guild_id": GUILD
    }})
}

#[tokio::test]
async fn a_target_deleted_while_the_reply_is_sent_leaves_a_reply_to_a_deleted_message() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(300));
    sends.mount(&server).await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;

    let (sent, ()) = tokio::join!(
        account.send_message(general(), "hello".to_owned(), reply(true)),
        async {
            sends.first_nonce().await;
            connection.send(delete(3, TARGET)).await;
            events_until(&subscription, &format!("MessageDeleted({TARGET})")).await;
            let pending = pending_row(&account, general());
            assert_eq!(pending.delivery, Delivery::Pending);
            assert_eq!(pending.replied_to(), Some((general(), target())));
            assert_eq!(
                account.store().reply_target(general(), target()),
                ReplyTarget::Deleted
            );
        }
    );

    let sent = sent.unwrap();
    assert_eq!(ids(&account), [sent.get()]);
    assert!(outbox(&account).is_empty());
    let confirmed = account.store().message(general(), sent).unwrap();
    assert_eq!(confirmed.replied_to(), Some((general(), target())));
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
}

#[tokio::test]
async fn a_target_deleted_while_the_reply_is_refused_leaves_a_failed_reply() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(unknown_reference())
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;
    let (account, subscription, mut connection) = replying(&mut fake, &server).await;

    let (sent, ()) = tokio::join!(
        account.send_message(general(), "hello".to_owned(), reply(false)),
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            connection.send(delete(3, TARGET)).await;
            events_until(&subscription, &format!("MessageDeleted({TARGET})")).await;
        }
    );

    assert!(matches!(sent, Err(RequestError::ReplyTargetDeleted)));
    assert!(ids(&account).is_empty());
    let failed = pending_row(&account, general());
    assert_eq!(failed.delivery, Delivery::Failed);
    assert_eq!(&*failed.content, "hello");
    assert_eq!(
        account.store().reply_target(general(), target()),
        ReplyTarget::Deleted
    );
    account.discard_message(general(), failed.id);
    assert!(outbox(&account).is_empty());
}

// TARGET is held for a reply, then a jump leaves it behind.
async fn held_out_of_sight(
    fake: &mut FakeGateway,
    server: &MockServer,
) -> (Account, Subscription, FakeConnection) {
    mock_page(server, ("around", "20"), page(&[19, 20, 21])).await;
    mock_page(server, ("limit", "50"), page(&[30])).await;
    let (account, subscription, connection) = replying(fake, server).await;
    assert!(account.hold_reply(general(), target()));
    account
        .load_messages(
            general(),
            MessageLoad::Around {
                id: Snowflake::new(20),
                limit: 3,
            },
        )
        .await
        .unwrap();
    assert_eq!(ids(&account), [19, 20, 21]);
    (account, subscription, connection)
}

#[tokio::test]
async fn a_reply_to_a_held_message_that_left_the_window_is_sent() {
    for mention in [true, false] {
        let mut fake = FakeGateway::start().await;
        let server = MockServer::start().await;
        let sends = Sends::new(&[200]);
        sends.mount(&server).await;
        let (account, _subscription, _connection) = held_out_of_sight(&mut fake, &server).await;

        let pending = account
            .queue_message(general(), "hello", reply(mention))
            .unwrap();

        let row = pending_row(&account, general());
        assert_eq!(row.kind, MessageType::Reply);
        assert_eq!(row.replied_to(), Some((general(), target())));
        assert_eq!(row.mentions.len(), usize::from(mention));
        let ReplyTarget::Loaded(preview) = account.store().reply_target(general(), target()) else {
            panic!("the target isn't loaded");
        };
        assert_eq!(&*preview.content, "hi");
        assert_eq!(account.store().held_reply(general()), None);

        let sent = account.deliver_message(general(), pending).await.unwrap();

        let bodies = sends.bodies();
        assert_eq!(bodies.len(), 1);
        assert_eq!(
            bodies[0]["message_reference"],
            json!({
                "guild_id": GUILD,
                "channel_id": GENERAL.to_string(),
                "message_id": TARGET.to_string(),
            })
        );
        assert_eq!(bodies[0].get("allowed_mentions").is_some(), !mention);
        assert_eq!(ids(&account), [30, sent.get()]);
    }
}

#[tokio::test]
async fn a_released_message_that_left_the_window_is_refused() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = held_out_of_sight(&mut fake, &server).await;

    account.release_reply(general());

    assert!(matches!(
        account.queue_message(general(), "hello", reply(true)),
        Err(RequestError::InvalidRequest)
    ));
    assert!(sends.bodies().is_empty());
}

#[tokio::test]
async fn a_held_reply_is_queued_where_the_window_was_evicted() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;
    assert!(account.hold_reply(general(), target()));
    for other in 1..=10_u64 {
        account.view_channel(Snowflake::new(900 + other));
    }
    assert!(account.store().messages(general()).is_none());

    account
        .queue_message(general(), "hello", reply(true))
        .unwrap();

    let row = pending_row(&account, general());
    assert_eq!(row.replied_to(), Some((general(), target())));
}

#[tokio::test]
async fn closing_the_account_ends_its_holds() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, _subscription, _connection) = replying(&mut fake, &server).await;
    assert!(account.hold_reply(general(), target()));
    assert_eq!(account.store().held_reply(general()), Some(target()));

    account.close();

    assert_eq!(account.store().held_reply(general()), None);
}

#[tokio::test]
async fn a_reply_to_the_users_own_message_asks_for_no_ping() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;
    let mut own = message(2, TARGET);
    own["d"]["author"]["id"] = ME.into();
    connection.send(own).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;

    account
        .send_message(general(), "hello".to_owned(), reply(true))
        .await
        .unwrap();

    assert_eq!(
        sends.bodies()[0]["allowed_mentions"],
        json!({"parse": ["users", "roles", "everyone"], "replied_user": false})
    );
}
