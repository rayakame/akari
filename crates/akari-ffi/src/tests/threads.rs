use akari_core::model::{ChannelId, UserId};
use serde_json::{Value, json};

use super::fake::{CATEGORY, FakeGateway, GENERAL, GUILD, VOICE, events_until, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::records::ThreadInfo;
use crate::subscription::StoreEvent;

// READY's thread in GENERAL, its last message 400000000000000030.
const THREAD: ChannelId = ChannelId::new(300_000_000_000_000_020);
const MIRA: UserId = UserId::new(100_000_000_000_000_002);

// Past its window, the thread is listed only while unread.
fn unread_thread(ready: &mut Value) {
    ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
}

#[tokio::test]
async fn a_threads_record_carries_its_state() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    let thread = store.channel(THREAD).and_then(|channel| channel.thread);

    assert_eq!(
        thread,
        Some(ThreadInfo {
            joined: true,
            archived: false,
            locked: false,
            private: false,
            message_count: Some(12),
            owner_id: Some(MIRA),
        })
    );
    assert_eq!(
        store.channel(GENERAL).and_then(|channel| channel.thread),
        None
    );
    account.close();
}

#[tokio::test]
async fn channel_list_includes_listed_threads() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready_with(unread_thread).await;
    events_until(&subscription, online).await;

    assert_eq!(
        account.store().channel_list(GUILD),
        [VOICE, CATEGORY, GENERAL, THREAD]
    );
    account.close();
}

#[tokio::test]
async fn a_listing_change_reaches_the_subscription() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready_with(unread_thread).await;
    events_until(&subscription, online).await;

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_ACK", "d": {
            "channel_id": THREAD.get().to_string(),
            "message_id": "400000000000000030", "version": 2
        }}),
    )
    .await;
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ThreadListingChanged { .. })
    })
    .await;

    assert!(events.contains(&StoreEvent::ThreadListingChanged {
        guild_id: GUILD,
        thread_id: THREAD,
    }));
    assert_eq!(
        account.store().channel_list(GUILD),
        [VOICE, CATEGORY, GENERAL]
    );
    account.close();
}

#[tokio::test]
async fn muted_channels_covers_threads() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway
        .serve_ready_with(|ready| {
            ready["guilds"][0]["threads"][0]["member"]["muted"] = true.into();
        })
        .await;
    events_until(&subscription, online).await;

    assert_eq!(
        account.store().muted_channels(vec![THREAD, GENERAL]),
        [THREAD]
    );
    account.close();
}
