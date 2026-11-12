// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use akari_core::media::{AnimatedFormat, EmbedSlot, ImageError, ImageKey, ImageRequest};
use akari_core::model::Snowflake;
use akari_core::{DiscordClient, Endpoints, MessageLoad, Token};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::gateway::Logs;
use crate::support::gif::{self, Shape};

const CHANNEL: u64 = 300_000_000_000_000_091;
const LINKED: u64 = 400_000_000_000_000_091;
const ATTACHED: u64 = 400_000_000_000_000_092;
const ATTACHMENT: u64 = 400_000_000_000_000_093;
const ELSEWHERE: u64 = 400_000_000_000_000_094;
const ELSEWHERE_AS_WEBP: u64 = 400_000_000_000_000_095;
const ELSEWHERE_TOO_HEAVY: u64 = 400_000_000_000_000_096;
const TOKEN: &str = "heavy-gif-test-token.secret";

// A GIF that never played: 10 159 614 bytes (286 MB decoded), which the media proxy gave back
// at 33 532 759, past the 16 MB a display file may weigh.
const UPLOAD: Shape = Shape {
    width: 540,
    height: 405,
    frames: 327,
    bytes: 10_159_614,
};
const PROXY_COPY: usize = 33_532_759;
// The proxy's animated WebP of it: 3.6 KB over the cap at its full size, 7.2 MB at 512 × 384.
const WEBP_FULL: usize = 16 * 1024 * 1024 + 3_686;
const WEBP_SMALL: usize = 7_200_000;
// A byte past what Akari streams to disk.
const TOO_HEAVY: usize = 64 * 1024 * 1024 + 1;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

fn message(id: u64, attachments: Value, embeds: Value) -> Value {
    json!({
        "id": id.to_string(), "type": 0, "content": "", "channel_id": CHANNEL.to_string(),
        "author": {"id": "4243", "username": "mira", "global_name": null,
                   "avatar": null, "discriminator": "0", "public_flags": 0},
        "attachments": attachments, "embeds": embeds,
        "mentions": [], "mention_roles": [], "pinned": false, "mention_everyone": false,
        "tts": false, "timestamp": "2026-10-11T12:00:00.000000+00:00",
        "edited_timestamp": null, "flags": 0, "components": []
    })
}

fn gif_answer(bytes: Vec<u8>) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "image/gif")
        .set_body_bytes(bytes)
}

fn webp_answer(width: u32, height: u32, bytes: usize) -> ResponseTemplate {
    let mut body = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    body.extend(10u32.to_le_bytes());
    body.extend([0x12, 0, 0, 0]);
    body.extend(&(width - 1).to_le_bytes()[..3]);
    body.extend(&(height - 1).to_le_bytes()[..3]);
    body.resize(bytes, 0);
    ResponseTemplate::new(200)
        .insert_header("content-type", "image/webp")
        .set_body_bytes(body)
}

// As an attachment and as a posted link, asked as the list and the viewer do (800 and 4096
// pixels) at the caps the app runs with. Another site's GIF has no upload to fall back to: it
// plays from the proxy's WebP at the box's size where WebP plays and that fits, else from the
// proxy's copy streamed to disk.
#[tokio::test]
async fn a_gif_the_proxy_makes_too_heavy_plays() {
    let logs = Logs::capture();
    let cdn = MockServer::start().await;
    let media = MockServer::start().await;
    let upload = gif::animated(&UPLOAD);
    let copy = gif::heavier(&upload, PROXY_COPY - upload.len());
    assert_eq!(copy.len(), PROXY_COPY);
    assert_eq!(
        UPLOAD.frames * usize::from(UPLOAD.width) * usize::from(UPLOAD.height) * 4,
        286_059_600
    );
    let file = format!("/attachments/{CHANNEL}/{ATTACHMENT}/secret-file-name.gif");
    let external = "/external/abc/https/example.com/secret-file-name.gif";
    let external_as_webp = "/external/def/https/example.com/secret-file-name.gif";
    let external_too_heavy = "/external/ghi/https/example.com/secret-file-name.gif";
    let ex = now() + 86_400;
    let query = format!("ex={ex:x}&is={:x}&hm=5ec2e75160a7a7e2e&", ex - 86_400);
    let link = format!("{}{file}?{query}", cdn.uri());
    let proxied = format!("{}{file}?{query}", media.uri());
    let page = json!([
        message(
            LINKED,
            json!([]),
            json!([{
                "type": "image", "url": link,
                "thumbnail": {"url": link, "proxy_url": proxied, "width": UPLOAD.width,
                              "height": UPLOAD.height, "content_type": "image/gif", "flags": 0}
            }])
        ),
        message(
            ATTACHED,
            json!([{
                "id": ATTACHMENT.to_string(), "filename": "secret-file-name.gif",
                "size": upload.len(), "url": link, "proxy_url": proxied,
                "width": UPLOAD.width, "height": UPLOAD.height, "content_type": "image/gif"
            }]),
            json!([])
        ),
        message(
            ELSEWHERE,
            json!([]),
            json!([{
                "type": "image", "url": "https://example.com/secret-file-name.gif",
                "thumbnail": {"url": "https://example.com/secret-file-name.gif",
                              "proxy_url": format!("{}{external}", media.uri()),
                              "width": UPLOAD.width, "height": UPLOAD.height}
            }])
        ),
        message(
            ELSEWHERE_AS_WEBP,
            json!([]),
            json!([{
                "type": "image", "url": "https://example.com/secret-file-name.gif",
                "thumbnail": {"url": "https://example.com/secret-file-name.gif",
                              "proxy_url": format!("{}{external_as_webp}", media.uri()),
                              "width": UPLOAD.width, "height": UPLOAD.height}
            }])
        ),
        message(
            ELSEWHERE_TOO_HEAVY,
            json!([]),
            json!([{
                "type": "image", "url": "https://example.com/secret-file-name.gif",
                "thumbnail": {"url": "https://example.com/secret-file-name.gif",
                              "proxy_url": format!("{}{external_too_heavy}", media.uri()),
                              "width": UPLOAD.width, "height": UPLOAD.height}
            }])
        ),
    ]);
    Mock::given(path(format!("/api/v9/channels/{CHANNEL}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .mount(&cdn)
        .await;
    Mock::given(method("GET"))
        .and(path(file.clone()))
        .respond_with(gif_answer(upload.clone()))
        .mount(&cdn)
        .await;
    Mock::given(method("GET"))
        .and(path(external_as_webp))
        .and(query_param("format", "webp"))
        .and(query_param("animated", "true"))
        .and(query_param("width", "512"))
        .respond_with(webp_answer(512, 384, WEBP_SMALL))
        .mount(&media)
        .await;
    Mock::given(method("GET"))
        .and(path(external_as_webp))
        .and(query_param("format", "webp"))
        .and(query_param("animated", "true"))
        .respond_with(webp_answer(540, 405, WEBP_FULL))
        .mount(&media)
        .await;
    Mock::given(method("GET"))
        .and(path(external_too_heavy))
        .respond_with(gif_answer(gif::heavier(&upload, TOO_HEAVY - upload.len())))
        .mount(&media)
        .await;
    for copied in [file.as_str(), external, external_as_webp] {
        Mock::given(method("GET"))
            .and(path(copied))
            .respond_with(gif_answer(copy.clone()))
            .mount(&media)
            .await;
    }
    let directory = std::env::temp_dir().join(format!("akari-heavy-gif-{}", std::process::id()));
    let endpoints = Endpoints {
        gateway: "ws://127.0.0.1:9/".to_owned(),
        api: format!("{}/api/v9/", cdn.uri()),
        cdn: format!("{}/", cdn.uri()),
        media: format!("{}/", media.uri()),
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
    let thumbnail = |message: u64| ImageKey::EmbedMedia {
        channel: Snowflake::new(CHANNEL),
        message: Snowflake::new(message),
        embed: 0,
        slot: EmbedSlot::Thumbnail,
    };
    let attached = ImageKey::Attachment {
        channel: Snowflake::new(CHANNEL),
        message: Snowflake::new(ATTACHED),
        id: Snowflake::new(ATTACHMENT),
    };
    let playing = |key: &ImageKey, side: u32| ImageRequest {
        key: key.clone(),
        width: side,
        height: side * 3 / 4,
        animated: Some(AnimatedFormat::Gif),
    };

    let mut answers = Vec::new();
    for key in [thumbnail(LINKED), attached] {
        for side in [800, 4096] {
            let answer = account.image(playing(&key, side)).await;
            let kept = answer
                .as_ref()
                .ok()
                .and_then(|file| std::fs::read(&file.path).ok());
            answers.push((answer, kept));
        }
    }
    let read = |answer: &Result<akari_core::media::ImageFile, ImageError>| {
        answer
            .as_ref()
            .ok()
            .and_then(|file| std::fs::read(&file.path).ok())
    };
    let elsewhere = account.image(playing(&thumbnail(ELSEWHERE), 800)).await;
    let streamed = read(&elsewhere);
    let as_webp = |width: u32, height: u32| ImageRequest {
        key: thumbnail(ELSEWHERE_AS_WEBP),
        width,
        height,
        animated: Some(AnimatedFormat::WebP),
    };
    let small = account.image(as_webp(512, 384)).await;
    let small_weighs = read(&small).map(|bytes| bytes.len());
    let small_again = account.image(as_webp(512, 384)).await;
    let full = account.image(as_webp(4096, 3072)).await;
    let full_streamed = read(&full);
    let full_again = account.image(as_webp(4096, 3072)).await;
    let too_heavy = account
        .image(playing(&thumbnail(ELSEWHERE_TOO_HEAVY), 800))
        .await;
    let asked_as_webp = media
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| {
            request.url.path() == external_as_webp
                && request
                    .url
                    .query()
                    .is_some_and(|query| query.contains("format=webp"))
        })
        .count();
    account.close();
    let _ = std::fs::remove_dir_all(&directory);

    for (answer, kept) in &answers {
        let file = answer
            .as_ref()
            .unwrap_or_else(|err| panic!("the upload plays: {err} {}", logs.text()));
        assert!(file.animated);
        assert_eq!((file.width, file.height), (540, 405));
        assert!(kept.as_ref() == Some(&upload), "the upload, byte for byte");
    }
    let elsewhere = elsewhere.unwrap_or_else(|err| panic!("the copy plays: {err} {}", logs.text()));
    assert!(elsewhere.animated);
    assert!(
        streamed.as_ref() == Some(&copy),
        "the proxy's copy, byte for byte"
    );
    let small = small.unwrap_or_else(|err| panic!("the WebP plays: {err} {}", logs.text()));
    assert!(small.animated);
    assert_eq!((small.width, small.height), (512, 384));
    assert_eq!(small_weighs, Some(WEBP_SMALL));
    assert_eq!(small_again.map(|file| file.from_disk), Ok(true));
    let full = full.unwrap_or_else(|err| panic!("the copy plays: {err} {}", logs.text()));
    assert_eq!((full.width, full.height), (540, 405));
    assert!(
        full_streamed.as_ref() == Some(&copy),
        "the WebP is over the cap too"
    );
    assert_eq!(full_again.map(|file| file.from_disk), Ok(true));
    assert_eq!(too_heavy, Err(ImageError::TooLarge));
    assert_eq!(asked_as_webp, 2, "each box asks its own WebP");
    assert!(
        logs.text()
            .lines()
            .any(|line| line.contains("image over the streamed cap")),
        "{}",
        logs.text()
    );
    let fetched = cdn.received_requests().await.unwrap_or_default();
    let uploads: Vec<_> = fetched
        .iter()
        .filter(|request| request.url.path() == file)
        .collect();
    assert_eq!(uploads.len(), 2, "once per image, then from disk");
    assert!(
        uploads
            .iter()
            .all(|request| request.url.query() == Some(query.trim_end_matches('&')))
    );
    let text = logs.text();
    assert_eq!(
        text.matches("proxy copy too large, original asked").count(),
        2,
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains("image over the display cap")
                && line.contains("copy=\"proxy\"")
                && line.contains("has_original=false")),
        "{text}"
    );
    assert_eq!(
        text.matches("heavy GIF kept as the proxy's WebP").count(),
        1,
        "{text}"
    );
    assert_eq!(
        text.matches("no WebP of a heavy GIF, the file streamed")
            .count(),
        1,
        "{text}"
    );
    for secret in [
        "ex=",
        "hm=",
        "5ec2e75160a7a7e2e",
        "secret-file-name",
        "/attachments/",
        "example.com",
        &cdn.uri(),
        &media.uri(),
        TOKEN,
    ] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}
