use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::*;
use crate::cache::tests::TempDir;
use crate::gateway::fake::client_with_media;
use crate::media::sniff::tests::{png_bytes, webp_bytes};
use crate::media::{AnimatedFormat, EmbedSlot, ImageError, ImageKey, ImageRequest};
use crate::state::ImageHash;

const ME: u64 = 100_000_000_000_000_001;
const AUTHOR: u64 = 100_000_000_000_000_002;
const HASH: &str = "0123456789abcdef0123456789abcdef";
const MESSAGE: u64 = 400_000_000_000_000_050;
const ATTACHMENT: u64 = 400_000_000_000_000_051;
const AVATAR_PATH: &str = "/avatars/100000000000000002/0123456789abcdef0123456789abcdef.webp";
const REFRESH: &str = "/api/v9/attachments/refresh-urls";

struct Online {
    account: Account,
    server: MockServer,
    dir: TempDir,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

impl Online {
    fn uri(&self) -> String {
        self.server.uri()
    }

    fn image_directory(&self) -> std::path::PathBuf {
        self.dir.path().join(ME.to_string())
    }

    async fn requests_to(&self, wanted: &str) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| request.url.path() == wanted)
            .collect()
    }
}

async fn connected() -> Online {
    connected_with(None, TempDir::new()).await
}

// With the CDN at `cdn` instead of the mock server, keeping images under `dir`.
async fn connected_with(cdn: Option<String>, dir: TempDir) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let client = match cdn {
        None => client_with_media(fake.url(), &server.uri()),
        Some(cdn) => crate::gateway::fake::client_with_endpoints(
            crate::Endpoints {
                gateway: fake.url(),
                api: format!("{}/api/v9/", server.uri()),
                cdn: cdn.clone(),
                media: cdn,
                allow_plaintext: true,
                ..crate::Endpoints::default()
            },
            Arc::new(crate::gateway::fake::NoStore),
        ),
    }
    .with_image_cache(dir.path().to_path_buf());
    let account = Account::start_as(
        client,
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
        Snowflake::new(ME),
    )
    .unwrap();
    let subscription = account.store().subscribe();
    let connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        server,
        dir,
        _connection: connection,
        _fake: fake,
    }
}

fn avatar() -> ImageKey {
    ImageKey::Avatar {
        user: Snowflake::new(AUTHOR),
        hash: ImageHash::parse(HASH),
    }
}

fn request(key: ImageKey, side: u32) -> ImageRequest {
    ImageRequest {
        key,
        width: side,
        height: side,
        animated: None,
    }
}

fn webp(side: u32) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "image/webp")
        .set_body_bytes(webp_bytes(side, side, false))
}

fn png(width: u32, height: u32) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "image/png")
        .set_body_bytes(png_bytes(width, height, false))
}

async fn serve(server: &MockServer, at: &str, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(answer)
        .mount(server)
        .await;
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn signature(ex: i64) -> String {
    format!("ex={ex:x}&is={:x}&hm=5eed", ex - 86_400)
}

fn attachment_path() -> String {
    format!("/attachments/{GENERAL}/{ATTACHMENT}/photo.png")
}

fn attachment_json(online: &Online, ex: i64) -> Value {
    attachment_json_at(&online.uri(), ex)
}

// An attachment whose links point at `host`.
fn attachment_json_at(host: &str, ex: i64) -> Value {
    let link = format!("{host}{}?{}&", attachment_path(), signature(ex));
    json!({
        "id": ATTACHMENT.to_string(),
        "filename": "photo.png",
        "size": 2000,
        "url": link,
        "proxy_url": link,
        "width": 1920,
        "height": 1080,
        "content_type": "image/png",
    })
}

fn attachment_key() -> ImageKey {
    ImageKey::Attachment {
        channel: general(),
        message: Snowflake::new(MESSAGE),
        id: Snowflake::new(ATTACHMENT),
    }
}

// The message the attachment and embed tests ask about, loaded into #general.
async fn load_message(online: &Online, edit: impl FnOnce(&mut Value)) {
    let mut message = message(0, MESSAGE)["d"].clone();
    message.as_object_mut().unwrap().remove("nonce");
    edit(&mut message);
    Mock::given(method("GET"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([message])))
        .mount(&online.server)
        .await;
    online
        .account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
}

#[tokio::test]
async fn an_avatar_is_fetched_without_the_token_or_discords_headers() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;

    let file = online.account.image(request(avatar(), 80)).await.unwrap();

    assert_eq!((file.width, file.height), (80, 80));
    assert!(!file.animated);
    assert!(!file.from_disk);
    assert!(file.path.starts_with(online.image_directory()));
    assert_eq!(
        std::fs::read(&file.path).unwrap(),
        webp_bytes(80, 80, false)
    );
    let requests = online.requests_to(AVATAR_PATH).await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url.query(), Some("size=80"));
    for header in [
        "authorization",
        "x-super-properties",
        "x-discord-locale",
        "x-fingerprint",
        "cookie",
    ] {
        assert!(!request.headers.contains_key(header), "{header}");
    }
    let agent = online
        .account
        .shared
        .rest
        .client()
        .properties()
        .browser_user_agent
        .clone();
    assert_eq!(request.headers["user-agent"], agent.as_str());
}

#[tokio::test]
async fn the_second_ask_is_answered_from_disk() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;

    let first = online.account.image(request(avatar(), 80)).await.unwrap();
    let smaller = online.account.image(request(avatar(), 40)).await.unwrap();

    assert!(smaller.from_disk, "the larger file answers a smaller box");
    assert_eq!(smaller.path, first.path);
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 1);
}

#[tokio::test]
async fn waiters_for_the_same_image_share_one_download() {
    let online = connected().await;
    serve(
        &online.server,
        AVATAR_PATH,
        webp(80).set_delay(Duration::from_millis(200)),
    )
    .await;

    let answers =
        futures_util::future::join_all((0..5).map(|_| online.account.image(request(avatar(), 80))))
            .await;

    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 1);
}

#[tokio::test]
async fn the_last_waiter_gone_cancels_the_download() {
    let online = connected().await;
    Mock::given(path(AVATAR_PATH))
        .respond_with(webp(80).set_delay(Duration::from_secs(5)))
        .up_to_n_times(1)
        .mount(&online.server)
        .await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    let account = Arc::new(online.account);
    let first = {
        let account = account.clone();
        tokio::spawn(async move { account.image(request(avatar(), 80)).await })
    };
    while online
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != AVATAR_PATH)
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    first.abort();
    let _ = first.await;
    let second = account.image(request(avatar(), 80)).await;

    assert!(second.is_ok(), "{second:?}");
    let requests: Vec<_> = online
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.url.path() == AVATAR_PATH)
        .collect();
    assert_eq!(requests.len(), 2, "a new download, not the cancelled one");
}

// Notes when each image request arrived.
#[derive(Clone, Default)]
struct Arrivals(Arc<Mutex<Vec<String>>>);

impl Respond for Arrivals {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.lock().unwrap().push(request.url.path().to_owned());
        webp(80).set_delay(Duration::from_millis(500))
    }
}

#[tokio::test]
async fn at_most_eight_downloads_run_at_once() {
    let online = connected().await;
    let arrivals = Arrivals::default();
    Mock::given(method("GET"))
        .respond_with(arrivals.clone())
        .mount(&online.server)
        .await;
    let account = Arc::new(online.account);
    let keys: Vec<ImageKey> = (0..20)
        .map(|n| ImageKey::Avatar {
            user: Snowflake::new(AUTHOR + n),
            hash: ImageHash::parse(HASH),
        })
        .collect();
    let all = {
        let account = account.clone();
        tokio::spawn(async move {
            futures_util::future::join_all(
                keys.into_iter().map(|key| account.image(request(key, 80))),
            )
            .await
        })
    };

    tokio::time::sleep(Duration::from_millis(250)).await;
    let early = arrivals.0.lock().unwrap().len();
    let answers = all.await.unwrap();

    assert!((1..=8).contains(&early), "{early} before any answered");
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(arrivals.0.lock().unwrap().len(), 20);
}

#[tokio::test]
async fn server_errors_are_tried_three_times() {
    let online = connected().await;
    Mock::given(path(AVATAR_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&online.server)
        .await;

    let failed = online.account.image(request(avatar(), 80)).await;

    assert_eq!(failed, Err(ImageError::Unavailable));
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 3);
}

#[tokio::test]
async fn a_server_error_then_success_answers() {
    let online = connected().await;
    Mock::given(path(AVATAR_PATH))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&online.server)
        .await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;

    assert!(online.account.image(request(avatar(), 80)).await.is_ok());
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 3);
}

#[tokio::test]
async fn a_429_pauses_that_host() {
    let online = connected().await;
    let other = "/avatars/100000000000000009/0123456789abcdef0123456789abcdef.webp";
    Mock::given(path(AVATAR_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0.3"))
        .up_to_n_times(1)
        .mount(&online.server)
        .await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    serve(&online.server, other, webp(80)).await;
    let started = std::time::Instant::now();

    let limited = online.account.image(request(avatar(), 80)).await;
    let after = online
        .account
        .image(request(
            ImageKey::Avatar {
                user: Snowflake::new(100_000_000_000_000_009),
                hash: ImageHash::parse(HASH),
            },
            80,
        ))
        .await;

    assert!(limited.is_ok() && after.is_ok());
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "waited out Retry-After"
    );
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 2);
}

#[tokio::test]
async fn a_404_is_remembered_for_the_session() {
    let online = connected().await;
    Mock::given(path(AVATAR_PATH))
        .respond_with(ResponseTemplate::new(404))
        .mount(&online.server)
        .await;

    assert_eq!(
        online.account.image(request(avatar(), 80)).await,
        Err(ImageError::NotFound)
    );
    assert_eq!(
        online.account.image(request(avatar(), 160)).await,
        Err(ImageError::NotFound)
    );
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 1);
}

#[tokio::test]
async fn redirects_are_refused() {
    let online = connected().await;
    Mock::given(path(AVATAR_PATH))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/elsewhere", online.uri())),
        )
        .mount(&online.server)
        .await;
    serve(&online.server, "/elsewhere", webp(80)).await;

    assert!(online.account.image(request(avatar(), 80)).await.is_err());
    assert!(online.requests_to("/elsewhere").await.is_empty());
}

#[tokio::test]
async fn only_images_are_kept() {
    let online = connected().await;
    let page = "/avatars/100000000000000002/0123456789abcdef0123456789abcdef.webp";
    Mock::given(path(page))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html")
                .set_body_bytes(webp_bytes(80, 80, false)),
        )
        .up_to_n_times(1)
        .mount(&online.server)
        .await;
    Mock::given(path(page))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/webp")
                .set_body_bytes(b"<html>not an image</html>".to_vec()),
        )
        .mount(&online.server)
        .await;

    assert_eq!(
        online.account.image(request(avatar(), 80)).await,
        Err(ImageError::NotAnImage)
    );
    assert_eq!(
        online.account.image(request(avatar(), 80)).await,
        Err(ImageError::NotAnImage)
    );
    assert_eq!(
        std::fs::read_dir(online.image_directory().join("files"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn bodies_and_pixels_have_caps() {
    let online = connected().await;
    let huge = "/avatars/100000000000000003/0123456789abcdef0123456789abcdef.webp";
    let mut body = webp_bytes(80, 80, false);
    body.resize(crate::media::MAX_BODY + 1, 0);
    serve(
        &online.server,
        huge,
        ResponseTemplate::new(200)
            .insert_header("content-type", "image/webp")
            .set_body_bytes(body),
    )
    .await;
    serve(
        &online.server,
        AVATAR_PATH,
        ResponseTemplate::new(200)
            .insert_header("content-type", "image/webp")
            .set_body_bytes(webp_bytes(10_000, 10_000, false)),
    )
    .await;

    let too_many_bytes = online
        .account
        .image(request(
            ImageKey::Avatar {
                user: Snowflake::new(100_000_000_000_000_003),
                hash: ImageHash::parse(HASH),
            },
            80,
        ))
        .await;
    let too_many_pixels = online.account.image(request(avatar(), 80)).await;

    assert_eq!(too_many_bytes, Err(ImageError::TooLarge));
    assert_eq!(too_many_pixels, Err(ImageError::TooLarge));
}

#[tokio::test]
async fn offline_answers_from_disk_and_fails_fast_otherwise() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    online.account.image(request(avatar(), 80)).await.unwrap();

    online.account.disconnect();
    let cached = online.account.image(request(avatar(), 80)).await;
    let uncached = online
        .account
        .image(request(ImageKey::DefaultAvatar { index: 3 }, 80))
        .await;

    assert!(cached.unwrap().from_disk);
    assert_eq!(uncached, Err(ImageError::Unavailable));
    assert!(online.requests_to("/embed/avatars/3.png").await.is_empty());
}

#[tokio::test]
async fn a_closed_account_has_no_images() {
    let online = connected().await;

    online.account.close();

    assert_eq!(
        online.account.image(request(avatar(), 80)).await,
        Err(ImageError::Closed)
    );
}

#[tokio::test]
async fn default_avatars_are_the_cdns_pngs() {
    let online = connected().await;
    serve(&online.server, "/embed/avatars/5.png", png(256, 256)).await;

    let file = online
        .account
        .image(request(ImageKey::DefaultAvatar { index: 5 }, 80))
        .await
        .unwrap();
    let again = online
        .account
        .image(request(ImageKey::DefaultAvatar { index: 5 }, 320))
        .await
        .unwrap();

    assert_eq!((file.width, file.height), (256, 256));
    assert!(again.from_disk, "the source's own size answers any box");
    let requests = online.requests_to("/embed/avatars/5.png").await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.query(), None);
    assert_eq!(
        online
            .account
            .image(request(ImageKey::DefaultAvatar { index: 6 }, 80))
            .await,
        Err(ImageError::UnknownKey)
    );
}

#[tokio::test]
async fn an_animation_is_asked_in_the_hosts_form() {
    let online = connected().await;
    let animated = "a_0123456789abcdef0123456789abcdef";
    let gif = format!("/avatars/{AUTHOR}/{animated}.gif");
    let mut body = b"GIF89a".to_vec();
    body.extend([80, 0, 80, 0, 0, 0, 0]);
    for _ in 0..2 {
        body.extend([0x2c, 0, 0, 0, 0, 80, 0, 80, 0, 0, 2, 2, 0x4c, 0x01, 0]);
    }
    body.push(0x3b);
    serve(
        &online.server,
        &gif,
        ResponseTemplate::new(200)
            .insert_header("content-type", "image/gif")
            .set_body_bytes(body),
    )
    .await;

    let file = online
        .account
        .image(ImageRequest {
            key: ImageKey::Avatar {
                user: Snowflake::new(AUTHOR),
                hash: ImageHash::parse(animated),
            },
            width: 80,
            height: 80,
            animated: Some(AnimatedFormat::Gif),
        })
        .await
        .unwrap();

    assert!(file.animated);
    assert_eq!(
        online.requests_to(&gif).await[0].url.query(),
        Some("size=80")
    );
}

#[tokio::test]
async fn an_attachment_comes_from_the_proxy_at_the_boxs_size() {
    let online = connected().await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, fresh)]);
    })
    .await;
    serve(&online.server, &attachment_path(), png(1024, 576)).await;

    let file = online
        .account
        .image(ImageRequest {
            key: attachment_key(),
            width: 800,
            height: 450,
            animated: None,
        })
        .await
        .unwrap();

    assert_eq!((file.width, file.height), (1024, 576));
    let requests = online.requests_to(&attachment_path()).await;
    let query = requests[0].url.query().unwrap().to_owned();
    assert!(query.starts_with(&signature(fresh)), "{query}");
    assert!(
        query.ends_with("format=webp&quality=lossless&width=1024&height=576"),
        "{query}"
    );
    assert!(
        online.requests_to(REFRESH).await.is_empty(),
        "the link wasn't due"
    );
}

#[tokio::test]
async fn an_expiring_link_is_renewed_first() {
    let online = connected().await;
    let stale = now_seconds() + 60;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, stale)]);
    })
    .await;
    let renewed = format!(
        "{}{}?{}&",
        online.uri(),
        attachment_path(),
        signature(fresh)
    );
    Mock::given(method("POST"))
        .and(path(REFRESH))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let original = body["attachment_urls"][0].as_str().unwrap().to_owned();
            ResponseTemplate::new(200).set_body_json(
                json!({"refreshed_urls": [{"original": original, "refreshed": renewed}]}),
            )
        })
        .expect(1)
        .mount(&online.server)
        .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .and(query_param("ex", format!("{fresh:x}")))
        .respond_with(png(1024, 576))
        .mount(&online.server)
        .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .respond_with(ResponseTemplate::new(403))
        .mount(&online.server)
        .await;

    let file = online
        .account
        .image(ImageRequest {
            key: attachment_key(),
            width: 800,
            height: 450,
            animated: None,
        })
        .await;

    assert!(file.is_ok(), "{file:?}");
    let original = online
        .account
        .original_url(&attachment_key())
        .await
        .unwrap();
    assert!(
        original.contains(&format!("ex={fresh:x}")),
        "the renewed link is kept"
    );
}

#[tokio::test]
async fn a_denied_link_past_its_expiry_is_renewed_once() {
    let online = connected().await;
    let past = now_seconds() - 600;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, past)]);
    })
    .await;
    let renewals = Arc::new(Mutex::new(0));
    let counted = renewals.clone();
    let uri = online.uri();
    Mock::given(method("POST"))
        .and(path(REFRESH))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let original = body["attachment_urls"][0].as_str().unwrap().to_owned();
            let mut renewals = counted.lock().unwrap();
            *renewals += 1;
            // The first renewal is past its expiry too, as with a skewed clock.
            let ex = if *renewals == 1 { past + 1 } else { fresh };
            let refreshed = format!("{uri}{}?{}&", attachment_path(), signature(ex));
            ResponseTemplate::new(200).set_body_json(
                json!({"refreshed_urls": [{"original": original, "refreshed": refreshed}]}),
            )
        })
        .mount(&online.server)
        .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .and(query_param("ex", format!("{fresh:x}")))
        .respond_with(png(1024, 576))
        .mount(&online.server)
        .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .respond_with(ResponseTemplate::new(403))
        .mount(&online.server)
        .await;

    let file = online
        .account
        .image(ImageRequest {
            key: attachment_key(),
            width: 800,
            height: 450,
            animated: None,
        })
        .await;

    assert!(file.is_ok(), "{file:?}");
    assert_eq!(*renewals.lock().unwrap(), 2);
}

#[tokio::test]
async fn an_unknown_message_or_one_off_discords_hosts() {
    let online = connected().await;
    assert_eq!(
        online.account.image(request(attachment_key(), 80)).await,
        Err(ImageError::UnknownKey)
    );
    load_message(&online, |message| {
        let mut elsewhere = attachment_json(&online, now_seconds() + 86_400);
        elsewhere["proxy_url"] = "https://example.com/photo.png".into();
        message["attachments"] = json!([elsewhere]);
    })
    .await;

    assert_eq!(
        online.account.image(request(attachment_key(), 80)).await,
        Err(ImageError::Unsupported)
    );
}

#[tokio::test]
async fn an_embed_image_and_its_icons() {
    let online = connected().await;
    let image = format!("{}/external/abc/https/example.com/card.png", online.uri());
    let icon = format!("{}/external/def/https/example.com/icon.png", online.uri());
    load_message(&online, |message| {
        message["embeds"] = json!([{
            "type": "rich",
            "url": "https://example.com/",
            "image": {"url": "https://example.com/card.png", "proxy_url": image, "width": 1200, "height": 630},
            "author": {"name": "Example", "icon_url": "https://example.com/icon.png", "proxy_icon_url": icon},
        }]);
    })
    .await;
    serve(
        &online.server,
        "/external/abc/https/example.com/card.png",
        png(1024, 538),
    )
    .await;
    serve(
        &online.server,
        "/external/def/https/example.com/icon.png",
        png(64, 64),
    )
    .await;
    let embed = |slot| ImageKey::EmbedMedia {
        channel: general(),
        message: Snowflake::new(MESSAGE),
        embed: 0,
        slot,
    };

    let card = online
        .account
        .image(ImageRequest {
            key: embed(EmbedSlot::Image),
            width: 800,
            height: 420,
            animated: None,
        })
        .await
        .unwrap();
    let author = online
        .account
        .image(request(embed(EmbedSlot::AuthorIcon), 48))
        .await
        .unwrap();

    assert_eq!(card.width, 1024);
    assert_eq!(author.width, 64);
    assert_eq!(
        online
            .account
            .image(request(embed(EmbedSlot::FooterIcon), 40))
            .await,
        Err(ImageError::UnknownKey)
    );
    assert_eq!(
        online
            .account
            .original_url(&embed(EmbedSlot::Image))
            .await
            .unwrap(),
        "https://example.com/card.png"
    );
}

// A video embed's poster when it has no thumbnail: the proxy's still of the video, as the
// official client asks it (`format=webp`); the original is Discord's copy, for playback.
#[tokio::test]
async fn an_embeds_video_gives_its_still_and_its_copy() {
    let online = connected().await;
    let video = format!("{}/external/vid/https/example.com/clip.mp4", online.uri());
    load_message(&online, |message| {
        message["embeds"] = json!([{
            "type": "gifv",
            "url": "https://example.com/clip",
            "video": {"url": "https://example.com/clip.mp4", "proxy_url": video, "width": 640, "height": 360},
        }]);
    })
    .await;
    serve(
        &online.server,
        "/external/vid/https/example.com/clip.mp4",
        png(640, 360),
    )
    .await;
    let key = ImageKey::EmbedMedia {
        channel: general(),
        message: Snowflake::new(MESSAGE),
        embed: 0,
        slot: EmbedSlot::Video,
    };

    let still = online
        .account
        .image(ImageRequest {
            key: key.clone(),
            width: 400,
            height: 225,
            animated: None,
        })
        .await
        .unwrap();

    assert_eq!(still.width, 640);
    let asked = online
        .requests_to("/external/vid/https/example.com/clip.mp4")
        .await;
    assert!(
        asked[0]
            .url
            .query()
            .unwrap_or_default()
            .contains("format=webp"),
        "{:?}",
        asked[0].url
    );
    assert_eq!(online.account.original_url(&key).await.unwrap(), video);
}

#[tokio::test]
async fn a_file_gone_from_disk_is_fetched_again() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    let first = online.account.image(request(avatar(), 80)).await.unwrap();
    std::fs::remove_file(&first.path).unwrap();

    let again = online.account.image(request(avatar(), 80)).await.unwrap();

    assert!(!again.from_disk);
    assert!(again.path.exists());
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 2);
}

#[tokio::test]
async fn clearing_images_deletes_them() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    let first = online.account.image(request(avatar(), 80)).await.unwrap();

    online.account.clear_images().await;

    assert!(!first.path.exists());
    assert!(
        !online
            .account
            .image(request(avatar(), 80))
            .await
            .unwrap()
            .from_disk
    );
}

#[tokio::test]
async fn saving_an_original_streams_it() {
    let online = connected().await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, fresh)]);
    })
    .await;
    let original = vec![9_u8; 300_000];
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(original.clone()),
        )
        .mount(&online.server)
        .await;
    let destination = online.dir.path().join("saved.png");

    let written = online
        .account
        .save_original(&attachment_key(), &destination)
        .await
        .unwrap();

    assert_eq!(written, 300_000);
    assert_eq!(std::fs::read(&destination).unwrap(), original);
    let request = &online.requests_to(&attachment_path()).await[0];
    let query = request.url.query().unwrap();
    assert!(
        !query.contains("width") && !query.contains("format"),
        "{query}"
    );
}

#[tokio::test]
async fn forgetting_the_account_deletes_its_images() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    online.account.image(request(avatar(), 80)).await.unwrap();
    let client = online.account.shared.rest.client().clone();

    client.forget_account(Snowflake::new(ME)).await.unwrap();

    assert!(!online.image_directory().exists());
}

// The first avatar is refused once with a 429; notes when each request arrived.
#[derive(Clone, Default)]
struct RateLimited(Arc<Mutex<Vec<(String, std::time::Instant)>>>);

impl Respond for RateLimited {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut arrivals = self.0.lock().unwrap();
        let first = arrivals.is_empty();
        arrivals.push((request.url.path().to_owned(), std::time::Instant::now()));
        if first {
            ResponseTemplate::new(429).insert_header("retry-after", "0.3")
        } else {
            webp(80)
        }
    }
}

#[tokio::test]
async fn a_429_holds_back_other_downloads_from_that_host() {
    let online = connected().await;
    let arrivals = RateLimited::default();
    Mock::given(method("GET"))
        .respond_with(arrivals.clone())
        .mount(&online.server)
        .await;
    let account = Arc::new(online.account);
    let limited = {
        let account = account.clone();
        tokio::spawn(async move { account.image(request(avatar(), 80)).await })
    };
    while arrivals.0.lock().unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;

    let other = account
        .image(request(
            ImageKey::Avatar {
                user: Snowflake::new(100_000_000_000_000_009),
                hash: ImageHash::parse(HASH),
            },
            80,
        ))
        .await;

    assert!(other.is_ok() && limited.await.unwrap().is_ok());
    let arrivals = arrivals.0.lock().unwrap().clone();
    let refused = arrivals[0].1;
    let others: Vec<_> = arrivals[1..].iter().map(|(_, at)| *at - refused).collect();
    assert!(
        others
            .iter()
            .all(|after| *after >= Duration::from_millis(300)),
        "{others:?}"
    );
}

// Answers every request with a chunked body of `bytes`, so no Content-Length tells its size.
async fn chunked_cdn(bytes: usize) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0; 4096];
                let _ = stream.read(&mut request).await;
                let mut body = webp_bytes(80, 80, false);
                body.resize(bytes, 0);
                let mut answer = b"HTTP/1.1 200 OK\r\ncontent-type: image/webp\r\ntransfer-encoding: chunked\r\n\r\n".to_vec();
                for chunk in body.chunks(16 * 1024) {
                    answer.extend(format!("{:x}\r\n", chunk.len()).as_bytes());
                    answer.extend(chunk);
                    answer.extend(b"\r\n");
                }
                answer.extend(b"0\r\n\r\n");
                let _ = stream.write_all(&answer).await;
            });
        }
    });
    format!("http://{address}/")
}

#[tokio::test]
async fn a_body_without_a_length_is_capped_while_it_streams() {
    let online = connected_with(
        Some(chunked_cdn(crate::media::MAX_BODY + 1).await),
        TempDir::new(),
    )
    .await;

    assert_eq!(
        online.account.image(request(avatar(), 80)).await,
        Err(ImageError::TooLarge)
    );
}

#[tokio::test]
async fn a_source_smaller_than_asked_answers_larger_asks() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(128)).await;

    let first = online.account.image(request(avatar(), 160)).await.unwrap();
    let larger = online.account.image(request(avatar(), 320)).await.unwrap();

    assert_eq!(
        first.width, 128,
        "the CDN never answers larger than the source"
    );
    assert!(larger.from_disk, "nothing larger exists");
    assert_eq!(online.requests_to(AVATAR_PATH).await.len(), 1);
}

#[tokio::test]
async fn forgetting_the_account_mid_download_leaves_nothing_behind() {
    let online = connected().await;
    serve(
        &online.server,
        AVATAR_PATH,
        webp(80).set_delay(Duration::from_millis(300)),
    )
    .await;
    let account = Arc::new(online.account);
    let downloading = {
        let account = account.clone();
        tokio::spawn(async move { account.image(request(avatar(), 80)).await })
    };
    while online.server.received_requests().await.unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let client = account.shared.rest.client().clone();

    client.forget_account(Snowflake::new(ME)).await.unwrap();
    let late = downloading.await.unwrap();
    let after = account.image(request(avatar(), 80)).await;

    assert!(late.is_err(), "{late:?}");
    assert_eq!(after, Err(ImageError::Unavailable));
    assert!(!online.dir.path().join(ME.to_string()).exists());
    assert_eq!(
        online
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == AVATAR_PATH)
            .count(),
        1,
        "nothing downloads after the account is forgotten"
    );
}

// The first request is refused with a 429; the next seven hold every slot for a while.
#[derive(Clone, Default)]
struct Crowded(Arc<Mutex<Vec<(String, std::time::Instant)>>>);

impl Respond for Crowded {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut arrivals = self.0.lock().unwrap();
        let first = arrivals.is_empty();
        arrivals.push((request.url.path().to_owned(), std::time::Instant::now()));
        if first {
            // Refused only once every slot is taken.
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0.4")
                .set_delay(Duration::from_millis(200))
        } else if arrivals.len() <= 8 {
            webp(80).set_delay(Duration::from_millis(800))
        } else {
            webp(80)
        }
    }
}

#[tokio::test]
async fn a_429_holds_back_requests_already_waiting_for_a_slot() {
    let online = connected().await;
    let arrivals = Crowded::default();
    Mock::given(method("GET"))
        .respond_with(arrivals.clone())
        .mount(&online.server)
        .await;
    let account = Arc::new(online.account);
    let key = |n: u64| ImageKey::Avatar {
        user: Snowflake::new(AUTHOR + n),
        hash: ImageHash::parse(HASH),
    };
    let mut tasks = Vec::new();
    for n in 0..8 {
        let account = account.clone();
        tasks.push(tokio::spawn(async move {
            account.image(request(key(n), 80)).await
        }));
        while arrivals.0.lock().unwrap().len() <= n as usize {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    // Queued behind the eight; the 429 frees a slot during its own pause.
    let queued = {
        let account = account.clone();
        tokio::spawn(async move { account.image(request(key(20), 80)).await })
    };

    for task in tasks {
        assert!(task.await.unwrap().is_ok());
    }
    assert!(queued.await.unwrap().is_ok());
    let arrivals = arrivals.0.lock().unwrap().clone();
    let paused_until = arrivals[0].1 + Duration::from_millis(600);
    for (path, at) in &arrivals[8..] {
        assert!(*at >= paused_until, "{path} came during the pause");
    }
}

#[tokio::test]
async fn a_link_discord_wont_renew_is_not_found_for_the_session() {
    let online = connected().await;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, now_seconds() - 600)]);
    })
    .await;
    Mock::given(method("POST"))
        .and(path(REFRESH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"refreshed_urls": []})))
        .mount(&online.server)
        .await;
    let ask = || ImageRequest {
        key: attachment_key(),
        width: 800,
        height: 450,
        animated: None,
    };

    assert_eq!(online.account.image(ask()).await, Err(ImageError::NotFound));
    assert_eq!(online.account.image(ask()).await, Err(ImageError::NotFound));
    assert_eq!(online.requests_to(REFRESH).await.len(), 1);
}

#[tokio::test]
async fn images_go_where_ready_says_the_account_is() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let dir = TempDir::new();
    let client =
        client_with_media(fake.url(), &server.uri()).with_image_cache(dir.path().to_path_buf());
    // The host thought it was another account; READY names ME.
    let account = Account::start_as(
        client,
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
        Snowflake::new(100_000_000_000_000_077),
    )
    .unwrap();
    let subscription = account.store().subscribe();
    let _connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    serve(&server, AVATAR_PATH, webp(80)).await;

    let file = account.image(request(avatar(), 80)).await.unwrap();

    assert!(
        file.path.starts_with(dir.path().join(ME.to_string())),
        "{:?}",
        file.path
    );
    assert!(
        !dir.path()
            .join("100000000000000077")
            .join("files")
            .read_dir()
            .is_ok_and(|mut files| files.next().is_some())
    );
}

#[tokio::test]
async fn two_accounts_of_one_user_share_its_directory() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let dir = TempDir::new();
    let client =
        client_with_media(fake.url(), &server.uri()).with_image_cache(dir.path().to_path_buf());
    let start = || {
        Account::start_as(
            client.clone(),
            Token::new(TOKEN.to_owned()),
            timing(),
            DEFAULT_LIMITS,
            Snowflake::new(ME),
        )
        .unwrap()
    };
    let (first, second) = (start(), start());
    let _one = online(&mut fake, &first).await;
    let _two = online(&mut fake, &second).await;
    for account in [&first, &second] {
        while !matches!(account.store().connection(), ConnectionState::Online) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let other = "/avatars/100000000000000009/0123456789abcdef0123456789abcdef.webp";
    serve(
        &server,
        AVATAR_PATH,
        webp(80).set_delay(Duration::from_millis(100)),
    )
    .await;
    serve(
        &server,
        other,
        webp(96).set_delay(Duration::from_millis(100)),
    )
    .await;

    let (mine, theirs) = tokio::join!(
        first.image(request(avatar(), 80)),
        second.image(request(
            ImageKey::Avatar {
                user: Snowflake::new(100_000_000_000_000_009),
                hash: ImageHash::parse(HASH),
            },
            96
        ))
    );

    let (mine, theirs) = (mine.unwrap(), theirs.unwrap());
    assert_eq!((mine.width, theirs.width), (80, 96));
    assert_eq!(
        std::fs::read(&mine.path).unwrap(),
        webp_bytes(80, 80, false)
    );
    assert_eq!(
        std::fs::read(&theirs.path).unwrap(),
        webp_bytes(96, 96, false)
    );
}

#[tokio::test]
async fn forgetting_the_account_before_its_first_ask_keeps_the_directory_gone() {
    let online = connected().await;
    serve(&online.server, AVATAR_PATH, webp(80)).await;
    let client = online.account.shared.rest.client().clone();

    client.forget_account(Snowflake::new(ME)).await.unwrap();
    let after = online.account.image(request(avatar(), 80)).await;

    assert_eq!(after, Err(ImageError::Unavailable));
    assert!(!online.image_directory().exists());
    assert!(
        online.requests_to(AVATAR_PATH).await.is_empty(),
        "nothing downloads for a forgotten account"
    );
}

#[tokio::test]
async fn a_second_account_of_a_forgotten_user_gets_no_images_either() {
    let mut first = connected().await;
    serve(&first.server, AVATAR_PATH, webp(80)).await;
    let client = first.account.shared.rest.client().clone();
    let second = Account::start_as(
        client.clone(),
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
        Snowflake::new(ME),
    )
    .unwrap();
    let _connection = online(&mut first._fake, &second).await;
    while !matches!(second.store().connection(), ConnectionState::Online) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    client.forget_account(Snowflake::new(ME)).await.unwrap();
    let after = second.image(request(avatar(), 80)).await;

    assert_eq!(after, Err(ImageError::Unavailable));
    assert!(!first.image_directory().exists());
}

#[tokio::test]
async fn an_account_opened_again_after_a_forget_has_images() {
    let mut first = connected().await;
    serve(&first.server, AVATAR_PATH, webp(80)).await;
    let client = first.account.shared.rest.client().clone();
    client.forget_account(Snowflake::new(ME)).await.unwrap();

    let reopened = Account::start_as(
        client.clone(),
        Token::new(TOKEN.to_owned()),
        timing(),
        DEFAULT_LIMITS,
        Snowflake::new(ME),
    )
    .unwrap();
    let _connection = online(&mut first._fake, &reopened).await;
    while !matches!(reopened.store().connection(), ConnectionState::Online) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let file = reopened.image(request(avatar(), 80)).await;

    assert!(file.is_ok(), "{file:?}");
    assert!(first.image_directory().exists());
}

#[tokio::test]
async fn clearing_caches_keeps_the_images_of_an_open_account_that_hasnt_asked_yet() {
    let dir = TempDir::new();
    let root = dir.path().to_path_buf();
    {
        let earlier = connected_with(None, dir).await;
        serve(&earlier.server, AVATAR_PATH, webp(80)).await;
        earlier.account.image(request(avatar(), 80)).await.unwrap();
        earlier.account.close();
        // The directory outlives this run: a new TempDir handle takes it over below.
        std::mem::forget(earlier.dir);
    }
    let later = connected_with(None, TempDir::at(root)).await;
    serve(&later.server, AVATAR_PATH, webp(80)).await;
    let client = later.account.shared.rest.client().clone();

    client.clear_caches().await;
    let file = later.account.image(request(avatar(), 80)).await.unwrap();

    assert!(file.from_disk, "an open account's kept images stay");
    assert!(later.requests_to(AVATAR_PATH).await.is_empty());
}

#[tokio::test]
async fn a_proxy_answer_a_pixel_under_the_box_is_not_the_source() {
    let online = connected().await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, fresh)]);
    })
    .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .and(query_param("width", "600"))
        .respond_with(png(599, 337))
        .mount(&online.server)
        .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .and(query_param("width", "1024"))
        .respond_with(png(1024, 576))
        .mount(&online.server)
        .await;
    let ask = |width, height| ImageRequest {
        key: attachment_key(),
        width,
        height,
        animated: None,
    };

    let small = online.account.image(ask(550, 310)).await.unwrap();
    let large = online.account.image(ask(1000, 563)).await.unwrap();

    assert_eq!((small.width, small.height), (599, 337));
    assert!(!large.from_disk, "an answer under the box isn't the source");
    assert_eq!((large.width, large.height), (1024, 576));
}

#[tokio::test]
async fn a_proxy_answer_of_the_sources_size_answers_every_ask() {
    let online = connected().await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, fresh)]);
    })
    .await;
    Mock::given(method("GET"))
        .and(path(attachment_path()))
        .respond_with(png(1920, 1080))
        .mount(&online.server)
        .await;
    let ask = |width, height| ImageRequest {
        key: attachment_key(),
        width,
        height,
        animated: None,
    };

    let first = online.account.image(ask(1500, 844)).await.unwrap();
    let larger = online.account.image(ask(1900, 1069)).await.unwrap();

    assert_eq!((first.width, first.height), (1920, 1080));
    assert!(larger.from_disk, "the source's own size answers every ask");
}

// A CDN that announces 300 000 bytes and sends 1 000, then closes, or holds the connection.
async fn partial_cdn(close: bool) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0; 4096];
                let _ = stream.read(&mut request).await;
                let mut answer =
                    b"HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: 300000\r\n\r\n"
                        .to_vec();
                answer.extend(vec![7_u8; 1000]);
                let _ = stream.write_all(&answer).await;
                let _ = stream.flush().await;
                if !close {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });
        }
    });
    format!("http://{address}/")
}

// A folder with a file the user chose to replace.
fn chosen_file(online: &Online) -> std::path::PathBuf {
    let folder = online.dir.path().join("saves");
    std::fs::create_dir_all(&folder).unwrap();
    let destination = folder.join("photo.png");
    std::fs::write(&destination, b"the old file").unwrap();
    destination
}

fn files_in(folder: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_failed_save_keeps_the_file_that_was_there() {
    let cdn = partial_cdn(true).await;
    let online = connected_with(Some(cdn.clone()), TempDir::new()).await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json_at(&cdn, fresh)]);
    })
    .await;
    let destination = chosen_file(&online);

    let saved = online
        .account
        .save_original(&attachment_key(), &destination)
        .await;

    assert_eq!(saved, Err(ImageError::Unavailable));
    assert_eq!(std::fs::read(&destination).unwrap(), b"the old file");
    assert_eq!(files_in(destination.parent().unwrap()), ["photo.png"]);
}

#[tokio::test]
async fn a_cancelled_save_keeps_the_file_that_was_there() {
    let cdn = partial_cdn(false).await;
    let online = connected_with(Some(cdn.clone()), TempDir::new()).await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json_at(&cdn, fresh)]);
    })
    .await;
    let destination = chosen_file(&online);
    let account = Arc::new(online.account);
    let folder = destination.parent().unwrap().to_path_buf();
    let saving = {
        let account = account.clone();
        let destination = destination.clone();
        tokio::spawn(async move { account.save_original(&attachment_key(), &destination).await })
    };
    // The save is writing once a second file is in the folder.
    tokio::time::timeout(Duration::from_secs(5), async {
        while files_in(&folder).len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    saving.abort();
    let _ = saving.await;

    assert_eq!(std::fs::read(&destination).unwrap(), b"the old file");
    assert_eq!(files_in(&folder), ["photo.png"]);
}

// Notes when each request arrived; images take two seconds, the attachment answers at once.
#[derive(Clone, Default)]
struct SlowImages(Arc<Mutex<Vec<(String, std::time::Instant)>>>);

impl Respond for SlowImages {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let at = request.url.path().to_owned();
        self.0
            .lock()
            .unwrap()
            .push((at.clone(), std::time::Instant::now()));
        if at == attachment_path() {
            png(1920, 1080)
        } else {
            webp(80).set_delay(Duration::from_secs(2))
        }
    }
}

#[tokio::test]
async fn a_save_isnt_held_back_by_the_image_downloads() {
    let online = connected().await;
    let fresh = now_seconds() + 86_400;
    load_message(&online, |message| {
        message["attachments"] = json!([attachment_json(&online, fresh)]);
    })
    .await;
    let arrivals = SlowImages::default();
    Mock::given(method("GET"))
        .respond_with(arrivals.clone())
        .mount(&online.server)
        .await;
    let account = Arc::new(online.account);
    let keys: Vec<ImageKey> = (0..8)
        .map(|n| ImageKey::Avatar {
            user: Snowflake::new(AUTHOR + n),
            hash: ImageHash::parse(HASH),
        })
        .collect();
    let images = {
        let account = account.clone();
        tokio::spawn(async move {
            futures_util::future::join_all(
                keys.into_iter().map(|key| account.image(request(key, 80))),
            )
            .await
        })
    };
    while arrivals.0.lock().unwrap().len() < 8 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let destination = online.dir.path().join("saved.png");

    let saved = account.save_original(&attachment_key(), &destination).await;

    assert!(saved.is_ok(), "{saved:?}");
    let (first_image, save) = {
        let arrivals = arrivals.0.lock().unwrap();
        let first_image = arrivals.iter().map(|(_, at)| *at).min().unwrap();
        let save = arrivals
            .iter()
            .find(|(at, _)| *at == attachment_path())
            .map(|(_, when)| *when)
            .unwrap();
        (first_image, save)
    };
    // No image slot frees before the first image's two seconds are over.
    assert!(
        save < first_image + Duration::from_secs(2),
        "the save waited {:?} for an image slot",
        save - first_image
    );
    assert!(images.await.unwrap().iter().all(Result::is_ok));
}
