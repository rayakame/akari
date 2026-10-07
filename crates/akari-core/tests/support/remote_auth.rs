use std::time::Duration;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rsa::{OAEP_SHA256_MGF1SHA256, OaepPublicEncryptingKey, PublicEncryptingKey};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderMap;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const WAIT: Duration = Duration::from_secs(5);

pub struct RemoteAuthServer {
    url: String,
    sessions: mpsc::UnboundedReceiver<Session>,
}

impl RemoteAuthServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|err| panic!("bind failed: {err}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|err| panic!("no address: {err}"));
        let (sessions, receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let sessions = sessions.clone();
                tokio::spawn(async move {
                    let mut headers = HeaderMap::new();
                    // The error type is tungstenite's, fixed by its callback signature.
                    #[allow(clippy::result_large_err)]
                    let callback = |request: &Request, response: Response| {
                        headers = request.headers().clone();
                        Ok(response)
                    };
                    if let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, callback).await {
                        let _ = sessions.send(Session::new(ws, headers));
                    }
                });
            }
        });
        Self {
            url: format!("ws://{address}/?v=2"),
            sessions: receiver,
        }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub async fn accept(&mut self) -> Session {
        timeout(WAIT, self.sessions.recv())
            .await
            .unwrap_or_else(|_| panic!("the client didn't connect"))
            .unwrap_or_else(|| panic!("the server stopped"))
    }
}

pub struct Session {
    ws: WebSocketStream<TcpStream>,
    pub headers: HeaderMap,
    spki: Vec<u8>,
    pub ack: bool,
    pub heartbeats: usize,
}

impl Session {
    fn new(ws: WebSocketStream<TcpStream>, headers: HeaderMap) -> Self {
        Self {
            ws,
            headers,
            spki: Vec::new(),
            ack: true,
            heartbeats: 0,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    pub async fn send(&mut self, packet: Value) {
        self.ws
            .send(Message::text(packet.to_string()))
            .await
            .unwrap_or_else(|err| panic!("send failed: {err}"));
    }

    // Answers heartbeats on the way; None once the client closed.
    pub async fn recv(&mut self) -> Option<Value> {
        loop {
            let message = timeout(WAIT, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("the client went quiet"));
            match message {
                Some(Ok(Message::Text(text))) => {
                    let packet: Value = serde_json::from_str(&text)
                        .unwrap_or_else(|err| panic!("client sent bad JSON: {err}"));
                    if packet["op"] == "heartbeat" {
                        self.heartbeats += 1;
                        if self.ack {
                            self.send(json!({"op": "heartbeat_ack"})).await;
                        }
                        continue;
                    }
                    return Some(packet);
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        }
    }

    // False if the client closed meanwhile.
    pub async fn pump(&mut self, duration: Duration) -> bool {
        match timeout(duration, self.recv()).await {
            Err(_) => true,
            Ok(None) => false,
            Ok(Some(packet)) => panic!("unexpected packet while pumping: {packet}"),
        }
    }

    pub async fn closed_by_client(&mut self) -> bool {
        timeout(WAIT, async { while self.recv().await.is_some() {} })
            .await
            .is_ok()
    }

    pub async fn hello(&mut self, heartbeat_interval: u64) {
        self.send(
            json!({"op": "hello", "heartbeat_interval": heartbeat_interval, "timeout_ms": 120_000}),
        )
        .await;
    }

    pub async fn handshake(&mut self, heartbeat_interval: u64) -> String {
        self.handshake_until_fingerprint(heartbeat_interval).await;
        let fingerprint = URL_SAFE_NO_PAD.encode(digest(&SHA256, &self.spki));
        self.send(json!({"op": "pending_remote_init", "fingerprint": fingerprint}))
            .await;
        fingerprint
    }

    pub async fn handshake_until_fingerprint(&mut self, heartbeat_interval: u64) {
        self.hello(heartbeat_interval).await;
        let init = self.recv().await.unwrap_or_else(|| panic!("no init"));
        assert_eq!(init["op"], "init");
        self.spki = STANDARD
            .decode(init["encoded_public_key"].as_str().unwrap_or_default())
            .unwrap_or_else(|err| panic!("public key isn't base64: {err}"));

        let mut nonce = [0_u8; 32];
        aws_lc_rs::rand::fill(&mut nonce).unwrap_or_else(|_| panic!("no randomness"));
        let encrypted_nonce = self.encrypt(&nonce);
        self.send(json!({"op": "nonce_proof", "encrypted_nonce": encrypted_nonce}))
            .await;
        let proof = self
            .recv()
            .await
            .unwrap_or_else(|| panic!("no nonce proof"));
        assert_eq!(proof["op"], "nonce_proof");
        assert_eq!(proof["nonce"], URL_SAFE_NO_PAD.encode(nonce));
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> String {
        let public = PublicEncryptingKey::from_der(&self.spki)
            .ok()
            .and_then(|key| OaepPublicEncryptingKey::new(key).ok())
            .unwrap_or_else(|| panic!("the client's public key isn't RSA"));
        let mut ciphertext = vec![0; public.ciphertext_size()];
        let ciphertext = public
            .encrypt(&OAEP_SHA256_MGF1SHA256, plaintext, &mut ciphertext, None)
            .unwrap_or_else(|_| panic!("encryption failed"));
        STANDARD.encode(ciphertext)
    }

    pub async fn scan(&mut self, user_payload: &str) {
        let encrypted = self.encrypt(user_payload.as_bytes());
        self.send(json!({"op": "pending_ticket", "encrypted_user_payload": encrypted}))
            .await;
    }

    pub async fn finish(&mut self, ticket: &str) {
        self.send(json!({"op": "pending_login", "ticket": ticket}))
            .await;
        self.close(1000).await;
    }

    pub async fn close(&mut self, code: u16) {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        };
        let _ = self.ws.close(Some(frame)).await;
    }
}
