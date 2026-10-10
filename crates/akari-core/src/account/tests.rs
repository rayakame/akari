use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::timeout;

use super::*;
use crate::gateway::fake::{FakeConnection, FakeGateway, WAIT, client, timing};
use crate::gateway::{GatewayCommand, PresenceStatus};
use crate::model::Snowflake;
use crate::state::{DEFAULT_LIMITS, Delivery, StoreEvent, Subscription};

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
        StoreEvent::MessageUpdated(message) => format!("MessageUpdated({})", message.id.get()),
        StoreEvent::MessageDeleted { message_id, .. } => {
            format!("MessageDeleted({})", message_id.get())
        }
        StoreEvent::MessagesLoaded { first, last, .. } => {
            format!("MessagesLoaded({}..{})", first.get(), last.get())
        }
        StoreEvent::MessagesCleared { .. } => "MessagesCleared".to_owned(),
        StoreEvent::MessageReplaced { message, .. } => {
            format!("MessageReplaced({})", message.id.get())
        }
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
    let (parked, release) = store.park_next_conversion();
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
        let finish = Finish::new(account.shared.clone());
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

#[test]
fn a_task_that_stops_itself_closes_the_gateway() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (account, mut finish) = runtime.block_on(async {
        let account = Account::start(
            crate::gateway::fake::client_for("ws://127.0.0.1:9/".to_owned()),
            Token::new(TOKEN.to_owned()),
            timing(),
            DEFAULT_LIMITS,
        )
        .unwrap();
        let finish = Finish::new(account.shared.clone());
        (account, finish)
    });

    finish.ended = Some(Some(Arc::new(GatewayError::Stopped)));
    drop(finish);

    assert!(stopped(&account.store().connection()));
    assert!(account.connect().is_err());
}

fn start_with(fake: &FakeGateway, server: &wiremock::MockServer) -> Account {
    Account::start(
        crate::gateway::fake::client_with(fake.url(), format!("{}/api/v9/", server.uri())),
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
    )
    .unwrap()
}

const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";

fn page(ids: &[u64]) -> Value {
    page_with(ids, &[])
}

fn page_with(ids: &[u64], edited: &[u64]) -> Value {
    let mut messages: Vec<Value> = ids
        .iter()
        .rev()
        .map(|id| {
            let mut message = message(0, *id)["d"].clone();
            if edited.contains(id) {
                message["content"] = "edited".into();
            }
            message
        })
        .collect();
    for message in &mut messages {
        message.as_object_mut().unwrap().remove("nonce");
    }
    Value::Array(messages)
}

fn ids(account: &Account) -> Vec<u64> {
    account
        .store()
        .messages(Snowflake::new(GENERAL))
        .map(|window| {
            window
                .messages
                .iter()
                .map(|message| message.id.get())
                .collect()
        })
        .unwrap_or_default()
}

fn general() -> ChannelId {
    Snowflake::new(GENERAL)
}

async fn mock_page(server: &wiremock::MockServer, query: (&str, &str), body: Value) {
    use wiremock::matchers::{path, query_param};
    wiremock::Mock::given(path(MESSAGES))
        .and(query_param(query.0, query.1))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn loading_the_latest_messages_fills_the_window() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let _connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    mock_page(&server, ("limit", "3"), page(&[10, 11, 12])).await;

    account
        .load_messages(general(), MessageLoad::Latest { limit: 3 })
        .await
        .unwrap();

    assert_eq!(
        next(&subscription).await.as_deref(),
        Some("MessagesLoaded(10..12)")
    );
    assert_eq!(ids(&account), [10, 11, 12]);
}

#[tokio::test]
async fn limits_are_clamped_to_1_to_100() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let _connection = online(&mut fake, &account).await;
    mock_page(&server, ("limit", "1"), page(&[10])).await;
    mock_page(&server, ("limit", "100"), page(&[])).await;

    account
        .load_messages(general(), MessageLoad::Latest { limit: 0 })
        .await
        .unwrap();
    account
        .load_messages(general(), MessageLoad::Older { limit: 200 })
        .await
        .unwrap();

    let queries: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.query().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(queries, ["limit=1", "limit=100&before=10"]);
}

#[tokio::test]
async fn older_pages_use_the_windows_first_message() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let _connection = online(&mut fake, &account).await;
    mock_page(&server, ("limit", "3"), page(&[10, 11, 12])).await;
    account
        .load_messages(general(), MessageLoad::Latest { limit: 3 })
        .await
        .unwrap();
    server.reset().await;
    mock_page(&server, ("before", "10"), page(&[7, 8, 9])).await;

    account
        .load_messages(general(), MessageLoad::Older { limit: 3 })
        .await
        .unwrap();

    assert_eq!(ids(&account), [7, 8, 9, 10, 11, 12]);
}

#[tokio::test]
async fn a_broken_message_doesnt_end_the_history() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let _connection = online(&mut fake, &account).await;
    mock_page(&server, ("limit", "3"), page(&[10, 11, 12])).await;
    account
        .load_messages(general(), MessageLoad::Latest { limit: 3 })
        .await
        .unwrap();
    server.reset().await;
    let mut broken = page(&[7, 8, 9]);
    broken[1]["author"] = json!({"username": "no id"});
    mock_page(&server, ("before", "10"), broken).await;

    account
        .load_messages(general(), MessageLoad::Older { limit: 3 })
        .await
        .unwrap();

    assert_eq!(ids(&account), [7, 9, 10, 11, 12]);
    assert!(!account.store().messages(general()).unwrap().oldest);
}

#[tokio::test]
async fn a_new_session_refreshes_stale_windows() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    for (seq, id) in [(2, 10), (3, 11), (4, 12)] {
        connection.send(message(seq, id)).await;
    }
    events_until(&subscription, "MessageInserted(12)").await;
    {
        use wiremock::matchers::{path, query_param};
        wiremock::Mock::given(path(MESSAGES))
            .and(query_param("limit", "100"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(page_with(&[10, 12, 13], &[12]))
                    .set_delay(Duration::from_millis(300)),
            )
            .mount(&server)
            .await;
    }

    connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &fake, |_| {})).await;
    events_until(&subscription, "Online").await;
    fresh.send(message(2, 14)).await;
    while account.store().messages(general()).unwrap().stale || !ids(&account).contains(&14) {
        next(&subscription)
            .await
            .expect("the refresh never finished");
    }

    assert_eq!(ids(&account), [10, 12, 13, 14]);
    let edited = account
        .store()
        .message(general(), Snowflake::new(12))
        .unwrap();
    assert_eq!(&*edited.content, "edited");
}

async fn reconnect_with_a_new_session(
    fake: &mut FakeGateway,
    connection: &mut FakeConnection,
) -> FakeConnection {
    connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, fake, |_| {})).await;
    fresh
}

async fn mock_refresh_failures(server: &wiremock::MockServer, status: u16, times: u64) {
    use wiremock::matchers::{path, query_param};
    wiremock::Mock::given(path(MESSAGES))
        .and(query_param("limit", "100"))
        .respond_with(wiremock::ResponseTemplate::new(status))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

fn refresh_requests(requests: &[wiremock::Request]) -> usize {
    requests
        .iter()
        .filter(|request| request.url.query() == Some("limit=100"))
        .count()
}

#[tokio::test]
async fn a_refresh_after_a_reconnect_retries_server_errors() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    connection.send(message(2, 10)).await;
    events_until(&subscription, "MessageInserted(10)").await;
    mock_refresh_failures(&server, 500, 2).await;
    mock_page(&server, ("limit", "100"), page(&[10, 11])).await;

    let _fresh = reconnect_with_a_new_session(&mut fake, &mut connection).await;
    events_until(&subscription, "Online").await;
    while account.store().messages(general()).unwrap().stale {
        next(&subscription)
            .await
            .expect("the refresh never finished");
    }

    assert!(account.store().messages(general()).unwrap().latest);
    assert_eq!(ids(&account), [10, 11]);
    assert_eq!(
        refresh_requests(&server.received_requests().await.unwrap()),
        3
    );
}

#[tokio::test]
async fn a_refresh_that_keeps_failing_detaches_after_its_retries() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    mock_refresh_failures(&server, 500, u64::MAX).await;

    let _fresh = reconnect_with_a_new_session(&mut fake, &mut connection).await;
    events_until(&subscription, "Online").await;
    while account.store().messages(general()).unwrap().latest {
        next(&subscription)
            .await
            .expect("the window never detached");
    }

    assert!(account.store().messages(general()).unwrap().stale);
    assert_eq!(
        refresh_requests(&server.received_requests().await.unwrap()),
        4
    );
}

#[tokio::test]
async fn a_refresh_refused_by_discord_isnt_retried() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    mock_refresh_failures(&server, 403, u64::MAX).await;

    let _fresh = reconnect_with_a_new_session(&mut fake, &mut connection).await;
    events_until(&subscription, "Online").await;
    while account.store().messages(general()).unwrap().latest {
        next(&subscription)
            .await
            .expect("the window never detached");
    }

    assert_eq!(
        refresh_requests(&server.received_requests().await.unwrap()),
        1
    );
}

#[tokio::test]
async fn a_cancelled_load_stops_holding_live_messages() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    mock_page(&server, ("around", "20"), page(&[19, 20, 21])).await;
    {
        use wiremock::matchers::{path, query_param};
        wiremock::Mock::given(path(MESSAGES))
            .and(query_param("after", "21"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(page(&[22, 23, 24]))
                    .set_delay(Duration::from_millis(500)),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    mock_page(&server, ("after", "21"), page(&[22])).await;
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
    let cancelled = timeout(
        Duration::from_millis(100),
        account.load_messages(general(), MessageLoad::Newer { limit: 3 }),
    )
    .await;
    assert!(cancelled.is_err());

    account
        .load_messages(general(), MessageLoad::Newer { limit: 3 })
        .await
        .unwrap();
    connection.send(message(2, 30)).await;

    assert_eq!(
        events_until(&subscription, "MessageInserted(30)")
            .await
            .last()
            .map(String::as_str),
        Some("MessageInserted(30)")
    );
}

#[tokio::test]
async fn messages_created_during_a_catch_up_are_kept() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let mut connection = online(&mut fake, &account).await;
    mock_page(&server, ("around", "20"), page(&[19, 20, 21])).await;
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
    {
        use wiremock::matchers::{path, query_param};
        wiremock::Mock::given(path(MESSAGES))
            .and(query_param("after", "21"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(page(&[22]))
                    .set_delay(Duration::from_millis(300)),
            )
            .mount(&server)
            .await;
    }

    let (loaded, ()) = tokio::join!(
        account.load_messages(general(), MessageLoad::Newer { limit: 50 }),
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            connection.send(message(2, 30)).await;
        }
    );

    loaded.unwrap();
    assert_eq!(ids(&account), [19, 20, 21, 22, 30]);
    assert!(account.store().messages(general()).unwrap().latest);
}

#[tokio::test]
async fn a_failed_load_reports_the_error_and_stops_holding() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    mock_page(&server, ("around", "20"), page(&[19, 20, 21])).await;
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
    {
        use wiremock::matchers::{path, query_param};
        wiremock::Mock::given(path(MESSAGES))
            .and(query_param("after", "21"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let err = account
        .load_messages(general(), MessageLoad::Newer { limit: 50 })
        .await
        .unwrap_err();
    connection.send(message(2, 30)).await;
    connection.send(message(3, 31)).await;
    connection
        .send(rename(4, "dispatches before this one are applied"))
        .await;
    events_until(&subscription, &format!("ChannelUpdated({GENERAL})")).await;
    mock_page(&server, ("after", "21"), page(&[22])).await;
    account
        .load_messages(general(), MessageLoad::Newer { limit: 50 })
        .await
        .unwrap();

    assert!(
        matches!(err, RequestError::ServerError { status: 500 }),
        "{err:?}"
    );
    assert_eq!(ids(&account), [19, 20, 21, 22]);
}

#[tokio::test]
async fn a_401_closes_the_account_like_a_rejected_token() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    {
        use wiremock::matchers::path;
        wiremock::Mock::given(path(MESSAGES))
            .respond_with(wiremock::ResponseTemplate::new(401))
            .mount(&server)
            .await;
    }

    let err = account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_err();

    assert!(matches!(err, RequestError::Unauthorized), "{err:?}");
    assert_eq!(connection.client_close_code().await, Some(1000));
    let rest: Vec<String> = {
        let mut events = Vec::new();
        while let Some(event) = next(&subscription).await {
            events.push(event);
        }
        events
    };
    assert_eq!(
        rest.last().map(String::as_str),
        Some("Closed(AuthenticationFailed)")
    );
    assert!(matches!(
        account.store().connection(),
        ConnectionState::Closed { error: Some(ref error) } if matches!(**error, GatewayError::AuthenticationFailed)
    ));
    let again = account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_err();
    assert!(
        matches!(again, RequestError::Unauthorized | RequestError::Closed),
        "{again:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

const ME: &str = "100000000000000001";

// The n-th send answers with `statuses[n]`, the last repeating. Created IDs depend only on the
// nonce, because Discord sends a nonce's first message only.
#[derive(Clone)]
struct Sends {
    bodies: Arc<std::sync::Mutex<Vec<Value>>>,
    statuses: Arc<Vec<u16>>,
    delay: Duration,
}

fn created(id: u64, body: &Value) -> Value {
    let mut message = message(0, id)["d"].clone();
    message["content"] = body["content"].clone();
    message["nonce"] = body["nonce"].clone();
    message["author"] = json!({"id": ME, "username": "akari_tester"});
    message
}

impl Sends {
    fn new(statuses: &[u16]) -> Self {
        Self {
            bodies: Arc::default(),
            statuses: Arc::new(statuses.to_vec()),
            delay: Duration::ZERO,
        }
    }

    fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    async fn mount(&self, server: &wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        let sends = self.clone();
        wiremock::Mock::given(method("POST"))
            .and(path(MESSAGES))
            .respond_with(move |request: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let mut bodies = sends.bodies.lock().unwrap();
                bodies.push(body.clone());
                let call = bodies.len() - 1;
                let mut nonces: Vec<&Value> = Vec::new();
                for sent in bodies.iter() {
                    if !nonces.contains(&&sent["nonce"]) {
                        nonces.push(&sent["nonce"]);
                    }
                }
                let nth = nonces
                    .iter()
                    .position(|nonce| **nonce == body["nonce"])
                    .unwrap_or(0) as u64;
                let status = sends.statuses[call.min(sends.statuses.len() - 1)];
                let template = wiremock::ResponseTemplate::new(status).set_delay(sends.delay);
                match status {
                    200 => template.set_body_json(created(400_000_000_000_000_050 + nth, &body)),
                    403 => template
                        .set_body_json(json!({"message": "Missing Permissions", "code": 50013})),
                    429 => template.set_body_json(json!({
                        "message": "This action cannot be performed due to slowmode rate limit.",
                        "code": 20016,
                        "retry_after": 25.0,
                        "global": false
                    })),
                    400 => template.set_body_json(json!({
                        "captcha_key": ["captcha-required"],
                        "captcha_service": "hcaptcha",
                        "captcha_sitekey": "site-key"
                    })),
                    _ => template,
                }
            })
            .mount(server)
            .await;
    }

    fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().unwrap().clone()
    }

    async fn first_nonce(&self) -> Value {
        timeout(WAIT, async {
            loop {
                if let Some(body) = self.bodies.lock().unwrap().first() {
                    return body["nonce"].clone();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("no send arrived")
    }
}

fn echo(seq: u64, id: u64, nonce: &Value) -> Value {
    let body = json!({"content": "hello", "nonce": nonce});
    json!({"op": 0, "s": seq, "t": "MESSAGE_CREATE", "d": created(id, &body)})
}

fn outbox(account: &Account) -> Vec<(u64, Delivery)> {
    account
        .store()
        .messages(general())
        .map(|window| {
            window
                .pending
                .iter()
                .map(|message| (message.id.get(), message.delivery))
                .collect()
        })
        .unwrap_or_default()
}

async fn sending(
    fake: &mut FakeGateway,
    server: &wiremock::MockServer,
) -> (Account, Subscription, FakeConnection) {
    let account = start_with(fake, server);
    let subscription = account.store().subscribe();
    let connection = online(fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    (account, subscription, connection)
}

async fn sync(
    connection: &mut FakeConnection,
    subscription: &Subscription,
    seq: u64,
) -> Vec<String> {
    connection.send(rename(seq, &format!("sync {seq}"))).await;
    let mut events = events_until(subscription, &format!("ChannelUpdated({GENERAL})")).await;
    events.pop();
    events
}

#[tokio::test]
async fn the_echo_first_replaces_the_pending_message() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(300));
    sends.mount(&server).await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;

    let (sent, ()) = tokio::join!(account.send_message(general(), "hello".to_owned()), async {
        let nonce = sends.first_nonce().await;
        connection
            .send(echo(2, 400_000_000_000_000_050, &nonce))
            .await;
    });

    assert_eq!(sent.unwrap().get(), 400_000_000_000_000_050);
    let events = sync(&mut connection, &subscription, 3).await;
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events[0].starts_with("MessageInserted("), "{events:?}");
    assert_eq!(events[1], "MessageReplaced(400000000000000050)");
    assert_eq!(ids(&account), [400_000_000_000_000_050]);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_late_echo_after_the_response_changes_nothing() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;

    account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap();
    let nonce = sends.first_nonce().await;
    connection
        .send(echo(2, 400_000_000_000_000_050, &nonce))
        .await;
    connection
        .send(echo(3, 400_000_000_000_000_050, &nonce))
        .await;

    let events = sync(&mut connection, &subscription, 4).await;
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[1], "MessageReplaced(400000000000000050)");
    assert_eq!(ids(&account), [400_000_000_000_000_050]);
}

#[tokio::test]
async fn an_echo_after_a_failed_send_still_shows_the_message() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[403]);
    sends.mount(&server).await;
    let (account, _subscription, mut connection) = sending(&mut fake, &server).await;

    let err = account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap_err();
    let failed = outbox(&account);
    let nonce = sends.first_nonce().await;
    connection
        .send(echo(2, 400_000_000_000_000_050, &nonce))
        .await;
    connection.send(rename(3, "sync")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        matches!(err, RequestError::Discord { code: 50013, .. }),
        "{err:?}"
    );
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, Delivery::Failed);
    assert_eq!(ids(&account), [400_000_000_000_000_050]);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_send_that_discord_took_but_answered_with_502_appears_once() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[502, 200]);
    sends.mount(&server).await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;

    let (sent, ()) = tokio::join!(account.send_message(general(), "hello".to_owned()), async {
        let nonce = sends.first_nonce().await;
        connection
            .send(echo(2, 400_000_000_000_000_050, &nonce))
            .await;
    });

    sent.unwrap();
    let bodies = sends.bodies();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    let events = sync(&mut connection, &subscription, 3).await;
    let shown: Vec<_> = events
        .iter()
        .filter(|event| event.contains("400000000000000050"))
        .collect();
    assert_eq!(shown, ["MessageReplaced(400000000000000050)"], "{events:?}");
    assert_eq!(ids(&account), [400_000_000_000_000_050]);
}

#[tokio::test]
async fn a_failed_message_stays_until_retried_or_discarded() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[403]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap_err();
    let failed = outbox(&account);
    account.discard_message(general(), Snowflake::new(failed[0].0));

    assert_eq!(failed[0].1, Delivery::Failed);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_retry_reuses_the_nonce() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[403, 200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap_err();
    let pending = Snowflake::new(outbox(&account)[0].0);
    let sent = account.retry_message(general(), pending).await.unwrap();

    let bodies = sends.bodies();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["nonce"], bodies[1]["nonce"]);
    assert_eq!(bodies[0]["nonce"], pending.get().to_string());
    assert_eq!(ids(&account), [sent.get()]);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn sends_in_one_channel_keep_their_order() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(30));
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let (one, two, three) = tokio::join!(
        account.send_message(general(), "one".to_owned()),
        account.send_message(general(), "two".to_owned()),
        account.send_message(general(), "three".to_owned()),
    );

    assert!(one.is_ok() && two.is_ok() && three.is_ok());
    let bodies = sends.bodies();
    let contents: Vec<_> = bodies.iter().map(|body| body["content"].clone()).collect();
    assert_eq!(contents, ["one", "two", "three"]);
    let nonces: Vec<u64> = bodies
        .iter()
        .map(|body| body["nonce"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(
        nonces.windows(2).all(|pair| pair[0] < pair[1]),
        "{nonces:?}"
    );
}

#[tokio::test]
async fn a_retried_send_keeps_its_place_before_later_sends() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[502, 200]).delayed(Duration::from_millis(30));
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let (one, two, three) = tokio::join!(
        account.send_message(general(), "one".to_owned()),
        account.send_message(general(), "two".to_owned()),
        account.send_message(general(), "three".to_owned()),
    );

    assert!(one.is_ok() && two.is_ok() && three.is_ok());
    let contents: Vec<_> = sends
        .bodies()
        .iter()
        .map(|body| body["content"].clone())
        .collect();
    assert_eq!(contents, ["one", "one", "two", "three"]);
}

#[cfg(feature = "repeat-nonce")]
#[tokio::test]
async fn a_repeated_send_posts_the_same_body_twice() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let (first, repeat) = account
        .send_message_twice(general(), "hello".to_owned())
        .await
        .unwrap();

    let bodies = sends.bodies();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(repeat.unwrap(), first);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_cancelled_send_leaves_the_message_failed() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(500));
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let cancelled = tokio::time::timeout(
        Duration::from_millis(100),
        account.send_message(general(), "hello".to_owned()),
    )
    .await;

    assert!(cancelled.is_err());
    let deliveries: Vec<Delivery> = outbox(&account)
        .into_iter()
        .map(|(_, delivery)| delivery)
        .collect();
    assert_eq!(deliveries, [Delivery::Failed]);
}

#[tokio::test]
async fn a_send_shows_at_once_and_goes_out_even_if_the_jump_fails() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(300));
    sends.mount(&server).await;
    mock_page(&server, ("around", "20"), page(&[19, 20, 21])).await;
    {
        use wiremock::matchers::{path, query_param};
        wiremock::Mock::given(path(MESSAGES))
            .and(query_param("limit", "50"))
            .respond_with(
                wiremock::ResponseTemplate::new(500).set_delay(Duration::from_millis(300)),
            )
            .mount(&server)
            .await;
    }
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;
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

    let (sent, shown) = tokio::join!(account.send_message(general(), "hello".to_owned()), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        outbox(&account)
    });

    assert!(sent.is_ok(), "{sent:?}");
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(sends.bodies().len(), 1);
}

#[tokio::test]
async fn sending_in_a_detached_window_jumps_to_the_present() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    mock_page(&server, ("around", "20"), page(&[19, 20, 21])).await;
    mock_page(&server, ("limit", "50"), page(&[30])).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;
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

    account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap();

    let mut requests: Vec<(String, String)> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            (
                request.method.to_string(),
                request.url.query().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    requests.sort();
    assert_eq!(
        requests,
        [
            ("GET".to_owned(), "limit=3&around=20".to_owned()),
            ("GET".to_owned(), "limit=50".to_owned()),
            ("POST".to_owned(), String::new()),
        ]
    );
    assert_eq!(ids(&account), [30, 400_000_000_000_000_050]);
}

#[tokio::test]
async fn empty_messages_are_refused_without_a_request() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let err = account
        .send_message(general(), " \n\t ".to_owned())
        .await
        .unwrap_err();

    assert!(matches!(err, RequestError::InvalidRequest), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_captcha_leaves_the_message_failed_with_the_challenge() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[400]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let err = account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap_err();

    let RequestError::CaptchaRequired(challenge) = err else {
        panic!("expected a captcha, got {err:?}");
    };
    assert_eq!(challenge.sitekey.as_deref(), Some("site-key"));
    let failed = outbox(&account);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, Delivery::Failed);
}

#[test]
fn nonces_are_unique_and_increasing() {
    let nonces = Nonces::default();
    let now = 1_700_000_000_000;

    let mut seen: Vec<u64> = (0..1000).map(|_| nonces.next(now).get()).collect();
    seen.push(nonces.next(now - 5).get());
    seen.push(nonces.next(now + 1).get());

    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(seen[0], MessageId::from_unix_millis(now, 0).get());
}

async fn next_command(connection: &mut FakeConnection) -> Option<Value> {
    let payload = timeout(Duration::from_millis(500), connection.recv())
        .await
        .ok()??;
    Some(payload)
}

#[tokio::test]
async fn set_status_sends_now_when_online() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account
        .set_status(PresenceStatus::DoNotDisturb)
        .await
        .unwrap();

    let command = next_command(&mut connection)
        .await
        .expect("no presence update");
    assert_eq!(command["op"], 3);
    assert_eq!(command["d"]["status"], "dnd");
}

#[tokio::test]
async fn set_status_while_offline_waits_for_the_next_session() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);

    account.set_status(PresenceStatus::Idle).await.unwrap();
    let mut connection = online(&mut fake, &account).await;

    let command = next_command(&mut connection)
        .await
        .expect("no presence update");
    assert_eq!(command["op"], 3);
    assert_eq!(command["d"]["status"], "idle");
}

#[tokio::test]
async fn the_status_is_sent_again_after_a_new_session_but_not_after_a_resume() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.set_status(PresenceStatus::Invisible).await.unwrap();
    assert_eq!(next_command(&mut connection).await.unwrap()["op"], 3);

    connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &fake, |_| {})).await;
    let again = next_command(&mut fresh)
        .await
        .expect("no presence update after READY");
    fresh.close(4000).await;
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;

    assert_eq!(again["d"]["status"], "invisible");
    assert!(
        next_command(&mut resumed).await.is_none(),
        "a resume sent the status again"
    );
}

#[tokio::test]
async fn gateway_commands_reach_the_gateway() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account
        .shared
        .send(GatewayCommand::UpdatePresence {
            status: PresenceStatus::Online,
        })
        .await
        .unwrap();

    assert_eq!(
        next_command(&mut connection).await.unwrap()["d"]["status"],
        "online"
    );
}

const G1: &str = "200000000000000001";

fn subscribed(command: &Value) -> Vec<String> {
    assert_eq!(command["op"], 37, "{command}");
    let mut guilds: Vec<String> = command["d"]["subscriptions"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    guilds.sort();
    guilds
}

#[tokio::test]
async fn viewing_a_channel_subscribes_its_guild_once_per_session() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.view_channel(general());
    let first = next_command(&mut connection)
        .await
        .expect("no subscription");
    account.view_channel(Snowflake::new(300_000_000_000_000_003));
    account.view_channel(Snowflake::new(300_000_000_000_000_010));

    assert_eq!(subscribed(&first), [G1]);
    assert_eq!(
        first["d"]["subscriptions"][G1],
        json!({"typing": true, "activities": true, "threads": true})
    );
    assert!(
        next_command(&mut connection).await.is_none(),
        "subscribed twice"
    );
}

#[tokio::test]
async fn loading_messages_subscribes_the_guild_too() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    mock_page(&server, ("limit", "50"), page(&[10])).await;

    account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();

    assert_eq!(
        subscribed(
            &next_command(&mut connection)
                .await
                .expect("no subscription")
        ),
        [G1]
    );
}

#[tokio::test]
async fn subscriptions_are_sent_again_after_ready_and_resumed() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    assert_eq!(
        subscribed(&next_command(&mut connection).await.unwrap()),
        [G1]
    );

    connection.close(4000).await;
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;
    let after_resume = next_command(&mut resumed)
        .await
        .expect("nothing after RESUMED");
    resumed.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &fake, |_| {})).await;
    let after_ready = next_command(&mut fresh).await.expect("nothing after READY");

    assert_eq!(subscribed(&after_resume), [G1]);
    assert_eq!(subscribed(&after_ready), [G1]);
}

#[tokio::test]
async fn a_channel_viewed_while_resuming_is_subscribed_after_resumed() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    connection.close(4000).await;
    events_until(&subscription, "Connecting").await;
    account.view_channel(general());
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;

    assert_eq!(
        subscribed(
            &next_command(&mut resumed)
                .await
                .expect("nothing after RESUMED")
        ),
        [G1]
    );
}

const THREAD: u64 = 300_000_000_000_000_020;
const VOICE: u64 = 300_000_000_000_000_003;

fn large(ready: &mut Value) {
    ready["guilds"][0]["large"] = true.into();
}

async fn online_in_a_large_guild(fake: &mut FakeGateway, account: &Account) -> FakeConnection {
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    assert_eq!(connection.handshake(60_000).await["op"], 2);
    connection.send(ready_payload(1, fake, large)).await;
    connection
}

fn member_lists(command: &Value) -> (Value, Value) {
    let entry = &command["d"]["subscriptions"][G1];
    (
        entry["channels"].clone(),
        entry["thread_member_lists"].clone(),
    )
}

#[tokio::test]
async fn viewing_a_channel_in_a_large_guild_subscribes_its_member_list() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online_in_a_large_guild(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.view_channel(general());
    let command = next_command(&mut connection)
        .await
        .expect("no subscription");

    assert_eq!(
        command["d"]["subscriptions"][G1],
        json!({
            "typing": true, "activities": true, "threads": true,
            "channels": {GENERAL.to_string(): [[0, 99]]},
            "thread_member_lists": []
        })
    );
}

#[cfg(feature = "flags-only")]
#[tokio::test]
async fn flags_only_subscriptions_leave_out_member_lists() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    account.subscribe_flags_only();
    let subscription = account.store().subscribe();
    let mut connection = online_in_a_large_guild(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.view_channel(general());
    let command = next_command(&mut connection)
        .await
        .expect("no subscription");

    assert_eq!(
        command["d"]["subscriptions"][G1],
        json!({"typing": true, "activities": true, "threads": true})
    );
}

#[tokio::test]
async fn every_viewed_channel_of_a_large_guild_keeps_its_member_list() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online_in_a_large_guild(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    next_command(&mut connection)
        .await
        .expect("no subscription");

    account.view_channel(Snowflake::new(THREAD));
    let with_thread = next_command(&mut connection).await.expect("no update");
    account.view_channel(Snowflake::new(VOICE));
    let with_voice = next_command(&mut connection).await.expect("no update");
    account.view_channel(general());

    assert_eq!(
        member_lists(&with_thread),
        (
            json!({GENERAL.to_string(): [[0, 99]]}),
            json!([THREAD.to_string()])
        )
    );
    assert_eq!(
        member_lists(&with_voice),
        (
            json!({GENERAL.to_string(): [[0, 99]], VOICE.to_string(): [[0, 99]]}),
            json!([THREAD.to_string()])
        )
    );
    assert!(
        next_command(&mut connection).await.is_none(),
        "subscribed again without a change"
    );
}

#[tokio::test]
async fn an_evicted_channel_drops_its_member_list() {
    let mut fake = FakeGateway::start().await;
    let account = Account::start(
        client(&fake),
        Token::new(TOKEN.to_owned()),
        timing(),
        WindowLimits {
            channels: 1,
            messages: 200,
        },
    )
    .unwrap();
    let subscription = account.store().subscribe();
    let mut connection = online_in_a_large_guild(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    next_command(&mut connection)
        .await
        .expect("no subscription");

    account.view_channel(Snowflake::new(300_000_000_000_000_010));
    let dropped = next_command(&mut connection)
        .await
        .expect("the member list stayed");

    assert_eq!(member_lists(&dropped), (json!({}), json!([])));
}

#[tokio::test]
async fn member_lists_are_sent_again_after_ready_and_resumed() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online_in_a_large_guild(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    next_command(&mut connection)
        .await
        .expect("no subscription");

    connection.close(4000).await;
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;
    let after_resume = next_command(&mut resumed)
        .await
        .expect("nothing after RESUMED");
    resumed.send(json!({"op": 9, "d": false})).await;
    let mut fresh = fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &fake, large)).await;
    let after_ready = next_command(&mut fresh).await.expect("nothing after READY");

    let general_list = json!({GENERAL.to_string(): [[0, 99]]});
    assert_eq!(member_lists(&after_resume).0, general_list);
    assert_eq!(member_lists(&after_ready).0, general_list);
}

#[tokio::test]
async fn a_late_resend_after_ready_doesnt_repeat_a_subscription() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    let first = next_command(&mut connection)
        .await
        .expect("no subscription");

    account.shared.resubscribe().await;

    assert_eq!(subscribed(&first), [G1]);
    assert!(
        next_command(&mut connection).await.is_none(),
        "op 37 sent twice for one change"
    );
}

#[tokio::test]
async fn nothing_is_subscribed_while_offline() {
    let mut fake = FakeGateway::start().await;
    let account = start(&fake);

    account.view_channel(general());
    let mut connection = online(&mut fake, &account).await;

    assert_eq!(
        subscribed(
            &next_command(&mut connection)
                .await
                .expect("no subscription after READY")
        ),
        [G1]
    );
}

// A plain member of guild 1 (the fixture's user owns it and is an administrator) in a GENERAL
// with a 30 s slowmode; `roles` are extra guild roles the member holds.
async fn slowmode_sending(
    fake: &mut FakeGateway,
    server: &wiremock::MockServer,
    roles: Value,
) -> (Account, Subscription, FakeConnection) {
    let account = start_with(fake, server);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    assert_eq!(connection.handshake(60_000).await["op"], 2);
    connection
        .send(ready_payload(1, fake, |data| {
            let guild = &mut data["guilds"][0];
            guild["properties"]["owner_id"] = "100000000000000099".into();
            guild["channels"][1]["rate_limit_per_user"] = 30.into();
            let mut held = Vec::new();
            for role in roles.as_array().cloned().unwrap_or_default() {
                held.push(role["id"].clone());
                guild["roles"].as_array_mut().unwrap().push(role);
            }
            data["merged_members"][0][0]["roles"] = Value::Array(held);
        }))
        .await;
    events_until(&subscription, "Online").await;
    account.view_channel(general());
    (account, subscription, connection)
}

fn slowmode_until(account: &Account) -> Option<std::time::SystemTime> {
    account.store().slowmode(general()).unwrap().until
}

#[tokio::test]
async fn a_message_over_the_limit_is_refused_before_anything_is_queued() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let err = account
        .send_message(general(), "a".repeat(2001))
        .await
        .unwrap_err();

    assert!(
        matches!(err, RequestError::TooLong { limit: 2000 }),
        "{err:?}"
    );
    assert!(outbox(&account).is_empty());
    assert!(sends.bodies().is_empty());
    // 2,000 code points, 2,001 UTF-16 units.
    let fits = format!("{}👍", "a".repeat(1999));
    account.send_message(general(), fits).await.unwrap();
    assert_eq!(sends.bodies().len(), 1);
}

#[tokio::test]
async fn a_send_starts_the_cooldown_when_it_is_queued() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(300));
    sends.mount(&server).await;
    let (account, _subscription, _connection) =
        slowmode_sending(&mut fake, &server, json!([])).await;
    assert_eq!(slowmode_until(&account), None);

    let before = std::time::SystemTime::now();
    let (sent, pending) = tokio::join!(account.send_message(general(), "hi".to_owned()), async {
        sends.first_nonce().await;
        slowmode_until(&account)
    });

    sent.unwrap();
    let pending = pending.expect("no cooldown while the send was pending");
    assert!(pending > before + Duration::from_secs(29), "{pending:?}");
    assert!(pending <= std::time::SystemTime::now() + Duration::from_secs(30));
    let confirmed = slowmode_until(&account).expect("the cooldown ended with the confirmation");
    let moved = confirmed
        .duration_since(pending)
        .unwrap_or_else(|err| err.duration());
    assert!(moved < Duration::from_millis(50), "restarted by {moved:?}");
}

#[tokio::test]
async fn a_failed_send_clears_the_cooldown_it_started() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    Sends::new(&[403]).mount(&server).await;
    let (account, _subscription, _connection) =
        slowmode_sending(&mut fake, &server, json!([])).await;

    let err = account
        .send_message(general(), "hi".to_owned())
        .await
        .unwrap_err();

    assert!(
        matches!(err, RequestError::Discord { code: 50013, .. }),
        "{err:?}"
    );
    assert_eq!(slowmode_until(&account), None);
}

#[tokio::test]
async fn a_slowmode_answer_holds_until_retry_after() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    Sends::new(&[429]).mount(&server).await;
    let (account, _subscription, _connection) =
        slowmode_sending(&mut fake, &server, json!([])).await;

    let before = std::time::SystemTime::now();
    let err = timeout(WAIT, account.send_message(general(), "hi".to_owned()))
        .await
        .expect("a long slowmode wait must fail at once")
        .unwrap_err();

    let RequestError::RateLimited {
        retry_after: Some(wait),
    } = err
    else {
        panic!("{err:?}");
    };
    assert!(wait > Duration::from_secs(24), "{wait:?}");
    let until = slowmode_until(&account).expect("no cooldown after the slowmode answer");
    assert!(until > before + Duration::from_secs(24), "{until:?}");
}

#[tokio::test]
async fn exempt_users_get_no_cooldown() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    Sends::new(&[200]).mount(&server).await;
    let permissions = (crate::model::Permissions::VIEW_CHANNEL.0
        | crate::model::Permissions::SEND_MESSAGES.0
        | crate::model::Permissions::BYPASS_SLOWMODE.0)
        .to_string();
    let bypass =
        json!([{"id": "600", "name": "bypass", "permissions": permissions, "position": 2}]);
    let (account, _subscription, _connection) = slowmode_sending(&mut fake, &server, bypass).await;

    account
        .send_message(general(), "hi".to_owned())
        .await
        .unwrap();

    let slowmode = account.store().slowmode(general()).unwrap();
    assert!(slowmode.exempt);
    assert_eq!(slowmode.until, None);
}

#[tokio::test]
async fn discords_length_refusal_is_too_long_with_its_limit() {
    use wiremock::matchers::{method, path};
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let refusal = |field: &str, message: &str| {
        let mut errors = serde_json::Map::new();
        errors.insert(
            field.to_owned(),
            json!({"_errors": [{"code": "BASE_TYPE_MAX_LENGTH", "message": message}]}),
        );
        json!({"code": 50035, "message": "Invalid Form Body", "errors": errors})
    };
    for body in [
        refusal("content", "Must be 1990 or fewer in length."),
        refusal("nonce", "Must be 25 or fewer in length."),
        refusal("content", "Too long."),
    ] {
        wiremock::Mock::given(method("POST"))
            .and(path(MESSAGES))
            .respond_with(wiremock::ResponseTemplate::new(400).set_body_json(body))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let mut errors = Vec::new();
    for _ in 0..3 {
        errors.push(
            account
                .send_message(general(), "hello".to_owned())
                .await
                .unwrap_err(),
        );
    }

    assert!(
        matches!(errors[0], RequestError::TooLong { limit: 1990 }),
        "{errors:?}"
    );
    assert!(
        matches!(errors[1], RequestError::Discord { code: 50035, .. }),
        "{errors:?}"
    );
    assert!(
        matches!(errors[2], RequestError::TooLong { limit: 2000 }),
        "{errors:?}"
    );
    assert!(
        outbox(&account)
            .iter()
            .all(|(_, delivery)| *delivery == Delivery::Failed)
    );
}

#[tokio::test]
async fn a_queued_message_waits_for_delivery_then_sends() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    let pending = account.queue_message(general(), "hello").unwrap();

    assert_eq!(outbox(&account), [(pending.get(), Delivery::Pending)]);
    assert!(sends.bodies().is_empty());
    let sent = account.deliver_message(general(), pending).await.unwrap();
    assert_eq!(sent.get(), 400_000_000_000_000_050);
    assert_eq!(sends.bodies()[0]["content"], "hello");
    assert!(outbox(&account).is_empty());
    assert!(matches!(
        account.deliver_message(general(), pending).await,
        Err(RequestError::InvalidRequest)
    ));
}

#[tokio::test]
async fn a_pending_message_is_delivered_once() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]).delayed(Duration::from_millis(100));
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;
    let pending = account.queue_message(general(), "hello").unwrap();

    let (first, second) = tokio::join!(
        account.deliver_message(general(), pending),
        account.deliver_message(general(), pending),
    );

    assert_eq!(sends.bodies().len(), 1);
    assert_eq!(
        [&first, &second].iter().filter(|sent| sent.is_ok()).count(),
        1
    );
    assert!(
        [first, second]
            .iter()
            .any(|sent| matches!(sent, Err(RequestError::InvalidRequest)))
    );
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_message_being_retried_isnt_delivered_again() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[403, 200]).delayed(Duration::from_millis(100));
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;
    account
        .send_message(general(), "hello".to_owned())
        .await
        .unwrap_err();
    let pending = Snowflake::new(outbox(&account)[0].0);

    let (retried, delivered) = tokio::join!(
        account.retry_message(general(), pending),
        account.deliver_message(general(), pending),
    );

    assert!(retried.is_ok());
    assert!(matches!(delivered, Err(RequestError::InvalidRequest)));
    assert_eq!(sends.bodies().len(), 2);
    assert!(outbox(&account).is_empty());
}

#[tokio::test]
async fn a_closed_account_refuses_before_anything_is_queued() {
    let mut fake = FakeGateway::start().await;
    let server = wiremock::MockServer::start().await;
    let sends = Sends::new(&[200]);
    sends.mount(&server).await;
    let (account, _subscription, _connection) = sending(&mut fake, &server).await;

    account.close();

    assert!(matches!(
        account.queue_message(general(), "hello"),
        Err(RequestError::Closed)
    ));
    assert!(outbox(&account).is_empty());
    assert!(sends.bodies().is_empty());
}
