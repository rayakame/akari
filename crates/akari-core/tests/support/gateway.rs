use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

pub const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub struct Logs(Arc<Mutex<Vec<u8>>>);

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

pub async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (stream, _) = timeout(WAIT, listener.accept())
        .await
        .unwrap_or_else(|_| panic!("the client didn't connect"))
        .unwrap_or_else(|err| panic!("accept failed: {err}"));
    tokio_tungstenite::accept_async(stream)
        .await
        .unwrap_or_else(|err| panic!("handshake failed: {err}"))
}

pub async fn send(ws: &mut WebSocketStream<TcpStream>, payload: Value) {
    ws.send(Message::text(payload.to_string()))
        .await
        .unwrap_or_else(|err| panic!("send failed: {err}"));
}

pub async fn recv(ws: &mut WebSocketStream<TcpStream>) -> Option<Value> {
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

pub async fn handshake(ws: &mut WebSocketStream<TcpStream>) -> Value {
    send(ws, json!({"op": 10, "d": {"heartbeat_interval": 60_000}})).await;
    recv(ws).await.unwrap_or_else(|| panic!("no handshake"))
}

impl Logs {
    /// Installs a global TRACE subscriber writing into the returned buffer. A test binary
    /// that uses it must hold a single test: tracing caches callsite interest process-wide.
    pub fn capture() -> Self {
        let logs = Self::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap_or_else(|err| panic!("{err}"));
        logs
    }

    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap_or_else(|err| panic!("{err}")).clone())
            .unwrap_or_else(|err| panic!("{err}"))
    }
}
