// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;

use akari_core::gateway::{ConnectionEvent, DispatchEvent};
use akari_core::{DiscordClient, Endpoints, Token};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::support::gateway::{Logs, WAIT, accept, handshake, recv, send};

const TOKEN: &str = "gateway-log-test-token.secret";

#[tokio::test]
async fn the_token_stays_out_of_the_logs() {
    let logs = Logs::capture();
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

    let text = logs.text();
    assert!(text.contains("reconnecting"), "no logs were captured");
    assert!(!text.contains(TOKEN));
    assert!(!format!("{gateway:?}").contains(TOKEN));
}
