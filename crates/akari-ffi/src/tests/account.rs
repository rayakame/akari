use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use akari_core::model::{ChannelId, ChannelType, MessageId, MessageType, Permissions};
use serde_json::json;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

use super::fake::{
    CATEGORY, DOWN, FakeGateway, GENERAL, GUILD, MESSAGE, VOICE, drain, events_until, fixture,
    online,
};
use super::gateway::{WAIT, send};
use super::support::{
    BlockingStore, MemoryStore, USER, block_on, local_client, poll_once, token, unreachable_client,
};
use crate::errors::GatewayError;
use crate::records::{Channel, ConnectionState, Delivery};
use crate::subscription::StoreEvent;

const DM: ChannelId = ChannelId::new(300_000_000_000_000_010);
const GROUP: ChannelId = ChannelId::new(300_000_000_000_000_011);

#[tokio::test]
async fn ready_crosses_as_ids_in_order() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();

    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;

    assert_eq!(
        events_until(&subscription, online).await,
        [
            StoreEvent::Connection {
                state: ConnectionState::Connecting
            },
            StoreEvent::Ready,
            StoreEvent::Connection {
                state: ConnectionState::Online
            },
        ]
    );
    account.close();
}

#[tokio::test]
async fn store_reads_convert_after_ready() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(store.connection(), ConnectionState::Online);
    assert_eq!(store.guild_ids(), [GUILD]);
    assert_eq!(
        store.guild(GUILD).map(|guild| guild.name).as_deref(),
        Some("Akari ✨ Lab")
    );
    assert_eq!(store.unavailable_guild_ids(), [DOWN]);
    assert_eq!(store.channel_list(GUILD), [VOICE, CATEGORY, GENERAL]);
    assert_eq!(store.private_channel_list(), [GROUP, DM]);
    assert_eq!(
        store.channel(GENERAL),
        Some(Channel {
            id: GENERAL,
            kind: ChannelType::GuildText,
            guild_id: Some(GUILD),
            parent_id: Some(CATEGORY),
            name: Some("general".to_owned()),
            position: 1,
            topic: None,
            nsfw: false,
            rate_limit_per_user: 0,
            recipient_ids: Vec::new(),
        })
    );
    let me = store.current_user().unwrap();
    assert_eq!(me.id, USER);
    assert_eq!(me.username, "akari_tester");
    assert_eq!(me.display_name, "Akari Tester");
    assert_eq!(store.permissions(GENERAL), Some(Permissions::ALL));
    assert_eq!(store.window(GENERAL), None);
    account.close();
}

#[tokio::test]
async fn channels_returns_known_ids_in_the_given_order() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let ids: Vec<ChannelId> = account
        .store()
        .channels(vec![GENERAL, ChannelId::new(1), VOICE])
        .into_iter()
        .map(|channel| channel.id)
        .collect();

    assert_eq!(ids, [GENERAL, VOICE]);
    account.close();
}

#[tokio::test]
async fn live_messages_cross_as_ids_and_read_as_records() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account.view_channel(GENERAL);
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["attachments"] = json!([{
        "id": "700000000000000001",
        "filename": "cat.png",
        "size": 1234,
        "url": "https://cdn.example.invalid/cat.png",
        "proxy_url": "https://media.example.invalid/cat.png",
        "content_type": "image/png",
    }]);
    message["embeds"] = json!([{"type": "rich", "title": "A link"}]);

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;

    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    assert_eq!(
        events.last(),
        Some(&StoreEvent::MessageInserted {
            channel_id: GENERAL,
            message_id: MESSAGE
        })
    );
    let store = account.store();
    let window = store.window(GENERAL).unwrap();
    assert_eq!(window.message_ids, [MESSAGE]);
    assert!(window.pending_ids.is_empty());
    assert!(window.latest && !window.oldest && !window.stale);
    let messages = store.messages(GENERAL, vec![MessageId::new(1), MESSAGE]);
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.id, MESSAGE);
    assert_eq!(message.channel_id, GENERAL);
    assert_eq!(message.kind, MessageType::Default);
    assert_eq!(message.content, "hi");
    assert_eq!(message.author.display_name, "Mira");
    assert_eq!(message.author.username, "mira");
    assert!(!message.from_webhook);
    assert_eq!(
        message.timestamp,
        UNIX_EPOCH + Duration::from_millis(1_709_281_980_000)
    );
    assert_eq!(message.edited_timestamp, None);
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.attachments[0].filename, "cat.png");
    assert_eq!(message.attachments[0].size, 1234);
    assert_eq!(
        message.attachments[0].content_type.as_deref(),
        Some("image/png")
    );
    assert_eq!(message.embed_count, 1);
    assert_eq!(message.delivery, Delivery::Sent);
    assert!(message.timestamp < SystemTime::now());
    account.close();
}

#[tokio::test]
async fn a_dm_message_moves_its_conversation_up() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["id"] = "400000000000000030".into();
    message["channel_id"] = DM.get().to_string().into();
    let fields = message.as_object_mut().unwrap();
    fields.remove("guild_id");
    fields.remove("member");

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;

    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ChannelUpdated { .. })
    })
    .await;
    assert_eq!(
        events.last(),
        Some(&StoreEvent::ChannelUpdated {
            channel_id: DM,
            guild_id: None
        })
    );
    assert_eq!(account.store().private_channel_list(), [DM, GROUP]);
    account.close();
}

#[tokio::test]
async fn batches_keep_order_and_stop_at_256() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let ids: Vec<ChannelId> = (0..600).map(|i| ChannelId::new(600_000 + i)).collect();

    for (seq, id) in (2..).zip(&ids) {
        let channel = json!({"id": id.get().to_string(), "type": 0, "guild_id": GUILD.get().to_string(),
            "name": "added", "position": 9, "permission_overwrites": []});
        send(
            &mut ws,
            json!({"op": 0, "s": seq, "t": "CHANNEL_CREATE", "d": channel}),
        )
        .await;
    }
    let store = account.store();
    timeout(WAIT, async {
        while store.channel(ids[599]).is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the channels never arrived");

    let mut added = Vec::new();
    let mut first = true;
    while added.len() < ids.len() {
        let batch = timeout(WAIT, subscription.next()).await.unwrap().unwrap();
        if first {
            assert_eq!(
                batch.len(),
                256,
                "everything was buffered, so the first batch is full"
            );
            first = false;
        }
        assert!(batch.len() <= 256, "a batch of {}", batch.len());
        added.extend(batch.into_iter().filter_map(|event| match event {
            StoreEvent::ChannelAdded { channel_id, .. } => Some(channel_id),
            _ => None,
        }));
    }
    assert_eq!(added, ids);
    account.close();
}

#[test]
fn close_ends_a_waiting_next_and_stops_buffering() {
    let client = unreachable_client(Arc::new(MemoryStore::default()));
    let account = client.account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    let inner = subscription.inner_for_tests();

    {
        let mut next = pin!(subscription.next());
        assert!(poll_once(next.as_mut()).is_pending());
        subscription.close();
        assert_eq!(poll_once(next.as_mut()), Poll::Ready(None));
    }

    assert_eq!(block_on(subscription.next()), None);
    assert!(
        inner.upgrade().is_none(),
        "the store still feeds the subscription"
    );
}

#[tokio::test]
async fn the_subscription_ends_after_the_account_closes_with_buffered_events_first() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let frame = CloseFrame {
        code: 4004.into(),
        reason: "".into(),
    };
    let _ = ws.close(Some(frame)).await;

    let events = drain(&subscription).await;
    assert_eq!(
        events.last(),
        Some(&StoreEvent::Connection {
            state: ConnectionState::Closed {
                error: Some(GatewayError::AuthenticationFailed)
            }
        }),
        "{events:?}"
    );
}

#[tokio::test]
async fn account_starts_outside_a_runtime() {
    let gateway = FakeGateway::start().await;
    let client = gateway.client();

    let (account, subscription) = std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let account = client.account(token("t")).unwrap();
        let subscription = account.store().subscribe();
        account.connect().unwrap();
        (account, subscription)
    })
    .join()
    .unwrap();
    let _ws = gateway.serve_ready().await;

    events_until(&subscription, online).await;
    account.close();
}

#[tokio::test]
async fn a_blocked_token_store_never_stalls_events() {
    let gateway = FakeGateway::start().await;
    let (store, entered, release) = BlockingStore::new();
    let endpoints = akari_core::Endpoints {
        gateway: format!("ws://{}/", gateway.address),
        api: "http://127.0.0.1:9/api/v9/".to_owned(),
        ..akari_core::Endpoints::default()
    };
    let client = local_client(endpoints, store);
    let loading = {
        let client = client.clone();
        std::thread::spawn(move || block_on(client.load_token(USER)).map(|token| token.is_some()))
    };
    entered
        .recv_timeout(WAIT)
        .expect("the token store was never called");

    let account = client.account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "CHANNEL_UPDATE", "d": {"id": GENERAL.get().to_string(), "name": "renamed"}}),
    )
    .await;

    events_until(&subscription, |event| {
        matches!(event, StoreEvent::ChannelUpdated { .. })
    })
    .await;
    release.send(()).unwrap();
    assert_eq!(loading.join().unwrap(), Ok(false));
    account.close();
}

#[test]
fn gateway_errors_map_one_to_one() {
    use akari_core::gateway::{DecodeError, GatewayError as Core};

    let cases = [
        (
            Core::AuthenticationFailed,
            GatewayError::AuthenticationFailed,
        ),
        (
            Core::Rejected { code: 4013 },
            GatewayError::Rejected { code: 4013 },
        ),
        (
            Core::MessageTooLarge { limit: 64 << 20 },
            GatewayError::MessageTooLarge { limit: 64 << 20 },
        ),
        (
            Core::InvalidReady(DecodeError::MissingField { op: 0, field: "d" }),
            GatewayError::InvalidReady,
        ),
        (Core::Closed, GatewayError::Closed),
        (Core::Stopped, GatewayError::Stopped),
        (Core::NoRuntime, GatewayError::Stopped),
    ];

    for (core, expected) in cases {
        assert_eq!(GatewayError::from(core), expected);
    }
}
