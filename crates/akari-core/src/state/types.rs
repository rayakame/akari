use std::fmt;
use std::sync::Arc;

use crate::model::{
    AttachmentId, ChannelId, ChannelType, GenericMarker, GuildId, MessageId, MessageReferenceType,
    MessageType, OverwriteType, Permissions, PremiumType, RoleId, Snowflake, StickerFormatType,
    StickerId, Timestamp, UserId, WebhookId,
};

/// The logged-in user.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CurrentUser {
    pub user: User,
    pub premium_type: PremiumType,
    pub mfa_enabled: bool,
    pub verified: bool,
}

/// Another user, as a DM recipient or a message author.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct User {
    pub id: UserId,
    pub username: Box<str>,
    /// The display name.
    pub global_name: Option<Box<str>>,
    /// 0 for users on the new username system.
    pub discriminator: u16,
    pub avatar: Option<ImageHash>,
    pub bot: bool,
    pub system: bool,
    pub public_flags: u64,
}

impl User {
    /// The global name, or the username if there is none.
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Guild {
    pub id: GuildId,
    pub name: Box<str>,
    pub icon: Option<ImageHash>,
    pub banner: Option<ImageHash>,
    pub owner_id: Option<UserId>,
    /// Including `@everyone`, whose ID is the guild's.
    pub roles: Arc<[Role]>,
    pub member_count: Option<u32>,
    /// Discord sends no live messages for large guilds without a guild subscription,
    /// which Akari doesn't make yet.
    pub large: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Role {
    pub id: RoleId,
    pub name: Box<str>,
    pub position: i32,
    pub permissions: Permissions,
    pub color: u32,
    pub hoist: bool,
}

/// The current user's membership in a guild.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Member {
    pub guild_id: GuildId,
    pub nick: Option<Box<str>>,
    /// Guild-specific avatar.
    pub avatar: Option<ImageHash>,
    /// Without `@everyone`.
    pub roles: Box<[RoleId]>,
    pub joined_at: Option<Timestamp>,
    /// End of a timeout; a time in the past means it is over.
    pub communication_disabled_until: Option<Timestamp>,
    pub flags: u64,
    pub pending: bool,
}

/// A guild channel, category, thread, DM or group DM.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Channel {
    pub id: ChannelId,
    pub kind: ChannelType,
    /// `None` for DMs and group DMs.
    pub guild_id: Option<GuildId>,
    /// A guild channel's category, or a thread's parent channel.
    pub parent_id: Option<ChannelId>,
    /// `None` for DMs.
    pub name: Option<Box<str>>,
    pub position: i32,
    pub topic: Option<Box<str>>,
    pub nsfw: bool,
    /// Slowmode in seconds.
    pub rate_limit_per_user: u32,
    pub permission_overwrites: Box<[PermissionOverwrite]>,
    /// DM and group DM recipients without the current user; see `Store::user`.
    pub recipients: Box<[UserId]>,
    /// Group DM icon.
    pub icon: Option<ImageHash>,
    /// Owner of a group DM or thread.
    pub owner_id: Option<UserId>,
    /// Set for threads.
    pub thread: Option<Box<ThreadInfo>>,
    pub flags: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PermissionOverwrite {
    /// A role ID or a user ID, depending on `kind`.
    pub id: Snowflake<GenericMarker>,
    pub kind: OverwriteType,
    pub allow: Permissions,
    pub deny: Permissions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadInfo {
    pub archived: bool,
    pub locked: bool,
    /// Minutes of inactivity before the thread is archived.
    pub auto_archive_duration: u32,
    pub archive_timestamp: Timestamp,
    pub create_timestamp: Option<Timestamp>,
    pub message_count: Option<u32>,
    pub member_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub kind: MessageType,
    /// As sent with this message. Not a real user when `webhook_id` is set.
    pub author: Arc<User>,
    pub webhook_id: Option<WebhookId>,
    pub content: Box<str>,
    pub timestamp: Timestamp,
    pub edited_timestamp: Option<Timestamp>,
    pub flags: u64,
    pub pinned: bool,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Box<[Arc<User>]>,
    pub mention_roles: Box<[RoleId]>,
    pub attachments: Box<[Attachment]>,
    pub embeds: Box<[Embed]>,
    pub stickers: Box<[Sticker]>,
    /// The source of a reply, pin, crosspost, thread starter or forward.
    pub reference: Option<Box<MessageReference>>,
    /// The referenced message as Discord sent it along; if it is loaded, `Store::message`
    /// has a fresher copy.
    pub referenced_message: ReferencedMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferencedMessage {
    /// Not a reply, or Discord didn't include it.
    NotIncluded,
    Deleted,
    Message(Arc<Message>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MessageReference {
    pub kind: MessageReferenceType,
    pub message_id: Option<MessageId>,
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Attachment {
    pub id: AttachmentId,
    pub filename: Box<str>,
    /// Alt text.
    pub description: Option<Box<str>>,
    pub content_type: Option<Box<str>>,
    /// Bytes.
    pub size: u64,
    pub url: Box<str>,
    pub proxy_url: Box<str>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Embed {
    /// `rich`, `image`, `video`, `gifv`, `article`, `link` and others.
    pub kind: Option<Box<str>>,
    pub title: Option<Box<str>>,
    pub description: Option<Box<str>>,
    pub url: Option<Box<str>>,
    pub timestamp: Option<Timestamp>,
    pub color: Option<u32>,
    pub author: Option<EmbedAuthor>,
    pub provider: Option<EmbedProvider>,
    pub footer: Option<EmbedFooter>,
    pub image: Option<EmbedMedia>,
    pub thumbnail: Option<EmbedMedia>,
    pub video: Option<EmbedMedia>,
    pub fields: Box<[EmbedField]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedMedia {
    pub url: Box<str>,
    pub proxy_url: Option<Box<str>>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedAuthor {
    pub name: Box<str>,
    pub url: Option<Box<str>>,
    pub proxy_icon_url: Option<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedProvider {
    pub name: Option<Box<str>>,
    pub url: Option<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedFooter {
    pub text: Box<str>,
    pub proxy_icon_url: Option<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedField {
    pub name: Box<str>,
    pub value: Box<str>,
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sticker {
    pub id: StickerId,
    pub name: Box<str>,
    pub format: StickerFormatType,
}

/// An avatar, icon or banner hash. The usual 32-hex-digit hashes take 17 bytes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ImageHash(Repr);

#[derive(Clone, PartialEq, Eq, Hash)]
enum Repr {
    Hex { animated: bool, bytes: [u8; 16] },
    Text(Box<str>),
}

const ANIMATED: &str = "a_";

impl ImageHash {
    pub(crate) fn parse(hash: &str) -> Self {
        let (animated, hex) = match hash.strip_prefix(ANIMATED) {
            Some(rest) => (true, rest),
            None => (false, hash),
        };
        match hex_bytes(hex) {
            Some(bytes) => Self(Repr::Hex { animated, bytes }),
            None => Self(Repr::Text(hash.into())),
        }
    }

    /// Animated images (`a_` prefix) are GIFs or animated WebP on the CDN.
    pub fn is_animated(&self) -> bool {
        match &self.0 {
            Repr::Hex { animated, .. } => *animated,
            Repr::Text(text) => text.starts_with(ANIMATED),
        }
    }
}

// Only lowercase: Discord's hashes are, and the text form must round-trip exactly.
fn hex_bytes(hex: &str) -> Option<[u8; 16]> {
    let digits = hex.as_bytes();
    if digits.len() != 32 {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0; 16];
    for (byte, [high, low]) in bytes.iter_mut().zip(digits.as_chunks::<2>().0) {
        *byte = (digit(*high)? << 4) | digit(*low)?;
    }
    Some(bytes)
}

impl fmt::Display for ImageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Hex { animated, bytes } => {
                if *animated {
                    f.write_str(ANIMATED)?;
                }
                bytes.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
            }
            Repr::Text(text) => f.write_str(text),
        }
    }
}

impl fmt::Debug for ImageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageHash({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_hashes_round_trip() {
        let plain = ImageHash::parse("0123456789abcdef0123456789abcdef");
        let animated = ImageHash::parse("a_0123456789abcdef0123456789abcdef");

        assert_eq!(plain.to_string(), "0123456789abcdef0123456789abcdef");
        assert!(!plain.is_animated());
        assert_eq!(animated.to_string(), "a_0123456789abcdef0123456789abcdef");
        assert!(animated.is_animated());
        assert!(size_of::<ImageHash>() <= 24);
        assert_eq!(size_of::<Option<ImageHash>>(), size_of::<ImageHash>());
    }

    #[test]
    fn odd_image_hashes_are_kept_as_text() {
        for hash in [
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcde",
            "a_",
            "embed/avatars/1",
            "",
        ] {
            assert_eq!(ImageHash::parse(hash).to_string(), hash);
        }
        assert!(ImageHash::parse("a_gif").is_animated());
    }

    #[test]
    fn display_name_prefers_the_global_name() {
        let mut user = User {
            id: Snowflake::new(1),
            username: "mira".into(),
            global_name: Some("Mira".into()),
            discriminator: 0,
            avatar: None,
            bot: false,
            system: false,
            public_flags: 0,
        };
        assert_eq!(user.display_name(), "Mira");

        user.global_name = None;
        assert_eq!(user.display_name(), "mira");
    }
}
