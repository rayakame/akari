//! Images from Discord's CDN and media proxy: keys by Discord's IDs, the sizes and forms to ask
//! for, downloads without the token, signed links, and the files kept on disk per account.
//! Decoding and drawing are the host's.

mod disk;
pub mod layout;
mod placeholder;
mod signed;
mod sniff;
pub(crate) mod url;

use std::path::PathBuf;

pub use placeholder::{Placeholder, placeholder};

use crate::model::{
    AttachmentId, ChannelId, EmojiId, GuildId, MessageId, RoleId, StickerFormatType, StickerId,
    UserId,
};
use crate::state::ImageHash;

/// An image by Discord's IDs. A changed avatar or icon has a new hash, so a new key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageKey {
    Avatar {
        user: UserId,
        hash: ImageHash,
    },
    /// A member's avatar for one guild.
    MemberAvatar {
        guild: GuildId,
        user: UserId,
        hash: ImageHash,
    },
    /// One of Discord's six default avatars, 0–5.
    DefaultAvatar {
        index: u8,
    },
    GuildIcon {
        guild: GuildId,
        hash: ImageHash,
    },
    /// A group DM's icon.
    GroupIcon {
        channel: ChannelId,
        hash: ImageHash,
    },
    RoleIcon {
        role: RoleId,
        hash: ImageHash,
    },
    Emoji {
        id: EmojiId,
        animated: bool,
    },
    Sticker {
        id: StickerId,
        format: StickerFormatType,
    },
    /// An attachment of a message the store has; a video's is its poster.
    Attachment {
        channel: ChannelId,
        message: MessageId,
        id: AttachmentId,
    },
    /// An image of a message's embed, by the embed's place in the message.
    EmbedMedia {
        channel: ChannelId,
        message: MessageId,
        embed: u8,
        slot: EmbedSlot,
    },
}

/// Which image of an embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EmbedSlot {
    Image,
    /// Also a video embed's poster.
    Thumbnail,
    AuthorIcon,
    FooterIcon,
}

/// The form an animation is asked in; the host picks what its decoder shows right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimatedFormat {
    WebP,
    Gif,
}

/// What a box shows. `width` and `height` are the box's pixels: its points times the screen's
/// backing scale, rounded up. The file has at least as many pixels, unless the source has
/// fewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRequest {
    pub key: ImageKey,
    pub width: u32,
    pub height: u32,
    /// `None` asks for the still image. An image that can't animate is still either way.
    pub animated: Option<AnimatedFormat>,
}

/// A file on disk with the image. It stays at least a minute after it's handed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    pub path: PathBuf,
    /// The file's pixels.
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    /// Already on disk: nothing was downloaded.
    pub from_disk: bool,
}

/// Why there is no image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// Discord has no such image; asking again this session fails at once.
    #[error("Discord has no such image")]
    NotFound,
    /// Offline, or the network or Discord failed; asking again later can work, e.g. online.
    #[error("the image can't be loaded now")]
    Unavailable,
    /// More bytes or pixels than Akari shows.
    #[error("the image is too large")]
    TooLarge,
    /// The answer isn't a PNG, GIF, WebP or JPEG image.
    #[error("not an image")]
    NotAnImage,
    /// The store doesn't have the key's message or embed, or the key names nothing.
    #[error("unknown image")]
    UnknownKey,
    /// An image Akari doesn't show yet, such as a Lottie sticker.
    #[error("unsupported image")]
    Unsupported,
    /// The file couldn't be written.
    #[error("the image couldn't be written to disk")]
    Io,
    /// The account is closed.
    #[error("the account is closed")]
    Closed,
}
