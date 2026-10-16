use std::sync::Arc;
use std::time::Duration;

use akari_core::model::ChannelId;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, GUILD, events_until, fixture, online};
use super::support::{USER, token};
use crate::account::{Account, MessageLoad};

struct Online {
    account: Arc<Account>,
    server: MockServer,
    _ws: WebSocketStream<TcpStream>,
}

async fn online_acking() -> Online {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": null})))
        .mount(&server)
        .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    Online {
        account,
        server,
        _ws: ws,
    }
}

fn unread(account: &Account, channel: ChannelId) -> bool {
    account.store().channel_badges(vec![channel])[0]
        .badge
        .unread
}

async fn acks_reaching(server: &MockServer, suffix: &str) -> usize {
    let count = || async {
        let requests = server.received_requests().await.unwrap_or_default();
        requests
            .iter()
            .filter(|request| request.url.path().ends_with(suffix))
            .count()
    };
    timeout(Duration::from_secs(5), async {
        while count().await == 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("no request reached the server");
    sleep(Duration::from_millis(300)).await;
    count().await
}

#[tokio::test]
async fn set_reading_crosses_and_acks() {
    let online = online_acking().await;
    Mock::given(method("GET"))
        .and(path("/api/v9/channels/300000000000000002/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture(include_str!(
                "../../../akari-core/tests/fixtures/messages_page.json"
            ))),
        )
        .mount(&online.server)
        .await;
    online
        .account
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
    assert!(unread(&online.account, GENERAL));

    online.account.set_reading(Some(GENERAL));

    assert!(!unread(&online.account, GENERAL));
    assert_eq!(acks_reaching(&online.server, "/ack").await, 1);
    online.account.set_reading(None);
    online.account.close();
}

#[tokio::test]
async fn mark_read_crosses() {
    let online = online_acking().await;
    assert!(unread(&online.account, GENERAL));

    online.account.mark_read(GENERAL);

    assert!(!unread(&online.account, GENERAL));
    assert_eq!(acks_reaching(&online.server, "/ack").await, 1);
    online.account.close();
}

#[tokio::test]
async fn mark_guilds_read_crosses() {
    let online = online_acking().await;

    online.account.mark_guilds_read(vec![GUILD]);

    assert!(!unread(&online.account, GENERAL));
    assert_eq!(
        acks_reaching(&online.server, "/read-states/ack-bulk").await,
        1
    );
    online.account.close();
}
