// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;

use akari_core::model::Snowflake;
use akari_core::{DiscordClient, Endpoints, MessageLoad, Token};
use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::gateway::Logs;

const CHANNEL: u64 = 300_000_000_000_000_077;

#[tokio::test]
async fn the_load_log_holds_numbers_and_the_channel_only() {
    let logs = Logs::capture();
    let server = MockServer::start().await;
    let page = json!([{
        "id": "400000000000000077",
        "type": 0,
        "content": "secret-content-77",
        "channel_id": CHANNEL.to_string(),
        "author": {"id": "4242", "username": "Mira-Name", "global_name": "Mira-Global",
                   "avatar": null, "discriminator": "0", "public_flags": 0},
        "attachments": [], "embeds": [], "mentions": [], "mention_roles": [],
        "pinned": false, "mention_everyone": false, "tts": false,
        "timestamp": "2026-10-11T12:00:00.000000+00:00", "edited_timestamp": null,
        "flags": 0, "components": []
    }]);
    Mock::given(path(format!("/api/v9/channels/{CHANNEL}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .mount(&server)
        .await;
    let endpoints = Endpoints {
        gateway: "ws://127.0.0.1:9/".to_owned(),
        api: format!("{}/api/v9/", server.uri()),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"));
    let account = client
        .account(
            Token::new("load-log-test-token.secret".to_owned()),
            Snowflake::new(1),
        )
        .unwrap_or_else(|err| panic!("{err}"));

    account
        .load_messages(Snowflake::new(CHANNEL), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_or_else(|err| panic!("{err}"));
    account.close();

    let text = logs.text();
    let line = text
        .lines()
        .find(|line| line.contains("history page loaded"))
        .unwrap_or_else(|| panic!("no load line: {text}"));
    assert!(line.contains(&CHANNEL.to_string()), "{line}");
    assert!(line.contains("headers_ms="), "{line}");
    assert!(
        text.lines()
            .any(|line| line.contains("rest connection opened") && line.contains("connect_ms=")),
        "{text}"
    );
    for secret in [
        "secret-content-77",
        "Mira-Name",
        "Mira-Global",
        "4242",
        "load-log-test-token",
    ] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}
