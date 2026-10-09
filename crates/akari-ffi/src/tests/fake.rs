use std::net::SocketAddr;
use std::sync::Arc;

use akari_core::model::{ChannelId, GuildId, MessageId};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;

use super::gateway::{WAIT, accept, handshake, send};
use super::support::{MemoryStore, local_client};
use crate::client::DiscordClient;
use crate::records::ConnectionState;
use crate::subscription::{StoreEvent, StoreSubscription};

pub const GUILD: GuildId = GuildId::new(200_000_000_000_000_001);
pub const DOWN: GuildId = GuildId::new(200_000_000_000_000_002);
pub const CATEGORY: ChannelId = ChannelId::new(300_000_000_000_000_001);
pub const GENERAL: ChannelId = ChannelId::new(300_000_000_000_000_002);
pub const VOICE: ChannelId = ChannelId::new(300_000_000_000_000_003);
pub const MESSAGE: MessageId = MessageId::new(400_000_000_000_000_003);

pub struct FakeGateway {
    listener: TcpListener,
    pub address: SocketAddr,
}

impl FakeGateway {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        Self { listener, address }
    }

    pub fn client(&self) -> Arc<DiscordClient> {
        self.client_with_api("http://127.0.0.1:9/api/v9/")
    }

    pub fn client_with_api(&self, api: &str) -> Arc<DiscordClient> {
        let endpoints = akari_core::Endpoints {
            gateway: format!("ws://{}/", self.address),
            api: api.to_owned(),
            ..akari_core::Endpoints::default()
        };
        local_client(endpoints, Arc::new(MemoryStore::default()))
    }

    pub async fn serve_ready(&self) -> WebSocketStream<TcpStream> {
        let mut ws = accept(&self.listener).await;
        assert_eq!(handshake(&mut ws).await["op"], 2);
        let mut ready: Value = fixture(include_str!(
            "../../../akari-core/tests/fixtures/ready.json"
        ));
        ready["d"]["resume_gateway_url"] = format!("ws://{}/resume", self.address).into();
        send(&mut ws, ready).await;
        ws
    }
}

pub fn fixture(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

pub async fn events_until(
    subscription: &StoreSubscription,
    last: impl Fn(&StoreEvent) -> bool,
) -> Vec<StoreEvent> {
    let mut events = Vec::new();
    timeout(WAIT, async {
        while !events.iter().any(&last) {
            let batch = subscription.next().await.expect("the subscription ended");
            events.extend(batch);
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the event never arrived; got {events:?}"));
    events
}

pub async fn drain(subscription: &StoreSubscription) -> Vec<StoreEvent> {
    let mut events = Vec::new();
    timeout(WAIT, async {
        while let Some(batch) = subscription.next().await {
            events.extend(batch);
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the subscription never ended; got {events:?}"));
    events
}

pub fn online(event: &StoreEvent) -> bool {
    matches!(
        event,
        StoreEvent::Connection {
            state: ConnectionState::Online
        }
    )
}
