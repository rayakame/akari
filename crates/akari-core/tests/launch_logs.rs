// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;

use akari_core::state::StoreEvent;
use akari_core::{DiscordClient, Endpoints, Token};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::support::gateway::{Logs, WAIT, accept, handshake, send};

#[tokio::test]
async fn a_connect_logs_its_milestones_with_durations_only() {
    let logs = Logs::capture();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|err| panic!("bind failed: {err}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|err| panic!("no address: {err}"));
    let endpoints = Endpoints {
        gateway: format!("ws://{address}/"),
        api: "http://127.0.0.1:9/api/v9/".to_owned(),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"));
    let account = client
        .account(Token::new("launch-log-test-token.secret".to_owned()))
        .unwrap_or_else(|err| panic!("{err}"));
    let subscription = account.store().subscribe();
    account.connect().unwrap_or_else(|err| panic!("{err}"));

    let mut ws = accept(&listener).await;
    assert_eq!(handshake(&mut ws).await["op"], 2);
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"]["resume_gateway_url"] = format!("ws://{address}/resume").into();
    send(&mut ws, ready).await;
    timeout(WAIT, async {
        while let Some(event) = subscription.next().await {
            if matches!(event, StoreEvent::Ready) {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("READY never arrived"));
    account.close();

    let text = logs.text();
    let line = |name: &str| {
        text.lines()
            .find(|line| line.contains(name))
            .unwrap_or_else(|| panic!("no {name:?} log: {text}"))
            .to_owned()
    };
    for (name, field) in [
        ("gateway connected", "connect_ms="),
        ("READY received", "after_connect_ms="),
        ("READY applied", "convert_ms="),
    ] {
        let line = line(name);
        assert!(line.contains("INFO"), "{line}");
        assert!(line.contains(field), "{line}");
    }
    assert!(!text.contains("launch-log-test-token"), "{text}");
    assert!(!text.contains("100000000000000001"), "{text}");
}
