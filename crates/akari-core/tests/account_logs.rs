// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;

use akari_core::state::StoreEvent;
use akari_core::{DiscordClient, Endpoints, Token};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::support::gateway::{Logs, WAIT, accept, handshake, send};

const GENERAL: &str = "300000000000000002";

#[tokio::test]
async fn a_broken_dispatch_is_skipped_without_logging_its_payload() {
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
    let account = client
        .account(Token::new("account-log-test-token.secret".to_owned()))
        .unwrap_or_else(|err| panic!("{err}"));
    let subscription = account.store().subscribe();
    account.connect().unwrap_or_else(|err| panic!("{err}"));

    let mut ws = accept(&listener).await;
    assert_eq!(handshake(&mut ws).await["op"], 2);
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"]["resume_gateway_url"] = format!("ws://{address}/resume").into();
    send(&mut ws, ready).await;
    let mut broken: Value = serde_json::from_str(include_str!("fixtures/message_create.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    broken["author"] = json!({"id": "leak-me", "username": "leak-me"});
    broken["content"] = "leak-me-too".into();
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": broken}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "CHANNEL_UPDATE", "d": {"id": GENERAL, "name": "renamed"}}),
    )
    .await;

    let updated = timeout(WAIT, async {
        while let Some(event) = subscription.next().await {
            if let StoreEvent::ChannelUpdated(channel) = event {
                return Some(channel);
            }
        }
        None
    })
    .await
    .unwrap_or_else(|_| panic!("the update after the broken dispatch never arrived"));
    assert_eq!(
        updated.and_then(|channel| channel.name.clone()).as_deref(),
        Some("renamed")
    );
    account.close();

    let text = logs.text();
    let skipped = text
        .lines()
        .find(|line| line.contains("skipping a dispatch that failed to decode"))
        .unwrap_or_else(|| panic!("no skip was logged: {text}"));
    assert!(skipped.contains("MESSAGE_CREATE"), "{skipped}");
    assert!(skipped.contains("seq=2"), "{skipped}");
    assert!(!text.contains("leak-me"), "{text}");
}
