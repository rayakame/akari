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
        StoreEvent::MessageUpdated(message) => format!("MessageUpdated({})", message.id.get()),
        StoreEvent::MessageDeleted { message_id, .. } => {
            format!("MessageDeleted({})", message_id.get())
        }
        StoreEvent::MessagesLoaded { first, last, .. } => {
            format!("MessagesLoaded({}..{})", first.get(), last.get())
        }
        StoreEvent::MessagesCleared { .. } => "MessagesCleared".to_owned(),
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

    assert_eq!(
        events_until(&subscription, "MessagesLoaded(14..14)").await,
        [
            "MessageDeleted(11)",
            "MessageUpdated(12)",
            "MessagesLoaded(13..13)",
            "MessagesLoaded(14..14)"
        ]
    );
    assert_eq!(ids(&account), [10, 12, 13, 14]);
    assert!(!account.store().messages(general()).unwrap().stale);
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

    assert!(matches!(err, RequestError::UnexpectedResponse), "{err:?}");
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
