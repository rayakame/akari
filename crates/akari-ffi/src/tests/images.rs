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
    EmbedSlot, ImageKey, ImageRequest, MediaSize, Pending, gallery_columns, media_mosaic,
    placeholder_image,
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

// The fixture: the current user has no avatar, recipient 2 has one, the guild an icon, the
// group DM none.
#[tokio::test]
async fn records_carry_their_image_keys() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(
        store.current_user().unwrap().avatar,
        ImageKey::DefaultAvatar { index: 0 }
    );
    let ren = store.user(akari_core::model::UserId::new(100_000_000_000_000_002));
    assert_eq!(
        ren.unwrap().avatar,
        ImageKey::Avatar {
            user_id: akari_core::model::UserId::new(100_000_000_000_000_002),
            hash: "0123456789abcdef0123456789abcdef".to_owned(),
        }
    );
    assert_eq!(
        store.guild(super::fake::GUILD).unwrap().icon,
        Some(ImageKey::GuildIcon {
            guild_id: super::fake::GUILD,
            hash: "00000000000000000000000000000001".to_owned(),
        })
    );
    let group = store.channel(akari_core::model::ChannelId::new(300_000_000_000_000_011));
    assert_eq!(group.unwrap().icon, None);
    account.close();
}

#[test]
fn every_image_key_round_trips() {
    let keys = [
        ImageKey::Avatar {
            user_id: USER,
            hash: "0123456789abcdef0123456789abcdef".to_owned(),
        },
        ImageKey::MemberAvatar {
            guild_id: super::fake::GUILD,
            user_id: USER,
            hash: "a_0123456789abcdef0123456789abcdef".to_owned(),
        },
        ImageKey::DefaultAvatar { index: 3 },
        ImageKey::GuildIcon {
            guild_id: super::fake::GUILD,
            hash: "00000000000000000000000000000001".to_owned(),
        },
        ImageKey::GroupIcon {
            channel_id: akari_core::model::ChannelId::new(300_000_000_000_000_011),
            hash: "00000000000000000000000000000002".to_owned(),
        },
        ImageKey::RoleIcon {
            role_id: akari_core::model::RoleId::new(500_000_000_000_000_002),
            hash: "00000000000000000000000000000003".to_owned(),
        },
        ImageKey::Emoji {
            emoji_id: akari_core::model::EmojiId::new(600_000_000_000_000_001),
            animated: true,
        },
        ImageKey::Sticker {
            sticker_id: akari_core::model::StickerId::new(700_000_000_000_000_001),
            format: akari_core::model::StickerFormatType::Lottie,
        },
        ImageKey::Attachment {
            channel_id: akari_core::model::ChannelId::new(300_000_000_000_000_002),
            message_id: akari_core::model::MessageId::new(400_000_000_000_000_001),
            attachment_id: akari_core::model::AttachmentId::new(800_000_000_000_000_001),
        },
    ];
    for key in keys {
        let core: akari_core::media::ImageKey = key.clone().into();
        assert_eq!(ImageKey::from(core), key);
    }
    for slot in [
        EmbedSlot::Image,
        EmbedSlot::Thumbnail,
        EmbedSlot::AuthorIcon,
        EmbedSlot::FooterIcon,
    ] {
        let key = ImageKey::EmbedMedia {
            channel_id: akari_core::model::ChannelId::new(300_000_000_000_000_002),
            message_id: akari_core::model::MessageId::new(400_000_000_000_000_001),
            embed: 2,
            slot,
        };
        let core: akari_core::media::ImageKey = key.clone().into();
        assert_eq!(ImageKey::from(core), key);
    }
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
        again.result().await,
        "every result() call gets the same answer"
    );
}

#[tokio::test]
async fn concurrent_results_share_one_answer() {
    let online = connected(false).await;
    let pending = online.account.request_image(avatar());

    let (first, second) = tokio::join!(pending.result(), pending.result());

    assert!(first.is_ok(), "{first:?}");
    assert_eq!(first, second);
    assert_eq!(online.downloads().await, 1);
}

#[tokio::test]
async fn a_panic_in_the_core_is_a_failure_not_a_cancel() {
    let runtime = crate::runtime::runtime().unwrap();
    let pending: Pending<u64> = Pending::start(runtime, async { panic!("a bug") });

    assert_eq!(pending.result().await, Err(ImageError::Failed));
}

#[tokio::test]
async fn a_cancel_before_the_answer_is_cancelled_and_after_it_changes_nothing() {
    let runtime = crate::runtime::runtime().unwrap();
    let waiting: Pending<u64> = Pending::start(runtime, async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(1)
    });
    let answered: Pending<u64> = Pending::start(runtime, async { Ok(2) });
    assert_eq!(answered.result().await, Ok(2));

    waiting.cancel();
    answered.cancel();

    assert_eq!(waiting.result().await, Err(ImageError::Cancelled));
    assert_eq!(answered.result().await, Ok(2));
}

#[tokio::test]
async fn a_cancelled_save_leaves_the_destination_as_it_was() {
    let online = connected(false).await;
    let original = "/avatars/100000000000000002/0123456789abcdef0123456789abcdef.png";
    Mock::given(path(original))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(webp())
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&online.server)
        .await;
    let destination = online._dir.0.join("saved.png");
    std::fs::write(&destination, b"the old file").unwrap();
    let key = ImageKey::Avatar {
        user_id: akari_core::model::UserId::new(100_000_000_000_000_002),
        hash: "0123456789abcdef0123456789abcdef".to_owned(),
    };
    let saving = online
        .account
        .save_original(key, destination.to_string_lossy().into_owned());
    tokio::time::timeout(Duration::from_secs(5), async {
        while online.server.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    saving.cancel();
    let cancelled = tokio::time::timeout(Duration::from_secs(1), saving.result()).await;

    assert_eq!(cancelled.unwrap(), Err(ImageError::Cancelled));
    assert_eq!(std::fs::read(&destination).unwrap(), b"the old file");
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

// Only a request nobody waits for: a running `result()` holds the object.
#[tokio::test]
async fn releasing_a_pending_image_nobody_waits_for_cancels_it() {
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
