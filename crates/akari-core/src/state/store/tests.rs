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
fn a_ready_is_built_and_diffed_outside_the_lock() {
    let store = ready_store();
    let (parked, release) = store.park_next_conversion();
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

#[test]
fn a_later_ready_is_diffed_outside_the_lock() {
    let store = ready_store();
    let (parked, on_parked) = std_mpsc::channel();
    let (release, on_release) = std_mpsc::channel::<()>();
    let applying = {
        let store = store.clone();
        thread::spawn(move || {
            crate::state::apply::tests::on_next_diff(move || {
                let _ = parked.send(());
                let _ = on_release.recv();
            });
            store.apply(ready());
        })
    };
    on_parked
        .recv_timeout(WAIT)
        .expect("READY never reached the diff");

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

    assert_eq!(guilds, Ok(1), "a read waited for the READY diff");
}

fn guild_create() -> DispatchEvent {
    dispatch(
        "GUILD_CREATE",
        &fixture(include_str!("../../../tests/fixtures/guild_create.json")),
    )
}

#[test]
fn a_guild_create_is_converted_outside_the_lock() {
    let store = ready_store();
    let (parked, release) = store.park_next_conversion();
    let applying = {
        let store = store.clone();
        thread::spawn(move || store.apply(guild_create()))
    };
    parked
        .recv_timeout(WAIT)
        .expect("GUILD_CREATE never reached the conversion");

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

    assert_eq!(
        guilds,
        Ok(1),
        "a read waited for the GUILD_CREATE conversion"
    );
    assert_eq!(store.guilds().len(), 2);
}

#[test]
fn a_long_write_warns_with_its_event_name_only() {
    let store = ready_store();
    store.delay_next_write(Duration::from_millis(5));

    store.apply(guild_create());

    let long = store.long_holds();
    assert!(
        long.iter()
            .any(|(event, held)| *event == "GUILD_CREATE" && *held > Duration::from_millis(4)),
        "{long:?}"
    );
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

#[tokio::test]
async fn next_batch_returns_buffered_events_in_order_up_to_max() {
    let store = ready_store();
    let subscription = store.subscribe();
    for name in ["a", "b", "c", "d", "e"] {
        store.apply(rename(GENERAL, name));
    }

    let first = subscription.next_batch(3).await;
    let second = subscription.next_batch(3).await;

    let names = |batch: &[StoreEvent]| -> Vec<String> {
        batch
            .iter()
            .map(|event| match event {
                StoreEvent::ChannelUpdated(channel) => {
                    channel.name.as_deref().unwrap_or_default().to_owned()
                }
                other => describe(other),
            })
            .collect()
    };
    assert_eq!(names(&first), ["a", "b", "c"]);
    assert_eq!(names(&second), ["d", "e"]);
}

#[tokio::test]
async fn next_batch_waits_for_the_first_event() {
    let store = ready_store();
    let subscription = store.subscribe();

    let (batch, ()) = tokio::join!(
        tokio::time::timeout(WAIT, subscription.next_batch(10)),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            store.apply(rename(GENERAL, "late"));
        }
    );

    let batch = batch.expect("the batch never arrived");
    assert_eq!(batch.len(), 1);
    assert_eq!(describe(&batch[0]), format!("ChannelUpdated({GENERAL})"));
}

#[tokio::test]
async fn next_batch_of_zero_takes_one() {
    let store = ready_store();
    let subscription = store.subscribe();
    store.apply(rename(GENERAL, "a"));
    store.apply(rename(GENERAL, "b"));

    assert_eq!(subscription.next_batch(0).await.len(), 1);
    assert_eq!(subscription.next_batch(0).await.len(), 1);
}

#[tokio::test]
async fn next_batch_is_empty_once_finished_and_drained() {
    let store = ready_store();
    let subscription = store.subscribe();
    store.apply(rename(GENERAL, "a"));
    store.apply(rename(GENERAL, "b"));
    store.finish();

    assert_eq!(subscription.next_batch(10).await.len(), 2);
    assert!(subscription.next_batch(10).await.is_empty());
    assert!(subscription.next_batch(10).await.is_empty());
}

#[tokio::test]
async fn next_batch_lowers_the_backlog() {
    let store = ready_store();
    let subscription = store.subscribe();
    for name in ["a", "b", "c", "d"] {
        store.apply(rename(GENERAL, name));
    }
    assert_eq!(subscription.buffered(), 4);

    let _ = subscription.next_batch(3).await;

    assert_eq!(subscription.buffered(), 1);
}

const CATEGORY: u64 = 300_000_000_000_000_001;
const VOICE: u64 = 300_000_000_000_000_003;

fn ids<M>(list: &[Arc<impl HasId<M>>]) -> Vec<u64> {
    list.iter().map(|item| item.id().get()).collect()
}

trait HasId<M> {
    fn id(&self) -> Snowflake<M>;
}

impl HasId<model::ChannelMarker> for Channel {
    fn id(&self) -> ChannelId {
        self.id
    }
}

impl HasId<model::GuildMarker> for Guild {
    fn id(&self) -> GuildId {
        self.id
    }
}

// The fixture's user owns G1 and is a moderator there; both would show every channel.
fn ready_as_member(general_overwrites: Value) -> DispatchEvent {
    let mut data = fixture(include_str!("../../../tests/fixtures/ready.json"))["d"].clone();
    let guild = &mut data["guilds"][0];
    guild["properties"]["owner_id"] = "100000000000000099".into();
    for channel in guild["channels"].as_array_mut().unwrap() {
        if channel["id"].as_str().and_then(|id| id.parse().ok()) == Some(GENERAL) {
            channel["permission_overwrites"] = general_overwrites.clone();
        }
    }
    data["merged_members"][0][0]["roles"] = json!([]);
    dispatch("READY", &data)
}

#[test]
fn channel_list_shows_what_the_user_can_view() {
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(ready_as_member(json!([])));

    assert_eq!(
        ids(&store.channel_list(Snowflake::new(G1))),
        [VOICE, CATEGORY, GENERAL]
    );
}

#[test]
fn channel_list_applies_permissions() {
    let store = Store::new(DEFAULT_LIMITS);
    let deny_view = json!([{
        "id": G1.to_string(),
        "type": 0,
        "allow": "0",
        "deny": Permissions::VIEW_CHANNEL.0.to_string(),
    }]);
    store.apply(ready_as_member(deny_view));

    assert_eq!(ids(&store.channel_list(Snowflake::new(G1))), [VOICE]);
}

#[test]
fn channel_list_of_an_unknown_guild_is_empty() {
    let store = ready_store();

    assert!(store.channel_list(Snowflake::new(G3)).is_empty());
}

#[test]
fn guild_list_reads_join_dates_from_the_current_member() {
    let store = ready_store();
    store.apply(dispatch(
        "GUILD_CREATE",
        &fixture(include_str!("../../../tests/fixtures/guild_create.json")),
    ));

    assert_eq!(ids(&store.guild_list()), [G3, G1]);
}

// Synthetic IDs, small and unique per guild; snowflake values don't matter here.
fn generated_guild(guild: u64, channels: u64, roles: u64, threads: u64) -> Value {
    let mut data = fixture(include_str!("../../../tests/fixtures/guild_create.json"));
    let channel = data["channels"][0].clone();
    let role = data["roles"][0].clone();
    let thread = fixture(include_str!("../../../tests/fixtures/thread_create.json"));
    let base = guild * 1_000_000;
    data["id"] = guild.to_string().into();
    data["properties"]["id"] = guild.to_string().into();
    let mut category = base;
    data["channels"] = (0..channels)
        .map(|i| {
            let mut next = channel.clone();
            let id = base + 1 + i;
            next["id"] = id.to_string().into();
            next["position"] = i.into();
            if i % 20 == 0 {
                next["type"] = 4.into();
                category = id;
            } else {
                next["parent_id"] = category.to_string().into();
            }
            next["permission_overwrites"] = json!([
                {"id": guild.to_string(), "type": 0, "allow": "0", "deny": "2048"},
                {"id": (base + 500_001).to_string(), "type": 0, "allow": "2048", "deny": "0"},
            ]);
            next
        })
        .collect();
    data["roles"] = (0..roles)
        .map(|i| {
            let mut next = role.clone();
            let id = if i == 0 { guild } else { base + 500_000 + i };
            next["id"] = id.to_string().into();
            next["position"] = i.into();
            next
        })
        .collect();
    data["threads"] = (0..threads)
        .map(|i| {
            let mut next = thread.clone();
            next["id"] = (base + 800_000 + i).to_string().into();
            next["guild_id"] = guild.to_string().into();
            next["parent_id"] = (base + 2).to_string().into();
            next
        })
        .collect();
    data
}

fn generated_ready(guilds: u64, channels: u64, roles: u64, threads: u64) -> DispatchEvent {
    let mut data = fixture(include_str!("../../../tests/fixtures/ready.json"))["d"].clone();
    let member = data["merged_members"][0][0].clone();
    data["guilds"] = (1..=guilds)
        .map(|guild| generated_guild(guild, channels, roles, threads))
        .collect();
    data["merged_members"] = (1..=guilds).map(|_| json!([member.clone()])).collect();
    dispatch("READY", &data)
}

fn page(template: &model::Message, first: u64, count: u64) -> Vec<model::Message> {
    (first..first + count)
        .map(|id| {
            let mut message = template.clone();
            message.id = Snowflake::new(id);
            message
        })
        .collect()
}

#[test]
#[ignore = "prints write-lock hold times; run in release with --nocapture"]
fn report_write_lock_hold_times() {
    let mut longest = std::collections::BTreeMap::<String, Duration>::new();
    let mut note = |label: &str, store: &Store| {
        for (event, held) in store.take_holds() {
            let entry = longest.entry(format!("{label}: {event}")).or_default();
            *entry = (*entry).max(held);
        }
    };
    let template = message_template();

    let store = Store::new(DEFAULT_LIMITS);
    store.record_holds();
    store.apply(ready());
    store.apply(ready());
    store.apply(guild_create());
    store.apply(rename(GENERAL, "renamed"));
    store.view_channel(Snowflake::new(GENERAL));
    store.apply(message_create(&template, 400_000_000_000_000_100));
    let fixture_page: Vec<model::Message> =
        serde_json::from_str(include_str!("../../../tests/fixtures/messages_page.json")).unwrap();
    let ticket = store
        .begin_load(Snowflake::new(GENERAL), LoadKind::Older)
        .unwrap();
    store.finish_load(ticket, fixture_page, false);
    note("fixtures", &store);

    let store = Store::new(DEFAULT_LIMITS);
    store.record_holds();
    store.apply(generated_ready(100, 100, 50, 20));
    let DispatchEvent::Ready(second) = generated_ready(100, 100, 50, 20) else {
        unreachable!()
    };
    let started = std::time::Instant::now();
    let prepared = store.prepare_ready(*second);
    let outside = started.elapsed();
    store.replace(prepared);
    println!(
        "{:>8.3} ms  outside the lock: converting and diffing the generated READY",
        outside.as_secs_f64() * 1000.0
    );
    let started = std::time::Instant::now();
    store.apply(dispatch(
        "GUILD_CREATE",
        &generated_guild(500, 500, 250, 100),
    ));
    println!(
        "{:>8.3} ms  GUILD_CREATE of the 500-channel guild, all of it",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(store.guilds().len(), 101);
    assert_eq!(store.guild_channels(Snowflake::new(500)).len(), 500);
    assert_eq!(store.threads(Snowflake::new(500)).len(), 100);
    assert_eq!(store.guild_channels(Snowflake::new(42)).len(), 100);
    let channel = Snowflake::new(1_000_002);
    store.view_channel(channel);
    let mut template = template;
    template.channel_id = channel;
    let ticket = store.begin_load(channel, LoadKind::Latest).unwrap();
    store.finish_load(ticket, page(&template, 10_000, 100), false);
    store.apply(message_create(&template, 20_000));
    note(
        "generated (100 guilds x 100 channels; one 500-channel guild; 100 messages)",
        &store,
    );

    for (event, held) in longest {
        println!("{:>8.3} ms  {event}", held.as_secs_f64() * 1000.0);
    }
}

#[test]
fn a_window_opened_between_prepare_and_swap_is_dropped_with_its_channel() {
    let store = ready_store();
    let mut data = fixture(include_str!("../../../tests/fixtures/ready.json"))["d"].clone();
    data["guilds"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .retain(|channel| channel["id"].as_str().and_then(|id| id.parse().ok()) != Some(GENERAL));
    let DispatchEvent::Ready(ready) = dispatch("READY", &data) else {
        unreachable!()
    };

    let prepared = store.prepare_ready(*ready);
    store.view_channel(Snowflake::new(GENERAL));
    assert!(store.messages(Snowflake::new(GENERAL)).is_some());
    store.replace(prepared);

    assert!(store.channel(Snowflake::new(GENERAL)).is_none());
    assert!(store.messages(Snowflake::new(GENERAL)).is_none());
}

#[test]
fn private_channel_list_follows_the_latest_message() {
    let store = ready_store();
    let dm = 300_000_000_000_000_010;
    let group = 300_000_000_000_000_011;
    let mut message = message_template();
    message.channel_id = Snowflake::new(dm);
    message.guild_id = None;

    let before = ids(&store.private_channel_list());
    store.apply(message_create(&message, 400_000_000_000_000_030));

    assert_eq!(before, [group, dm]);
    assert_eq!(ids(&store.private_channel_list()), [dm, group]);
}
