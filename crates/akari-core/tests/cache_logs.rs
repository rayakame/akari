// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::path::PathBuf;
use std::sync::Arc;

use akari_core::model::Snowflake;
use akari_core::state::{StoreEvent, Subscription};
use akari_core::{DiscordClient, Endpoints, MessageLoad, Token};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::time::timeout;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::gateway::{Logs, WAIT, accept, handshake, send};

const ME: u64 = 100_000_000_000_000_001;
const GENERAL: u64 = 300_000_000_000_000_002;
const TOKEN: &str = "cache-log-test-token.secret";

async fn until(subscription: &Subscription, wanted: fn(&StoreEvent) -> bool) {
    timeout(WAIT, async {
        while let Some(event) = subscription.next().await {
            if wanted(&event) {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the event never arrived"));
}

fn page() -> Value {
    json!([{
        "id": "400000000000000077",
        "type": 0,
        "content": "secret-content-77",
        "channel_id": GENERAL.to_string(),
        "author": {"id": "4242", "username": "Mira-Name", "global_name": "Mira-Global",
                   "avatar": null, "discriminator": "0", "public_flags": 0},
        "attachments": [{"id": "500", "filename": "secret-file-name.png", "size": 1,
                         "url": "https://cdn.example/a", "proxy_url": "https://media.example/a"}],
        "embeds": [], "mentions": [], "mention_roles": [],
        "pinned": false, "mention_everyone": false, "tts": false,
        "timestamp": "2026-10-11T12:00:00.000000+00:00", "edited_timestamp": null,
        "flags": 0, "components": []
    }])
}

// Every name the READY fixture holds: guilds, channels, users.
fn names(ready: &Value) -> Vec<String> {
    let mut names = Vec::new();
    let mut collect = |value: &Value| {
        for key in ["name", "username", "global_name"] {
            if let Some(name) = value[key].as_str() {
                names.push(name.to_owned());
            }
        }
    };
    collect(&ready["user"]);
    for guild in ready["guilds"].as_array().into_iter().flatten() {
        collect(guild);
        collect(&guild["properties"]);
        for channel in guild["channels"].as_array().into_iter().flatten() {
            collect(channel);
        }
    }
    for user in ready["users"].as_array().into_iter().flatten() {
        collect(user);
    }
    names.retain(|name| name.len() > 3);
    names
}

#[tokio::test]
async fn the_cache_log_holds_numbers_only() {
    let logs = Logs::capture();
    let dir = std::env::temp_dir().join(format!("akari-cache-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|err| panic!("bind failed: {err}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|err| panic!("no address: {err}"));
    let server = MockServer::start().await;
    Mock::given(path(format!("/api/v9/channels/{GENERAL}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    let endpoints = Endpoints {
        gateway: format!("ws://{address}/"),
        api: format!("{}/api/v9/", server.uri()),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"))
            .with_cache(PathBuf::from(&dir));
    let account = client
        .account(Token::new(TOKEN.to_owned()), Snowflake::new(ME))
        .unwrap_or_else(|err| panic!("{err}"));
    let subscription = account.store().subscribe();
    account.connect().unwrap_or_else(|err| panic!("{err}"));
    let mut ws = accept(&listener).await;
    handshake(&mut ws).await;
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"]["resume_gateway_url"] = format!("ws://{address}/resume").into();
    let secrets = names(&ready["d"]);
    send(&mut ws, ready).await;
    until(&subscription, |event| matches!(event, StoreEvent::Ready)).await;
    account
        .load_messages(Snowflake::new(GENERAL), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_or_else(|err| panic!("{err}"));
    account.flush_cache().await;
    account.close();
    drop(account);

    let again = client
        .account(Token::new(TOKEN.to_owned()), Snowflake::new(ME))
        .unwrap_or_else(|err| panic!("{err}"));
    let restored = again.store().subscribe();
    until(&restored, |event| matches!(event, StoreEvent::Restored)).await;
    again
        .load_messages(Snowflake::new(GENERAL), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_or_else(|err| panic!("{err}"));
    again.close();

    let text = logs.text();
    let cache: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("akari_core::cache"))
        .collect();
    for wanted in ["cache written", "cache opened"] {
        assert!(
            cache.iter().any(|line| line.contains(wanted)),
            "no {wanted:?}: {text}"
        );
    }
    let mut forbidden = secrets;
    forbidden.extend(
        [
            "secret-content-77",
            "secret-file-name",
            "Mira-Name",
            "Mira-Global",
            "4242",
            "100000000000000001",
            "cache-log-test-token",
        ]
        .map(str::to_owned),
    );
    assert!(forbidden.len() > 8, "{forbidden:?}");
    for line in &cache {
        for secret in &forbidden {
            assert!(!line.contains(secret.as_str()), "{secret} in {line}");
        }
    }
    assert!(!text.contains("cache-log-test-token"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
