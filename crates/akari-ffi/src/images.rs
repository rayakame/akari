use std::sync::{Arc, Mutex, PoisonError};

use akari_core::media::{self, AnimatedFormat, layout};
use akari_core::model::{
    AttachmentId, ChannelId, EmojiId, GuildId, MessageId, RoleId, StickerFormatType, StickerId,
    UserId,
};
use akari_core::state::ImageHash;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

use crate::errors::ImageError;

/// An image by Discord's IDs; hashes as Discord sends them (`a_` for animated ones).
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ImageKey {
    Avatar {
        user_id: UserId,
        hash: String,
    },
    /// A member's avatar for one guild.
    MemberAvatar {
        guild_id: GuildId,
        user_id: UserId,
        hash: String,
    },
    /// One of Discord's six default avatars, 0–5.
    DefaultAvatar {
        index: u8,
    },
    GuildIcon {
        guild_id: GuildId,
        hash: String,
    },
    /// A group DM's icon.
    GroupIcon {
        channel_id: ChannelId,
        hash: String,
    },
    RoleIcon {
        role_id: RoleId,
        hash: String,
    },
    Emoji {
        emoji_id: EmojiId,
        animated: bool,
    },
    Sticker {
        sticker_id: StickerId,
        format: StickerFormatType,
    },
    /// An attachment of a message the store has; a video's is its poster.
    Attachment {
        channel_id: ChannelId,
        message_id: MessageId,
        attachment_id: AttachmentId,
    },
    /// An image of a message's embed, by the embed's place in the message.
    EmbedMedia {
        channel_id: ChannelId,
        message_id: MessageId,
        embed: u8,
        slot: EmbedSlot,
    },
}

/// Which image of an embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum EmbedSlot {
    Image,
    /// Also a video embed's poster.
    Thumbnail,
    AuthorIcon,
    FooterIcon,
}

/// A sticker's file format.
#[uniffi::remote(Enum)]
pub enum StickerFormatType {
    Png,
    Apng,
    Lottie,
    Gif,
    Unknown(u16),
}

/// The form an animation is asked in: what the host's decoder shows right.
#[uniffi::remote(Enum)]
pub enum AnimatedFormat {
    WebP,
    Gif,
}

/// What a box shows: `width` and `height` are its pixels (points times the screen's backing
/// scale, rounded up). `animated` is `None` for the still image.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ImageRequest {
    pub key: ImageKey,
    pub width: u32,
    pub height: u32,
    pub animated: Option<AnimatedFormat>,
}

/// A file with the image; it stays at least a minute after it's handed out. `from_disk`: it was
/// kept, nothing was downloaded.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ImageFile {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub from_disk: bool,
}

/// A decoded thumbhash: straight RGBA rows from the top, at most 32 pixels on a side.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlaceholderImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl From<ImageKey> for media::ImageKey {
    fn from(key: ImageKey) -> Self {
        let hash = |text: &str| ImageHash::parse(text);
        match key {
            ImageKey::Avatar { user_id, hash: h } => Self::Avatar {
                user: user_id,
                hash: hash(&h),
            },
            ImageKey::MemberAvatar {
                guild_id,
                user_id,
                hash: h,
            } => Self::MemberAvatar {
                guild: guild_id,
                user: user_id,
                hash: hash(&h),
            },
            ImageKey::DefaultAvatar { index } => Self::DefaultAvatar { index },
            ImageKey::GuildIcon { guild_id, hash: h } => Self::GuildIcon {
                guild: guild_id,
                hash: hash(&h),
            },
            ImageKey::GroupIcon {
                channel_id,
                hash: h,
            } => Self::GroupIcon {
                channel: channel_id,
                hash: hash(&h),
            },
            ImageKey::RoleIcon { role_id, hash: h } => Self::RoleIcon {
                role: role_id,
                hash: hash(&h),
            },
            ImageKey::Emoji { emoji_id, animated } => Self::Emoji {
                id: emoji_id,
                animated,
            },
            ImageKey::Sticker { sticker_id, format } => Self::Sticker {
                id: sticker_id,
                format,
            },
            ImageKey::Attachment {
                channel_id,
                message_id,
                attachment_id,
            } => Self::Attachment {
                channel: channel_id,
                message: message_id,
                id: attachment_id,
            },
            ImageKey::EmbedMedia {
                channel_id,
                message_id,
                embed,
                slot,
            } => Self::EmbedMedia {
                channel: channel_id,
                message: message_id,
                embed,
                slot: match slot {
                    EmbedSlot::Image => media::EmbedSlot::Image,
                    EmbedSlot::Thumbnail => media::EmbedSlot::Thumbnail,
                    EmbedSlot::AuthorIcon => media::EmbedSlot::AuthorIcon,
                    EmbedSlot::FooterIcon => media::EmbedSlot::FooterIcon,
                },
            },
        }
    }
}

impl From<ImageRequest> for media::ImageRequest {
    fn from(request: ImageRequest) -> Self {
        Self {
            key: request.key.into(),
            width: request.width,
            height: request.height,
            animated: request.animated,
        }
    }
}

impl From<media::ImageFile> for ImageFile {
    fn from(file: media::ImageFile) -> Self {
        Self {
            path: file.path.to_string_lossy().into_owned(),
            width: file.width,
            height: file.height,
            animated: file.animated,
            from_disk: file.from_disk,
        }
    }
}

/// An image on its way. `result` waits for it once; `cancel`, or releasing this object, stops
/// waiting, and the download stops when nobody else waits for it. Swift doesn't cancel a Rust
/// call when its task is cancelled; call `cancel` from the task's cancellation handler.
#[derive(uniffi::Object)]
pub struct PendingImage {
    answer: Mutex<Option<oneshot::Receiver<Result<media::ImageFile, media::ImageError>>>>,
    task: AbortHandle,
}

impl PendingImage {
    pub(crate) fn start(
        account: Arc<akari_core::Account>,
        request: media::ImageRequest,
        runtime: &tokio::runtime::Runtime,
    ) -> Arc<Self> {
        let (sender, answer) = oneshot::channel();
        let task = runtime.spawn(async move {
            let _ = sender.send(account.image(request).await);
        });
        Arc::new(Self {
            answer: Mutex::new(Some(answer)),
            task: task.abort_handle(),
        })
    }
}

#[uniffi::export]
impl PendingImage {
    /// The image, or why there is none; `Cancelled` after `cancel` or a second call.
    pub async fn result(&self) -> Result<ImageFile, ImageError> {
        let answer = self
            .answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(ImageError::Cancelled)?;
        match answer.await {
            Ok(answer) => answer.map(Into::into).map_err(Into::into),
            Err(_) => Err(ImageError::Cancelled),
        }
    }

    /// Stops waiting; `result` then returns `Cancelled`.
    pub fn cancel(&self) {
        self.task.abort();
    }
}

impl Drop for PendingImage {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Decodes an attachment's `placeholder` as Discord's client does; `None` when it isn't a
/// thumbhash.
#[uniffi::export]
pub fn placeholder_image(thumbhash: String) -> Option<PlaceholderImage> {
    media::placeholder(&thumbhash).map(|placeholder| PlaceholderImage {
        width: placeholder.width,
        height: placeholder.height,
        rgba: placeholder.rgba,
    })
}

/// Points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MediaSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MediaRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A message's visual attachments: each cell's place in a box of `size`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaMosaic {
    pub size: MediaSize,
    pub cells: Vec<MediaRect>,
}

/// The forum gallery's columns.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct GalleryColumns {
    pub columns: u32,
    pub column_width: f64,
}

impl From<MediaSize> for layout::Size {
    fn from(size: MediaSize) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

impl From<layout::Size> for MediaSize {
    fn from(size: layout::Size) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

impl From<layout::Mosaic> for MediaMosaic {
    fn from(mosaic: layout::Mosaic) -> Self {
        Self {
            size: mosaic.size.into(),
            cells: mosaic
                .cells
                .into_iter()
                .map(|cell| MediaRect {
                    x: cell.x,
                    y: cell.y,
                    width: cell.width,
                    height: cell.height,
                })
                .collect(),
        }
    }
}

impl From<MediaMosaic> for layout::Mosaic {
    fn from(mosaic: MediaMosaic) -> Self {
        Self {
            size: mosaic.size.into(),
            cells: mosaic
                .cells
                .into_iter()
                .map(|cell| layout::Rect {
                    x: cell.x,
                    y: cell.y,
                    width: cell.width,
                    height: cell.height,
                })
                .collect(),
        }
    }
}

/// `source` scaled into `max`, never up.
#[uniffi::export]
pub fn media_contain(source: MediaSize, max: MediaSize) -> MediaSize {
    layout::contain(source.into(), max.into()).into()
}

/// The size `source` is drawn at to fill `cell`, never up; the overflow is cropped.
#[uniffi::export]
pub fn media_cover(source: MediaSize, cell: MediaSize) -> MediaSize {
    layout::cover(source.into(), cell.into()).into()
}

/// One attachment alone: contained in 550 × 350; a video at least 150 × 110.
#[uniffi::export]
pub fn media_box(source: MediaSize, video: bool) -> MediaSize {
    layout::media_box(source.into(), video).into()
}

/// The cells of up to ten visual attachments at 550 points.
#[uniffi::export]
pub fn media_mosaic(items: Vec<MediaSize>) -> MediaMosaic {
    let items: Vec<layout::Size> = items.into_iter().map(Into::into).collect();
    layout::mosaic(&items).into()
}

/// `mosaic` scaled down as a whole to `width` points; never up.
#[uniffi::export]
pub fn media_fit_width(mosaic: MediaMosaic, width: u32) -> MediaMosaic {
    layout::fit_width(&mosaic.into(), width).into()
}

/// A sticker's box.
#[uniffi::export]
pub fn sticker_size() -> MediaSize {
    layout::STICKER.into()
}

/// An embed card's image: contained in 400 × 300.
#[uniffi::export]
pub fn embed_image_size(source: MediaSize) -> MediaSize {
    layout::embed_image(source.into()).into()
}

/// An embed card's thumbnail: contained in 80 × 80.
#[uniffi::export]
pub fn embed_thumbnail_size(source: MediaSize) -> MediaSize {
    layout::embed_thumbnail(source.into()).into()
}

/// The forum gallery's columns for a page `width` points wide.
#[uniffi::export]
pub fn gallery_columns(width: f64) -> GalleryColumns {
    let gallery = layout::gallery_columns(width);
    GalleryColumns {
        columns: gallery.columns,
        column_width: gallery.column_width,
    }
}

/// A gallery card's height for its column's width; `ratio` is the media's height over its
/// width (2/3 in forums, 9/16 in media channels).
#[uniffi::export]
pub fn gallery_card_height(column_width: f64, ratio: f64) -> f64 {
    layout::gallery_card_height(column_width, ratio)
}
