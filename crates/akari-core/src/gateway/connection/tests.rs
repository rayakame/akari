use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use tokio::time::timeout;

use super::fake::{FakeConnection, FakeGateway, WAIT};
use super::*;
use crate::gateway::{GatewayCommand, PresenceStatus};
use crate::model::{Snowflake, UserMarker};
use crate::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
use crate::{Endpoints, TokenStore, TokenStoreError};

const TOKEN: &str = "gateway-test-token.secret";
const SESSION: &str = "session-1";

struct NoStore;

impl TokenStore for NoStore {
    fn load(&self, _: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
        Ok(None)
    }
    fn save(&self, _: Snowflake<UserMarker>, _: &Token) -> Result<(), TokenStoreError> {
        Ok(())
    }
    fn delete(&self, _: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
        Ok(())
    }
}

fn client_for(gateway: String) -> DiscordClient {
    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "en-US".to_owned(),
    };
    let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
    let endpoints = Endpoints {
        gateway,
        allow_plaintext: true,
        ..Endpoints::default()
    };
    DiscordClient::with_endpoints(properties, Arc::new(NoStore), endpoints).unwrap()
}

fn client(fake: &FakeGateway) -> DiscordClient {
    client_for(fake.url())
}

fn timing() -> Timing {
    Timing {
        hello_timeout: Duration::from_secs(2),
        close_timeout: Duration::from_millis(200),
        retry_base: Duration::from_millis(10),
        retry_max: Duration::from_millis(100),
        invalid_session_min: Duration::from_millis(10),
        invalid_session_max: Duration::from_millis(20),
        ..Timing::default()
    }
}

fn start(fake: &FakeGateway) -> Gateway {
    let gateway = Gateway::start(client(fake), Token::new(TOKEN.to_owned()), timing()).unwrap();
    gateway.connect().unwrap();
    gateway
}

async fn next(gateway: &Gateway) -> ConnectionEvent {
    timeout(WAIT, gateway.next())
        .await
        .expect("no event")
        .expect("the gateway ended")
}

async fn next_error(gateway: &Gateway) -> GatewayError {
    match timeout(WAIT, gateway.next()).await.expect("no event") {
        Err(err) => err,
        Ok(event) => panic!("expected an error, got {event:?}"),
    }
}

// Identifies with `interval` ms heartbeats and delivers READY with seq 1.
async fn connected(fake: &mut FakeGateway, gateway: &Gateway, interval: u64) -> FakeConnection {
    let mut connection = fake.accept().await;
    let identify = connection.handshake(interval).await;
    assert_eq!(identify["op"], 2);
    connection.ready(1, SESSION, &fake.resume_url()).await;
    assert!(matches!(
        next(gateway).await,
        ConnectionEvent::Dispatch(DispatchEvent::Ready(_))
    ));
    connection
}

fn last_heartbeat(connection: &FakeConnection) -> Option<&serde_json::Value> {
    connection.heartbeats.last().map(|(_, seq)| seq)
}

#[tokio::test]
async fn identify_presents_the_same_client_as_rest() {
    let mut fake = FakeGateway::start().await;
    let client = client(&fake);
    let gateway = Gateway::start(client.clone(), Token::new(TOKEN.to_owned()), timing()).unwrap();
    gateway.connect().unwrap();

    let mut connection = fake.accept().await;

    assert_eq!(connection.uri, "/?v=9&encoding=json&compress=zstd-stream");
    assert_eq!(
        connection.header("user-agent"),
        Some(client.properties().browser_user_agent.as_str())
    );
    assert_eq!(connection.header("origin"), Some("https://discord.com"));
    let identify = connection.handshake(60_000).await;
    assert_eq!(identify["op"], 2);
    assert_eq!(identify["d"]["token"], TOKEN);
    assert_eq!(
        identify["d"]["properties"],
        serde_json::to_value(client.properties()).unwrap()
    );
    assert_eq!(identify["d"]["capabilities"], 1597);
}

#[tokio::test]
async fn heartbeats_carry_the_last_sequence() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 50).await;

    connection.dispatch(2, "TYPING_START").await;

    connection
        .pump_until(WAIT, |connection, _| {
            last_heartbeat(connection) == Some(&json!(2))
        })
        .await;
}

#[tokio::test]
async fn an_unread_consumer_never_stalls_the_connection() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = fake.accept().await;
    connection.handshake(50).await;
    connection.ready(1, SESSION, &fake.resume_url()).await;
    for seq in 2..=501 {
        connection.dispatch(seq, "TYPING_START").await;
    }

    connection
        .pump_until(WAIT, |connection, _| {
            last_heartbeat(connection) == Some(&json!(501))
        })
        .await;
    let before = connection.heartbeats.len();
    let unexpected = connection.pump(Duration::from_millis(500)).await;

    assert!(unexpected.is_empty(), "{unexpected:?}");
    assert_eq!(connection.close_code, None);
    assert!(connection.heartbeats.len() >= before + 3);
    assert_eq!(gateway.buffered.load(Ordering::Relaxed), 501);
    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Dispatch(DispatchEvent::Ready(_))
    ));
    for _ in 2..=501 {
        assert!(matches!(
            next(&gateway).await,
            ConnectionEvent::Dispatch(DispatchEvent::Other(name)) if name == "TYPING_START"
        ));
    }
    assert_eq!(gateway.buffered.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn heartbeat_requests_are_answered_at_once() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 600_000).await;
    let before = connection.heartbeats.len();

    connection.send(json!({"op": 1, "d": null})).await;

    connection
        .pump_until(WAIT, |connection, _| connection.heartbeats.len() > before)
        .await;
}

#[tokio::test]
async fn close_ends_the_session() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    gateway.close();

    assert_eq!(connection.client_close_code().await, Some(1000));
    assert!(matches!(next_error(&gateway).await, GatewayError::Closed));
    assert!(matches!(gateway.connect(), Err(GatewayError::Closed)));
    assert!(fake.quiet_for(Duration::from_millis(200)).await);
}

#[tokio::test]
async fn dropping_the_gateway_ends_the_session() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    drop(gateway);

    assert_eq!(connection.client_close_code().await, Some(1000));
    assert!(fake.quiet_for(Duration::from_millis(200)).await);
}

#[tokio::test]
async fn text_frames_decode_like_compressed_ones() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = fake.accept().await;
    connection.send_text_frames();

    connection.handshake(60_000).await;
    connection.ready(1, SESSION, &fake.resume_url()).await;

    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Dispatch(DispatchEvent::Ready(_))
    ));
}

#[tokio::test]
async fn disconnect_keeps_the_session_for_the_next_connect() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;
    connection.dispatch(7, "TYPING_START").await;
    next(&gateway).await;

    gateway.disconnect();

    assert_eq!(connection.client_close_code().await, Some(4000));
    assert!(fake.quiet_for(Duration::from_millis(200)).await);
    gateway.connect().unwrap();
    let mut resumed = fake.accept().await;
    assert!(resumed.uri.starts_with("/resume?"), "{}", resumed.uri);
    assert_eq!(
        resumed.handshake(60_000).await,
        json!({"op": 6, "d": {"token": TOKEN, "session_id": SESSION, "seq": 7}})
    );
}

#[tokio::test]
async fn a_reconnect_request_resumes_on_the_resume_url() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;
    connection.dispatch(5, "TYPING_START").await;

    connection.send(json!({"op": 7, "d": null})).await;
    connection.dispatch(6, "TYPING_START").await;

    assert_eq!(connection.client_close_code().await, Some(4000));
    let mut resumed = fake.accept().await;
    assert!(resumed.uri.starts_with("/resume?"), "{}", resumed.uri);
    assert_eq!(resumed.handshake(60_000).await["d"]["seq"], 5);
    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Dispatch(DispatchEvent::Other(_))
    ));
    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Reconnecting {
            resume: true,
            reason: DisconnectReason::Requested,
            ..
        }
    ));
    resumed
        .send(json!({"op": 0, "s": 6, "t": "RESUMED", "d": {"_trace": []}}))
        .await;
    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Dispatch(DispatchEvent::Resumed)
    ));
}

#[test]
fn a_gateway_needs_a_runtime() {
    let client = client_for("ws://127.0.0.1:9/".to_owned());

    let result = client.gateway(Token::new(TOKEN.to_owned()));

    assert!(matches!(result, Err(GatewayError::NoRuntime)));
}

#[tokio::test]
async fn an_invalid_session_starts_over_on_the_gateway_url() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    connection.send(json!({"op": 9, "d": false})).await;

    assert_eq!(connection.client_close_code().await, Some(4000));
    let mut fresh = fake.accept().await;
    assert!(fresh.uri.starts_with("/?"), "{}", fresh.uri);
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    match next(&gateway).await {
        ConnectionEvent::Reconnecting {
            resume,
            delay,
            reason,
        } => {
            assert!(!resume);
            assert!(delay >= Duration::from_millis(10), "{delay:?}");
            assert!(matches!(reason, DisconnectReason::InvalidSession));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_resumable_invalid_session_resumes() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    connection.send(json!({"op": 9, "d": true})).await;

    assert_eq!(connection.client_close_code().await, Some(4000));
    let mut resumed = fake.accept().await;
    assert!(resumed.uri.starts_with("/resume?"), "{}", resumed.uri);
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
}

#[tokio::test]
async fn session_close_codes_start_over_and_others_resume() {
    for (code, op, path) in [(4009, 2, "/?"), (4007, 2, "/?"), (4000, 6, "/resume?")] {
        let mut fake = FakeGateway::start().await;
        let gateway = start(&fake);
        let mut connection = connected(&mut fake, &gateway, 60_000).await;

        connection.close(code).await;

        let mut next_connection = fake.accept().await;
        assert!(
            next_connection.uri.starts_with(path),
            "{code}: {}",
            next_connection.uri
        );
        assert_eq!(next_connection.handshake(60_000).await["op"], op, "{code}");
    }
}

#[tokio::test]
async fn a_rejected_token_ends_the_gateway() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;

    connection.close(4004).await;

    assert!(matches!(
        next_error(&gateway).await,
        GatewayError::AuthenticationFailed
    ));
    assert!(matches!(next_error(&gateway).await, GatewayError::Closed));
    assert!(fake.quiet_for(Duration::from_millis(300)).await);
    assert!(matches!(gateway.connect(), Err(GatewayError::Closed)));
}

#[tokio::test]
async fn a_refused_connection_ends_the_gateway() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;

    connection.close(4015).await;

    assert!(matches!(
        next_error(&gateway).await,
        GatewayError::Rejected { code: 4015 }
    ));
    assert!(fake.quiet_for(Duration::from_millis(300)).await);
}

#[tokio::test]
async fn a_ready_that_fails_to_decode_ends_the_gateway() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;
    let mut ready: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/ready.json")).unwrap();
    ready["d"]["user"]["id"] = serde_json::Value::Null;

    connection.send(ready).await;

    assert!(matches!(
        next_error(&gateway).await,
        GatewayError::InvalidReady(_)
    ));
    assert_eq!(connection.client_close_code().await, Some(1000));
    assert!(fake.quiet_for(Duration::from_millis(300)).await);
}

#[tokio::test]
async fn an_unacknowledged_heartbeat_reconnects_and_resumes() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 50).await;

    connection.ack = false;

    assert_eq!(connection.client_close_code().await, Some(4000));
    let mut resumed = fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    assert!(matches!(
        next(&gateway).await,
        ConnectionEvent::Reconnecting {
            reason: DisconnectReason::Zombie,
            ..
        }
    ));
}

fn start_with(fake: &FakeGateway, timing: Timing) -> Gateway {
    let gateway = Gateway::start(client(fake), Token::new(TOKEN.to_owned()), timing).unwrap();
    gateway.connect().unwrap();
    gateway
}

fn presence(status: PresenceStatus) -> GatewayCommand {
    GatewayCommand::UpdatePresence { status }
}

#[tokio::test]
async fn commands_fail_fast_without_a_session() {
    let mut fake = FakeGateway::start().await;
    let gateway = Gateway::start(client(&fake), Token::new(TOKEN.to_owned()), timing()).unwrap();
    let quick = Duration::from_secs(1);
    let online = || gateway.send(presence(PresenceStatus::Online));

    assert_eq!(
        timeout(quick, online()).await.unwrap(),
        Err(SendError::NotConnected)
    );
    gateway.connect().unwrap();
    let mut connection = fake.accept().await;
    assert_eq!(
        timeout(quick, online()).await.unwrap(),
        Err(SendError::NotConnected)
    );
    connection.handshake(60_000).await;
    connection.ready(1, SESSION, &fake.resume_url()).await;
    next(&gateway).await;
    gateway.disconnect();
    assert_eq!(connection.client_close_code().await, Some(4000));
    assert_eq!(
        timeout(quick, online()).await.unwrap(),
        Err(SendError::NotConnected)
    );
}

#[tokio::test]
async fn a_presence_update_reaches_discord() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    gateway
        .send(presence(PresenceStatus::DoNotDisturb))
        .await
        .unwrap();

    assert_eq!(
        connection.recv().await,
        Some(json!({"op": 3, "d": {"since": 0, "activities": [], "status": "dnd", "afk": false}}))
    );
}

#[tokio::test]
async fn commands_are_written_in_order() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;

    for n in 0..3 {
        gateway
            .send_payload(json!({"op": 99, "d": n}).to_string())
            .await
            .unwrap();
    }

    for n in 0..3 {
        assert_eq!(connection.recv().await, Some(json!({"op": 99, "d": n})));
    }
}

#[tokio::test]
async fn oversized_commands_are_refused() {
    let fake = FakeGateway::start().await;
    let gateway = start(&fake);

    let result = gateway.send_payload("x".repeat(15 * 1024 + 1)).await;

    assert_eq!(result, Err(SendError::TooLarge));
}

#[tokio::test]
async fn sends_after_close_fail() {
    let mut fake = FakeGateway::start().await;
    let gateway = start(&fake);
    let mut connection = connected(&mut fake, &gateway, 60_000).await;
    gateway.close();
    connection.client_close_code().await;
    next_error(&gateway).await;

    assert_eq!(
        gateway.send(presence(PresenceStatus::Idle)).await,
        Err(SendError::Closed)
    );
}

#[tokio::test]
async fn the_rate_limit_leaves_room_for_heartbeats() {
    let mut fake = FakeGateway::start().await;
    let window = Duration::from_secs(3);
    let gateway = Arc::new(start_with(
        &fake,
        Timing {
            rate_window: window,
            ..timing()
        },
    ));
    let mut connection = connected(&mut fake, &gateway, 500).await;
    let sender = gateway.clone();
    let sends = tokio::spawn(async move {
        for n in 0..130 {
            sender
                .send_payload(json!({"op": 99, "d": n}).to_string())
                .await
                .unwrap();
        }
    });

    let commands = connection
        .pump_until(Duration::from_secs(30), |_, payloads| payloads.len() == 130)
        .await;
    sends.await.unwrap();

    assert_eq!(connection.close_code, None);
    let numbers: Vec<_> = commands
        .iter()
        .map(|command| command["d"].clone())
        .collect();
    assert_eq!(numbers, (0..130).map(|n| json!(n)).collect::<Vec<_>>());
    let received = &connection.received;
    for (index, start) in received.iter().enumerate() {
        let in_window = received[index..]
            .iter()
            .take_while(|at| **at < *start + window)
            .count();
        assert!(in_window <= 120, "{in_window} messages within one window");
    }
    let times = &connection.payload_times;
    let (held_from, held_until) = times
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .max_by_key(|(from, until)| *until - *from)
        .unwrap();
    assert!(
        connection
            .heartbeats
            .iter()
            .any(|(at, _)| (held_from..held_until).contains(at)),
        "no heartbeat while commands were held back"
    );
}
