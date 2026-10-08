use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::gateway::{GatewayEvent, decode};
use crate::model::{self, Permissions, Snowflake};

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

fn raw_list(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn raw_id(value: &Value) -> u64 {
    value["id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .or_else(|| {
            value["properties"]["id"]
                .as_str()
                .and_then(|id| id.parse().ok())
        })
        .expect("a guild without an id")
}

// Run on a real READY with:
// AKARI_READY_FIXTURE="$PWD/captures/ready-<unix time>.json" \
//   cargo test -p akari-core --lib -- --ignored captured_ready_builds_the_store --nocapture
#[test]
#[ignore = "needs AKARI_READY_FIXTURE, a READY captured with `akari-cli connect --capture`"]
fn captured_ready_builds_the_store() {
    let path = std::env::var("AKARI_READY_FIXTURE")
        .expect("AKARI_READY_FIXTURE must point to a captured READY message");
    let json = std::fs::read_to_string(&path).expect("AKARI_READY_FIXTURE is not readable");
    let raw: Value = serde_json::from_str(&json).expect("AKARI_READY_FIXTURE is not JSON");
    let ready = match decode(json.as_bytes()) {
        Ok(GatewayEvent::Dispatch { event, .. }) => event,
        Ok(_) => panic!("AKARI_READY_FIXTURE is not a dispatch"),
        Err(err) => {
            let mut text = err.to_string();
            let mut source = std::error::Error::source(&err);
            while let Some(cause) = source {
                text = format!("{text}: {cause}");
                source = cause.source();
            }
            panic!("READY didn't decode: {text}");
        }
    };
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(ready);
    let data = &raw["d"];

    let (unavailable, available): (Vec<&Value>, Vec<&Value>) = raw_list(&data["guilds"])
        .iter()
        .partition(|guild| guild["unavailable"].as_bool() == Some(true));
    let mut marked: Vec<u64> = unavailable.iter().map(|guild| raw_id(guild)).collect();
    marked.sort_unstable();
    let mut found: Vec<u64> = store
        .unavailable_guilds()
        .iter()
        .map(|id| id.get())
        .collect();
    found.sort_unstable();
    assert_eq!(
        found, marked,
        "guilds became unavailable that READY didn't mark"
    );
    assert_eq!(
        store.guilds().len(),
        available.len(),
        "available guilds were lost"
    );

    let (mut channels, mut visible, mut threads) = (0, 0, 0);
    for raw_guild in available {
        let id = Snowflake::new(raw_id(raw_guild));
        assert!(store.guild(id).is_some(), "guild {} is missing", id.get());
        assert!(
            store.current_member(id).is_some(),
            "guild {} has no current member",
            id.get()
        );
        let guild_channels = store.guild_channels(id);
        let guild_threads = store.threads(id);
        assert_eq!(
            guild_channels.len() + guild_threads.len(),
            raw_list(&raw_guild["channels"]).len() + raw_list(&raw_guild["threads"]).len(),
            "guild {} lost channels or threads",
            id.get()
        );
        for channel in guild_channels.iter().chain(&guild_threads) {
            let permissions = store.permissions(channel.id).unwrap_or_else(|| {
                panic!(
                    "no permissions for channel {} in guild {}",
                    channel.id.get(),
                    id.get()
                )
            });
            if !channel.is_thread() && permissions.contains(Permissions::VIEW_CHANNEL) {
                visible += 1;
            }
        }
        channels += guild_channels.len();
        threads += guild_threads.len();
    }

    let private_channels = store.private_channels();
    assert_eq!(
        private_channels.len(),
        raw_list(&data["private_channels"]).len(),
        "private channels were lost"
    );
    for channel in &private_channels {
        for recipient in &channel.recipients {
            assert!(
                store.user(*recipient).is_some(),
                "a recipient of private channel {} doesn't resolve",
                channel.id.get()
            );
        }
    }

    println!(
        "guilds: {}, channels: {channels}, visible channels: {visible}, threads: {threads}, DMs: {}",
        store.guilds().len(),
        private_channels.len()
    );
}
