// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use akari_core::media::{ImageKey, ImageRequest};
use akari_core::model::Snowflake;
use akari_core::{DiscordClient, Endpoints, MessageLoad, Token};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::support::gateway::Logs;

const CHANNEL: u64 = 300_000_000_000_000_078;
const MESSAGE: u64 = 400_000_000_000_000_078;
const ATTACHMENT: u64 = 400_000_000_000_000_079;
const TOKEN: &str = "media-log-test-token.secret";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

// A PNG header of 1024 × 576.
fn png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(1024u32.to_be_bytes());
    bytes.extend(576u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

#[tokio::test]
async fn image_logs_name_keys_never_links_or_signatures() {
    let logs = Logs::capture();
    let server = MockServer::start().await;
    let file = format!("/attachments/{CHANNEL}/{ATTACHMENT}/secret-file-name.png");
    let stale = now() + 60;
    let fresh = now() + 86_400;
    let link = |ex: i64| {
        format!(
            "{}{file}?ex={ex:x}&is={:x}&hm=5ec2e75160a7a7e2e&",
            server.uri(),
            ex - 86_400
        )
    };
    let page = json!([{
        "id": MESSAGE.to_string(),
        "type": 0,
        "content": "",
        "channel_id": CHANNEL.to_string(),
        "author": {"id": "4243", "username": "mira", "global_name": null,
                   "avatar": null, "discriminator": "0", "public_flags": 0},
        "attachments": [{
            "id": ATTACHMENT.to_string(), "filename": "secret-file-name.png", "size": 2000,
            "url": link(stale), "proxy_url": link(stale), "width": 1920, "height": 1080
        }],
        "embeds": [], "mentions": [], "mention_roles": [],
        "pinned": false, "mention_everyone": false, "tts": false,
        "timestamp": "2026-10-11T12:00:00.000000+00:00", "edited_timestamp": null,
        "flags": 0, "components": []
    }]);
    Mock::given(path(format!("/api/v9/channels/{CHANNEL}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .mount(&server)
        .await;
    let renewed = link(fresh);
    Mock::given(method("POST"))
        .and(path("/api/v9/attachments/refresh-urls"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            ResponseTemplate::new(200).set_body_json(json!({"refreshed_urls": [{
                "original": body["attachment_urls"][0], "refreshed": renewed
            }]}))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(file.clone()))
        .and(query_param("ex", format!("{fresh:x}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(png()),
        )
        .mount(&server)
        .await;
    let directory = std::env::temp_dir().join(format!("akari-media-logs-{}", std::process::id()));
    let endpoints = Endpoints {
        gateway: "ws://127.0.0.1:9/".to_owned(),
        api: format!("{}/api/v9/", server.uri()),
        cdn: format!("{}/", server.uri()),
        media: format!("{}/", server.uri()),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"))
            .with_image_cache(directory.clone());
    let account = client
        .account(Token::new(TOKEN.to_owned()), Snowflake::new(1))
        .unwrap_or_else(|err| panic!("{err}"));
    // Connecting (to nothing) is enough: images aren't fetched while offline.
    account.connect().unwrap_or_else(|err| panic!("{err}"));
    account
        .load_messages(Snowflake::new(CHANNEL), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap_or_else(|err| panic!("{err}"));

    let image = account
        .image(ImageRequest {
            key: ImageKey::Attachment {
                channel: Snowflake::new(CHANNEL),
                message: Snowflake::new(MESSAGE),
                id: Snowflake::new(ATTACHMENT),
            },
            width: 800,
            height: 450,
            animated: None,
        })
        .await;
    account.close();
    let _ = std::fs::remove_dir_all(&directory);

    assert!(image.is_ok(), "{image:?}");
    let text = logs.text();
    assert!(text.contains("links refreshed"), "{text}");
    assert!(
        text.lines().any(|line| line.contains("image downloaded")
            && line.contains(&format!("attachment/{ATTACHMENT}"))),
        "{text}"
    );
    for secret in [
        "ex=",
        "hm=",
        "5ec2e75160a7a7e2e",
        "secret-file-name",
        "/attachments/",
        &server.uri(),
        TOKEN,
    ] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}
