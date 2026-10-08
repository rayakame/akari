use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderMap;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

use crate::gateway::decompress::ZstdCompressor;

pub(super) const WAIT: Duration = Duration::from_secs(5);
const READY: &str = include_str!("../../../tests/fixtures/ready.json");

pub(super) struct FakeGateway {
    address: SocketAddr,
    connections: mpsc::UnboundedReceiver<FakeConnection>,
}

impl FakeGateway {
    pub(super) async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, connections) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let sender = sender.clone();
                tokio::spawn(async move {
                    let mut uri = String::new();
                    let mut headers = HeaderMap::new();
                    // The error type is tungstenite's, fixed by its callback signature.
                    #[allow(clippy::result_large_err)]
                    let callback = |request: &Request, response: Response| {
                        uri = request.uri().to_string();
                        headers = request.headers().clone();
                        Ok(response)
                    };
                    if let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, callback).await {
                        let _ = sender.send(FakeConnection::new(ws, uri, headers));
                    }
                });
            }
        });
        Self {
            address,
            connections,
        }
    }

    pub(super) fn url(&self) -> String {
        format!("ws://{}/", self.address)
    }

    pub(super) fn resume_url(&self) -> String {
        format!("ws://{}/resume", self.address)
    }

    pub(super) async fn accept(&mut self) -> FakeConnection {
        timeout(WAIT, self.connections.recv())
            .await
            .expect("the client didn't connect")
            .expect("the server stopped")
    }

    pub(super) async fn quiet_for(&mut self, duration: Duration) -> bool {
        timeout(duration, self.connections.recv()).await.is_err()
    }
}

pub(super) struct FakeConnection {
    ws: WebSocketStream<TcpStream>,
    pub(super) uri: String,
    pub(super) headers: HeaderMap,
    compressor: Option<ZstdCompressor>,
    pub(super) ack: bool,
    pub(super) heartbeats: Vec<(Instant, Value)>,
    pub(super) received: Vec<Instant>,
    pub(super) payload_times: Vec<Instant>,
    pub(super) close_code: Option<Option<u16>>,
}

impl FakeConnection {
    fn new(ws: WebSocketStream<TcpStream>, uri: String, headers: HeaderMap) -> Self {
        let compressor = uri
            .contains("compress=zstd-stream")
            .then(ZstdCompressor::new);
        Self {
            ws,
            uri,
            headers,
            compressor,
            ack: true,
            heartbeats: Vec::new(),
            received: Vec::new(),
            payload_times: Vec::new(),
            close_code: None,
        }
    }

    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    pub(super) fn send_text_frames(&mut self) {
        self.compressor = None;
    }

    pub(super) async fn send(&mut self, payload: Value) {
        let text = payload.to_string();
        let message = match &mut self.compressor {
            Some(compressor) => Message::binary(compressor.message(text.as_bytes())),
            None => Message::text(text),
        };
        self.ws.send(message).await.unwrap();
    }

    pub(super) async fn recv(&mut self) -> Option<Value> {
        loop {
            let message = timeout(WAIT, self.ws.next())
                .await
                .expect("the client went quiet");
            match message {
                Some(Ok(Message::Text(text))) => {
                    self.received.push(Instant::now());
                    let payload: Value = serde_json::from_str(&text).unwrap();
                    if payload["op"] == 1 {
                        self.heartbeats.push((Instant::now(), payload["d"].clone()));
                        if self.ack {
                            self.send(json!({"op": 11, "d": null})).await;
                        }
                        continue;
                    }
                    self.payload_times.push(Instant::now());
                    return Some(payload);
                }
                Some(Ok(Message::Close(frame))) => {
                    self.close_code = Some(frame.map(|frame| u16::from(frame.code)));
                    return None;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    self.close_code.get_or_insert(None);
                    return None;
                }
            }
        }
    }

    pub(super) async fn pump(&mut self, duration: Duration) -> Vec<Value> {
        let mut payloads = Vec::new();
        let _ = timeout(duration, async {
            while let Some(payload) = self.recv().await {
                payloads.push(payload);
            }
        })
        .await;
        payloads
    }

    pub(super) async fn pump_until(
        &mut self,
        limit: Duration,
        done: impl Fn(&Self, &[Value]) -> bool,
    ) -> Vec<Value> {
        let deadline = Instant::now() + limit;
        let mut payloads = Vec::new();
        while !done(self, &payloads) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "the client never got there");
            payloads.extend(self.pump(left.min(Duration::from_millis(20))).await);
            assert_eq!(self.close_code, None, "the client closed the connection");
        }
        payloads
    }

    pub(super) async fn client_close_code(&mut self) -> Option<u16> {
        while self.recv().await.is_some() {}
        self.close_code.flatten()
    }

    pub(super) async fn hello(&mut self, interval_ms: u64) {
        self.send(json!({
            "op": 10,
            "d": {"heartbeat_interval": interval_ms, "_trace": []},
            "s": null,
            "t": null,
        }))
        .await;
    }

    pub(super) async fn handshake(&mut self, interval_ms: u64) -> Value {
        self.hello(interval_ms).await;
        self.recv().await.expect("no handshake")
    }

    pub(super) async fn ready(&mut self, seq: u64, session_id: &str, resume_url: &str) {
        let mut payload: Value = serde_json::from_str(READY).unwrap();
        payload["s"] = seq.into();
        payload["d"]["session_id"] = session_id.into();
        payload["d"]["resume_gateway_url"] = resume_url.into();
        self.send(payload).await;
    }

    pub(super) async fn dispatch(&mut self, seq: u64, name: &str) {
        self.send(json!({"op": 0, "s": seq, "t": name, "d": {}}))
            .await;
    }

    pub(super) async fn close(&mut self, code: u16) {
        let frame = CloseFrame {
            code: code.into(),
            reason: "".into(),
        };
        let _ = self.ws.close(Some(frame)).await;
    }
}
