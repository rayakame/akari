// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use akari_core::gateway::{ConnectionEvent, DispatchEvent};
use akari_core::{DiscordClient, Endpoints, Token};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

const TOKEN: &str = "gateway-log-test-token.secret";
const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl io::Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|err| panic!("{err}"))
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (stream, _) = timeout(WAIT, listener.accept())
        .await
        .unwrap_or_else(|_| panic!("the client didn't connect"))
        .unwrap_or_else(|err| panic!("accept failed: {err}"));
    tokio_tungstenite::accept_async(stream)
        .await
        .unwrap_or_else(|err| panic!("handshake failed: {err}"))
}

async fn send(ws: &mut WebSocketStream<TcpStream>, payload: Value) {
    ws.send(Message::text(payload.to_string()))
        .await
        .unwrap_or_else(|err| panic!("send failed: {err}"));
}

async fn recv(ws: &mut WebSocketStream<TcpStream>) -> Option<Value> {
    loop {
        match timeout(WAIT, ws.next())
            .await
            .unwrap_or_else(|_| panic!("the client went quiet"))
        {
            Some(Ok(Message::Text(text))) => {
                let payload: Value =
                    serde_json::from_str(&text).unwrap_or_else(|err| panic!("bad JSON: {err}"));
                if payload["op"] != 1 {
                    return Some(payload);
                }
            }
            Some(Ok(Message::Close(_)) | Err(_)) | None => return None,
            Some(Ok(_)) => {}
        }
    }
}

async fn handshake(ws: &mut WebSocketStream<TcpStream>) -> Value {
    send(ws, json!({"op": 10, "d": {"heartbeat_interval": 60_000}})).await;
    recv(ws).await.unwrap_or_else(|| panic!("no handshake"))
}

#[tokio::test]
async fn the_token_stays_out_of_the_logs() {
    let logs = Logs::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap_or_else(|err| panic!("{err}"));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|err| panic!("bind failed: {err}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|err| panic!("no address: {err}"));
    let endpoints = Endpoints {
        gateway: format!("ws://{address}/"),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"));
    let gateway = client
        .gateway(Token::new(TOKEN.to_owned()))
        .unwrap_or_else(|err| panic!("{err}"));
    gateway.connect().unwrap_or_else(|err| panic!("{err}"));

    let mut first = accept(&listener).await;
    assert_eq!(handshake(&mut first).await["op"], 2);
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"]["resume_gateway_url"] = format!("ws://{address}/resume").into();
    send(&mut first, ready).await;
    let event = timeout(WAIT, gateway.next()).await;
    assert!(
        matches!(
            event,
            Ok(Ok(ConnectionEvent::Dispatch(DispatchEvent::Ready(_))))
        ),
        "{event:?}"
    );
    send(&mut first, json!({"op": 7, "d": null})).await;
    while recv(&mut first).await.is_some() {}
    let mut resumed = accept(&listener).await;
    assert_eq!(handshake(&mut resumed).await["op"], 6);
    gateway.close();
    while recv(&mut resumed).await.is_some() {}

    let text = String::from_utf8(logs.0.lock().unwrap_or_else(|err| panic!("{err}")).clone())
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(text.contains("reconnecting"), "no logs were captured");
    assert!(!text.contains(TOKEN));
    assert!(!format!("{gateway:?}").contains(TOKEN));
}
