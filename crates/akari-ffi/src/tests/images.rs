use std::sync::Arc;
use std::time::Duration;

use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::cache::TempDir;
use super::fake::{FakeGateway, events_until, online};
use super::support::{MemoryStore, USER, core_client, token};
use crate::client::DiscordClient;
use crate::errors::ImageError;
use crate::images::{
    ImageKey, ImageRequest, MediaSize, gallery_columns, media_mosaic, placeholder_image,
};

const AVATAR: &str = "/avatars/100000000000000002/0123456789abcdef0123456789abcdef.webp";

// The smallest WebP header: VP8X with a canvas of 80 × 80.
fn webp() -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    bytes.extend(10u32.to_le_bytes());
    bytes.extend([0x10, 0, 0, 0, 79, 0, 0, 79, 0, 0]);
    bytes
}

fn avatar() -> ImageRequest {
    ImageRequest {
        key: ImageKey::Avatar {
            user_id: akari_core::model::UserId::new(100_000_000_000_000_002),
            hash: "0123456789abcdef0123456789abcdef".to_owned(),
        },
        width: 80,
        height: 80,
        animated: None,
    }
}

struct Online {
    account: Arc<crate::Account>,
    server: MockServer,
    _dir: TempDir,
    _gateway: FakeGateway,
    _ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
}

impl Online {
    async fn downloads(&self) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == AVATAR)
            .count()
    }
}

async fn connected(slow_first: bool) -> Online {
    let gateway = FakeGateway::start().await;
    let server = MockServer::start().await;
    let answer = ResponseTemplate::new(200)
        .insert_header("content-type", "image/webp")
        .set_body_bytes(webp());
    if slow_first {
        Mock::given(path(AVATAR))
            .respond_with(answer.clone().set_delay(Duration::from_secs(10)))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    Mock::given(path(AVATAR))
        .respond_with(answer)
        .mount(&server)
        .await;
    let dir = TempDir::new();
    let endpoints = akari_core::Endpoints {
        cdn: format!("{}/", server.uri()),
        media: format!("{}/", server.uri()),
        ..gateway.endpoints()
    };
    let core =
        core_client(endpoints, Arc::new(MemoryStore::default())).with_image_cache(dir.0.clone());
    let client = DiscordClient::from_core(core).unwrap();
    let account = client.account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    Online {
        account,
        server,
        _dir: dir,
        _gateway: gateway,
        _ws: ws,
    }
}

async fn arrived(online: &Online) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while online.downloads().await == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_pending_image_answers_with_its_file() {
    let online = connected(false).await;

    let file = online
        .account
        .request_image(avatar())
        .result()
        .await
        .unwrap();

    assert_eq!((file.width, file.height), (80, 80));
    assert!(std::path::Path::new(&file.path).exists());
    assert!(!file.from_disk);
    let again = online.account.request_image(avatar());
    assert!(again.result().await.unwrap().from_disk);
    assert_eq!(
        again.result().await,
        Err(ImageError::Cancelled),
        "a result is taken once"
    );
}

#[tokio::test]
async fn cancelling_stops_waiting_and_the_download() {
    let online = connected(true).await;
    let pending = online.account.request_image(avatar());
    arrived(&online).await;

    pending.cancel();
    let cancelled = tokio::time::timeout(Duration::from_secs(1), pending.result()).await;
    let next = online.account.request_image(avatar()).result().await;

    assert_eq!(cancelled.unwrap(), Err(ImageError::Cancelled));
    assert!(next.is_ok(), "{next:?}");
    assert_eq!(
        online.downloads().await,
        2,
        "a new download, not the cancelled one"
    );
}

#[tokio::test]
async fn releasing_a_pending_image_cancels_it() {
    let online = connected(true).await;
    let pending = online.account.request_image(avatar());
    arrived(&online).await;

    drop(pending);
    let next = online.account.request_image(avatar()).result().await;

    assert!(next.is_ok(), "{next:?}");
    assert_eq!(online.downloads().await, 2);
}

#[test]
fn placeholders_and_layout_cross() {
    let placeholder = placeholder_image("6BdS9LIx6ztgCNvPNYoH8Kd5wr5s46fPJx0".to_owned()).unwrap();
    assert_eq!((placeholder.width, placeholder.height), (32, 18));
    assert_eq!(placeholder.rgba.len(), 32 * 18 * 4);
    assert_eq!(placeholder_image("nope".to_owned()), None);

    let square = MediaSize {
        width: 1000,
        height: 1000,
    };
    let mosaic = media_mosaic(vec![square; 4]);
    assert_eq!(mosaic.cells.len(), 4);
    assert_eq!(mosaic.size.width, 550);
    assert_eq!(gallery_columns(1000.0).columns, 2);
}
