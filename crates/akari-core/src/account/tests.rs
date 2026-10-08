use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::timeout;

use super::*;
use crate::gateway::fake::{FakeConnection, FakeGateway, WAIT, client, timing};
use crate::model::Snowflake;
use crate::state::{DEFAULT_LIMITS, StoreEvent, Subscription};

const TOKEN: &str = "account-test-token.secret";
const GENERAL: u64 = 300_000_000_000_000_002;
const READY: &str = include_str!("../../tests/fixtures/ready.json");

fn start(fake: &FakeGateway) -> Account {
    Account::start(
        client(fake),
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
    )
    .unwrap()
}

fn describe(event: &StoreEvent) -> String {
    match event {
        StoreEvent::Connection(ConnectionState::Closed { error: Some(error) }) => {
            format!("Closed({error:?})")
        }
        StoreEvent::Connection(ConnectionState::Closed { error: None }) => "Closed".to_owned(),
        StoreEvent::Connection(state) => format!("{state:?}"),
        StoreEvent::Ready => "Ready".to_owned(),
        StoreEvent::ChannelUpdated(channel) => format!("ChannelUpdated({})", channel.id.get()),
        StoreEvent::MessageInserted(message) => format!("MessageInserted({})", message.id.get()),
        StoreEvent::MessagesStale { channel_id } => format!("MessagesStale({})", channel_id.get()),
        other => format!("{other:?}"),
    }
}

async fn next(subscription: &Subscription) -> Option<String> {
    timeout(WAIT, subscription.next())
        .await
        .expect("no event arrived")
        .map(|event| describe(&event))
}

async fn events_until(subscription: &Subscription, last: &str) -> Vec<String> {
    let mut events = Vec::new();
    while let Some(event) = next(subscription).await {
        let done = event == last;
        events.push(event);
        if done {
            return events;
        }
    }
    panic!("the subscription ended before {last}: {events:?}");
}

fn ready_payload(seq: u64, fake: &FakeGateway, edit: impl FnOnce(&mut Value)) -> Value {
    let mut payload: Value = serde_json::from_str(READY).unwrap();
    payload["s"] = seq.into();
    payload["d"]["session_id"] = "session-1".into();
    payload["d"]["resume_gateway_url"] = fake.resume_url().into();
    edit(&mut payload["d"]);
    payload
}

async fn online(fake: &mut FakeGateway, account: &Account) -> FakeConnection {
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    assert_eq!(connection.handshake(60_000).await["op"], 2);
    connection.send(ready_payload(1, fake, |_| {})).await;
    connection
}

fn message(seq: u64, id: u64) -> Value {
    let mut message: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/message_create.json")).unwrap();
    message["id"] = id.to_string().into();
    message["channel_id"] = GENERAL.to_string().into();
    json!({"op": 0, "s": seq, "t": "MESSAGE_CREATE", "d": message})
}

fn rename(seq: u64, name: &str) -> Value {
    json!({"op": 0, "s": seq, "t": "CHANNEL_UPDATE", "d": {"id": GENERAL.to_string(), "name": name}})
}

#[tokio::test]
async fn connecting_fills_the_store_from_ready() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    assert!(matches!(
        account.store().connection(),
        ConnectionState::Offline
    ));

    let _connection = online(&mut fake, &account).await;

    assert_eq!(
        events_until(&subscription, "Online").await,
        ["Connecting", "Ready", "Online"]
    );
    assert_eq!(account.store().guilds().len(), 1);
}

#[tokio::test]
async fn dispatches_after_ready_reach_subscribers() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    connection.send(rename(2, "renamed")).await;

    assert_eq!(
        next(&subscription).await,
        Some(format!("ChannelUpdated({GENERAL})"))
    );
}

#[tokio::test]
async fn a_broken_dispatch_is_skipped_and_the_next_one_applies() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(Snowflake::new(GENERAL));
    let mut broken = message(2, 10);
    broken["d"]["author"] = json!({"username": "leak-me"});

    connection.send(broken).await;
    connection.send(message(3, 11)).await;

    assert_eq!(
        next(&subscription).await.as_deref(),
        Some("MessageInserted(11)")
    );
}

#[tokio::test]
async fn a_reconnect_shows_as_connecting_and_resumed_changes_nothing() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    connection.close(4000).await;
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;

    assert_eq!(
        events_until(&subscription, "Online").await,
        ["Connecting", "Online"]
    );
}

#[tokio::test]
async fn a_new_session_after_reconnecting_is_diffed() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(Snowflake::new(GENERAL));
    connection.send(message(2, 10)).await;
    events_until(&subscription, "MessageInserted(10)").await;

    connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    let renamed = ready_payload(1, &fake, |ready| {
        ready["guilds"][0]["channels"][1]["name"] = "renamed".into();
    });
    fresh.send(renamed).await;

    assert_eq!(
        events_until(&subscription, "Online").await,
        [
            "Connecting".to_owned(),
            format!("ChannelUpdated({GENERAL})"),
            format!("MessagesStale({GENERAL})"),
            "Ready".to_owned(),
            "Online".to_owned(),
        ]
    );
    let window = account.store().messages(Snowflake::new(GENERAL)).unwrap();
    assert!(window.stale);
    assert_eq!(window.messages.len(), 1);
}

#[tokio::test]
async fn disconnect_shows_offline() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.disconnect();

    assert_eq!(connection.client_close_code().await, Some(4000));
    assert_eq!(next(&subscription).await.as_deref(), Some("Offline"));
    assert!(
        timeout(Duration::from_millis(300), subscription.next())
            .await
            .is_err()
    );
    assert!(matches!(
        account.store().connection(),
        ConnectionState::Offline
    ));
}

#[test]
fn stale_events_cant_undo_a_disconnect() {
    let store = Store::new(DEFAULT_LIMITS);
    let reconnecting = || ConnectionEvent::Reconnecting {
        resume: true,
        delay: Duration::ZERO,
        reason: crate::gateway::DisconnectReason::Requested,
    };

    on_event(
        &store,
        ConnectionEvent::Dispatch(DispatchEvent::Resumed),
        &|| false,
    );
    on_event(&store, reconnecting(), &|| false);
    let offline = store.connection();
    on_event(&store, reconnecting(), &|| true);
    let reconnecting_state = store.connection();
    on_event(
        &store,
        ConnectionEvent::Dispatch(DispatchEvent::Resumed),
        &|| true,
    );

    assert!(matches!(offline, ConnectionState::Offline));
    assert!(matches!(reconnecting_state, ConnectionState::Connecting));
    assert!(matches!(store.connection(), ConnectionState::Online));
}

#[tokio::test]
async fn a_rejected_token_closes_with_the_error() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();

    account.connect().unwrap();
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;
    connection.close(4004).await;

    assert_eq!(
        events_until(&subscription, "Closed(AuthenticationFailed)").await,
        ["Connecting", "Closed(AuthenticationFailed)"]
    );
    assert_eq!(next(&subscription).await, None);
    assert!(account.connect().is_err());
}

#[tokio::test]
async fn dropping_the_account_ends_the_session() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let store = account.store().clone();
    let subscription = store.subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    drop(account);

    assert_eq!(connection.client_close_code().await, Some(1000));
    assert_eq!(next(&subscription).await.as_deref(), Some("Closed"));
    assert_eq!(next(&subscription).await, None);
    assert_eq!(store.guilds().len(), 1);
}

#[tokio::test]
async fn view_channel_collects_live_messages() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.view_channel(Snowflake::new(GENERAL));
    connection.send(message(2, 10)).await;

    assert_eq!(
        next(&subscription).await.as_deref(),
        Some("MessageInserted(10)")
    );
    assert!(
        account
            .store()
            .message(Snowflake::new(GENERAL), Snowflake::new(10))
            .is_some()
    );
}

#[test]
fn a_disconnect_during_ready_leaves_the_account_offline() {
    let store = Store::new(DEFAULT_LIMITS);
    let wanted = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let ready: Value = serde_json::from_str(READY).unwrap();
    let ready = match crate::gateway::decode(ready.to_string().as_bytes()) {
        Ok(crate::gateway::GatewayEvent::Dispatch { event, .. }) => event,
        other => panic!("expected READY, got {other:?}"),
    };
    let (parked, release) = store.park_next_ready();
    let applying = {
        let store = store.clone();
        let wanted = wanted.clone();
        std::thread::spawn(move || {
            on_event(&store, ConnectionEvent::Dispatch(ready), &|| {
                wanted.load(std::sync::atomic::Ordering::SeqCst)
            });
        })
    };
    parked.recv_timeout(WAIT).unwrap();

    wanted.store(false, std::sync::atomic::Ordering::SeqCst);
    store.set_connection(ConnectionState::Offline);
    release.send(()).unwrap();
    applying.join().unwrap();

    assert!(
        matches!(store.connection(), ConnectionState::Offline),
        "{:?}",
        store.connection()
    );
}

#[tokio::test]
async fn an_undecodable_ready_closes_without_quoting_it() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;

    connection
        .send(ready_payload(1, &fake, |ready| {
            ready["user"]["id"] = "leak-me".into()
        }))
        .await;
    let mut closed = None;
    while let Some(event) = timeout(WAIT, subscription.next()).await.unwrap() {
        if let StoreEvent::Connection(ConnectionState::Closed { .. }) = &event {
            closed = Some(event);
        }
    }

    let closed = closed.expect("the account never closed");
    let ConnectionState::Closed { error: Some(error) } = account.store().connection() else {
        panic!("expected a fatal error");
    };
    assert!(matches!(*error, GatewayError::InvalidReady(_)));
    let mut shown = format!("{closed:?} {account:?} {error:?} {error}");
    let mut source = std::error::Error::source(&*error);
    while let Some(cause) = source {
        shown.push_str(&format!(" {cause} {cause:?}"));
        source = cause.source();
    }
    assert!(!shown.contains("leak-me"), "{shown}");
    assert!(shown.contains("line 1"), "{shown}");
}

fn stopped(state: &ConnectionState) -> bool {
    matches!(state, ConnectionState::Closed { error: Some(error) } if matches!(**error, GatewayError::Stopped))
}

fn drain(subscription: &Subscription) -> Vec<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut events = Vec::new();
        while let Some(event) = timeout(WAIT, subscription.next())
            .await
            .expect("the subscription never ended")
        {
            events.push(describe(&event));
        }
        events
    })
}

#[test]
fn a_runtime_shutdown_closes_the_account() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (account, subscription) = runtime.block_on(async {
        let account = Account::start(
            crate::gateway::fake::client_for("ws://127.0.0.1:9/".to_owned()),
            Token::new(TOKEN.to_owned()),
            timing(),
            DEFAULT_LIMITS,
        )
        .unwrap();
        let subscription = account.store().subscribe();
        (account, subscription)
    });

    drop(runtime);

    assert_eq!(drain(&subscription), ["Closed(Stopped)"]);
    assert!(stopped(&account.store().connection()));
}

#[test]
fn a_panicking_task_still_closes_the_account() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (account, subscription, finish) = runtime.block_on(async {
        let account = Account::start(
            crate::gateway::fake::client_for("ws://127.0.0.1:9/".to_owned()),
            Token::new(TOKEN.to_owned()),
            timing(),
            DEFAULT_LIMITS,
        )
        .unwrap();
        let subscription = account.store().subscribe();
        let finish = Finish::new(account.gateway.clone(), account.store().clone());
        (account, subscription, finish)
    });

    let task = runtime.spawn(async move {
        let _finish = finish;
        panic!("the pump broke");
    });
    let result = runtime.block_on(task);

    assert!(result.is_err_and(|err| err.is_panic()));
    assert_eq!(drain(&subscription), ["Closed(Stopped)"]);
    assert!(stopped(&account.store().connection()));
    assert!(account.connect().is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn ready_is_converted_off_the_runtime() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();

    let _connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    let converted = account.store().ready_thread();
    assert!(converted.is_some());
    assert_ne!(converted, Some(std::thread::current().id()));
}
