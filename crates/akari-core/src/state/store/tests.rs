use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::gateway::{GatewayEvent, decode};
use crate::model::{self, Snowflake};

const G1: u64 = 200_000_000_000_000_001;
const G3: u64 = 200_000_000_000_000_003;
const GENERAL: u64 = 300_000_000_000_000_002;
const WAIT: Duration = Duration::from_secs(5);

fn dispatch(name: &str, data: &Value) -> DispatchEvent {
    let payload = json!({"op": 0, "s": 1, "t": name, "d": data});
    match decode(payload.to_string().as_bytes()) {
        Ok(GatewayEvent::Dispatch { event, .. }) => event,
        other => panic!("expected a dispatch, got {other:?}"),
    }
}

fn fixture(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn ready() -> DispatchEvent {
    dispatch(
        "READY",
        &fixture(include_str!("../../../tests/fixtures/ready.json"))["d"],
    )
}

fn ready_store() -> Store {
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(ready());
    store
}

fn rename(channel: u64, name: &str) -> DispatchEvent {
    dispatch(
        "CHANNEL_UPDATE",
        &json!({"id": channel.to_string(), "name": name}),
    )
}

fn describe(event: &StoreEvent) -> String {
    match event {
        StoreEvent::Ready => "Ready".to_owned(),
        StoreEvent::GuildUpdated(guild) => format!("GuildUpdated({})", guild.id.get()),
        StoreEvent::ChannelUpdated(channel) => format!("ChannelUpdated({})", channel.id.get()),
        StoreEvent::Connection(state) => format!("Connection({state:?})"),
        StoreEvent::MessageInserted(message) => format!("MessageInserted({})", message.id.get()),
        other => format!("{other:?}"),
    }
}

async fn next(subscription: &Subscription) -> Option<String> {
    tokio::time::timeout(WAIT, subscription.next())
        .await
        .expect("no event arrived")
        .map(|event| describe(&event))
}

fn message_create(template: &model::Message, id: u64) -> DispatchEvent {
    let mut message = template.clone();
    message.id = Snowflake::new(id);
    DispatchEvent::MessageCreate(Box::new(message))
}

fn message_template() -> model::Message {
    let mut message: model::Message =
        serde_json::from_str(include_str!("../../../tests/fixtures/message_create.json")).unwrap();
    message.channel_id = Snowflake::new(GENERAL);
    message
}

#[tokio::test]
async fn subscribers_get_events_in_order() {
    let store = Store::new(DEFAULT_LIMITS);
    let first = store.subscribe();
    let second = store.subscribe();

    store.apply(ready());
    store.apply(dispatch(
        "GUILD_UPDATE",
        &json!({"id": G1.to_string(), "name": "New"}),
    ));
    store.apply(rename(GENERAL, "renamed"));

    for subscription in [&first, &second] {
        assert_eq!(next(subscription).await.as_deref(), Some("Ready"));
        assert_eq!(
            next(subscription).await,
            Some(format!("GuildUpdated({G1})"))
        );
        assert_eq!(
            next(subscription).await,
            Some(format!("ChannelUpdated({GENERAL})"))
        );
    }
}

#[tokio::test]
async fn a_subscriber_sees_only_changes_after_subscribing() {
    let store = ready_store();
    let subscription = store.subscribe();

    store.apply(rename(GENERAL, "renamed"));

    assert_eq!(
        next(&subscription).await,
        Some(format!("ChannelUpdated({GENERAL})"))
    );
}

#[test]
fn a_dropped_subscription_is_forgotten() {
    let store = ready_store();
    let kept = store.subscribe();
    drop(store.subscribe());

    store.apply(rename(GENERAL, "renamed"));

    assert_eq!(store.subscriber_count(), 1);
    drop(kept);
}

#[tokio::test]
async fn a_stalled_subscriber_never_blocks_the_store() {
    let store = ready_store();
    store.view_channel(Snowflake::new(GENERAL));
    let stalled = store.subscribe();
    let reader = store.subscribe();
    let template = message_template();
    let last = 400_000_000_000_020_000;
    let reading = tokio::spawn(async move {
        let mut seen = 0_usize;
        while let Some(event) = reader.next().await {
            seen += 1;
            if let StoreEvent::MessageInserted(message) = event
                && message.id.get() == last
            {
                return seen;
            }
        }
        seen
    });

    let writing = thread::spawn(move || {
        for id in (last - 19_999)..=last {
            store.apply(message_create(&template, id));
        }
        store
    });
    let store = writing.join().unwrap();
    let seen = tokio::time::timeout(Duration::from_secs(30), reading)
        .await
        .expect("the reading subscriber fell behind for good")
        .unwrap();

    // 20,000 inserts, and a trim for each one past the first 200.
    assert_eq!(stalled.buffered(), 39_800);
    assert_eq!(seen, 39_799);
    assert_eq!(
        store
            .messages(Snowflake::new(GENERAL))
            .unwrap()
            .messages
            .len(),
        200
    );
    assert_eq!(
        next(&stalled).await,
        Some(format!("MessageInserted({})", last - 19_999))
    );
}

#[test]
fn snapshots_never_change_and_reads_stay_consistent() {
    let store = ready_store();
    store.view_channel(Snowflake::new(GENERAL));
    store.apply(message_create(&message_template(), 10));
    let channel = store.channel(Snowflake::new(GENERAL)).unwrap();
    let channel_copy = (*channel).clone();
    let window = store.messages(Snowflake::new(GENERAL)).unwrap();
    let window_ids: Vec<_> = window.messages.iter().map(|message| message.id).collect();

    store.apply(rename(GENERAL, "renamed"));
    store.apply(dispatch(
        "MESSAGE_DELETE",
        &json!({"id": "10", "channel_id": GENERAL.to_string()}),
    ));

    assert_eq!(*channel, channel_copy);
    assert_eq!(
        window
            .messages
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>(),
        window_ids
    );

    let guild = fixture(include_str!("../../../tests/fixtures/guild_create.json"));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let store = store.clone();
            thread::spawn(move || {
                for _ in 0..2_000 {
                    for id in [G1, G3] {
                        let channels = store.guild_channels(Snowflake::new(id));
                        let mut ids: Vec<_> = channels.iter().map(|channel| channel.id).collect();
                        assert!(
                            channels
                                .iter()
                                .all(|channel| channel.guild_id == Some(Snowflake::new(id)))
                        );
                        ids.sort_unstable();
                        ids.dedup();
                        assert_eq!(ids.len(), channels.len());
                    }
                    if let Some(channel) = store.channel(Snowflake::new(GENERAL)) {
                        assert_eq!(channel.id, Snowflake::new(GENERAL));
                    }
                }
            })
        })
        .collect();
    for round in 0..500 {
        store.apply(dispatch("GUILD_CREATE", &guild));
        store.apply(rename(GENERAL, &format!("round {round}")));
        store.apply(dispatch("GUILD_DELETE", &json!({"id": G3.to_string()})));
    }
    for reader in readers {
        reader.join().unwrap();
    }
}

#[tokio::test]
async fn finish_ends_subscriptions_after_their_buffered_events() {
    let store = ready_store();
    let subscription = store.subscribe();
    store.apply(rename(GENERAL, "renamed"));

    store.finish();

    assert_eq!(
        next(&subscription).await,
        Some(format!("ChannelUpdated({GENERAL})"))
    );
    assert_eq!(next(&subscription).await, None);
}

#[tokio::test]
async fn subscribing_after_finish_ends_at_once() {
    let store = ready_store();
    store.finish();

    let subscription = store.subscribe();
    store.apply(rename(GENERAL, "renamed"));

    assert_eq!(next(&subscription).await, None);
    assert_eq!(
        store
            .channel(Snowflake::new(GENERAL))
            .and_then(|channel| channel.name.clone()),
        Some("renamed".into())
    );
}

#[tokio::test]
async fn connection_changes_are_state_and_events() {
    let store = Store::new(DEFAULT_LIMITS);
    let subscription = store.subscribe();

    store.set_connection(ConnectionState::Connecting);
    store.set_connection(ConnectionState::Connecting);
    store.set_connection(ConnectionState::Online);

    assert!(matches!(store.connection(), ConnectionState::Online));
    assert_eq!(
        next(&subscription).await.as_deref(),
        Some("Connection(Connecting)")
    );
    assert_eq!(
        next(&subscription).await.as_deref(),
        Some("Connection(Online)")
    );
}

#[test]
fn a_ready_is_built_outside_the_lock() {
    let store = ready_store();
    let (parked, release) = store.park_next_ready();
    let applying = {
        let store = store.clone();
        thread::spawn(move || store.apply(ready()))
    };
    parked
        .recv_timeout(WAIT)
        .expect("READY never reached the build");

    let (read, done) = std_mpsc::channel();
    {
        let store = store.clone();
        thread::spawn(move || {
            let _ = read.send(store.guilds().len());
        });
    }
    let guilds = done.recv_timeout(WAIT);
    release.send(()).unwrap();
    applying.join().unwrap();

    assert_eq!(guilds, Ok(1), "a read waited for the READY build");
}
