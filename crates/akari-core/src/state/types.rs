use std::fmt;
use std::sync::Arc;

use super::settings::NotificationLevel;
use crate::media::ImageKey;
use crate::model::{
    AttachmentId, ChannelId, ChannelType, EmojiId, GenericMarker, GuildId, MessageId,
    MessageReferenceType, MessageType, OverwriteType, Permissions, PremiumType, RoleId, Snowflake,
    StickerFormatType, StickerId, TagId, Timestamp, UserId, WebhookId,
};

/// The logged-in user.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CurrentUser {
    pub user: User,
    pub premium_type: PremiumType,
    pub mfa_enabled: bool,
    pub verified: bool,
    /// `Some(false)` for a user Discord knows is under 18; `None` while it doesn't know.
    pub nsfw_allowed: Option<bool>,
}

/// Another user, as a DM recipient or a message author.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct User {
    pub id: UserId,
    pub username: Box<str>,
    /// The display name.
    pub global_name: Option<Box<str>>,
    pub discriminator: Discriminator,
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

    /// Which of Discord's six default avatars the user shows without one, in the client's
    /// order: a bot with "0000" and no avatar gets 0 (webhooks); a legacy tag gives
    /// `tag % 5`; otherwise the ID's timestamp bits give `(id >> 22) % 6`.
    pub fn default_avatar_index(&self) -> u8 {
        if self.bot && self.discriminator == Discriminator::Zeros && self.avatar.is_none() {
            return 0;
        }
        match self.discriminator {
            Discriminator::Tag(tag) => (tag % 5) as u8,
            Discriminator::Migrated | Discriminator::Zeros => ((self.id.get() >> 22) % 6) as u8,
        }
    }

    /// The image every view asks for: the avatar, or the default avatar without one. A hash
    /// that isn't the CDN's (the client's bundled bot avatars) counts as none.
    pub fn avatar_key(&self) -> ImageKey {
        match &self.avatar {
            Some(hash) if hash.is_cdn_hash() => ImageKey::Avatar {
                user: self.id,
                hash: hash.clone(),
            },
            _ => ImageKey::DefaultAvatar {
                index: self.default_avatar_index(),
            },
        }
    }
}

/// The four digits after a user's name, as Discord sends them: "0" for the username system,
/// "0000" for webhooks and some bots, a tag for legacy users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Discriminator {
    /// "0": a user on the new username system.
    Migrated,
    /// "0000": webhooks and crossposts, some bots.
    Zeros,
    /// A legacy tag, 1–9999.
    Tag(u16),
}

impl Discriminator {
    /// Anything else (junk, out of range) reads as `Migrated`.
    pub fn parse(text: &str) -> Self {
        if text == "0000" {
            return Self::Zeros;
        }
        match text.parse::<u16>() {
            Ok(tag) if (1..=9999).contains(&tag) => Self::Tag(tag),
            _ => Self::Migrated,
        }
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
    /// The level for members who set none.
    pub default_message_notifications: NotificationLevel,
    /// A community server, where the user can choose its channels ("Channels & Roles").
    pub community: bool,
    /// Its custom emoji, sorted: which server an emoji in a message is from.
    pub emojis: Box<[EmojiId]>,
    /// Its owner marked it explicit or age-restricted: every channel is behind the age gate.
    pub age_restricted: bool,
}

impl Guild {
    /// Equal in all a host shows: its emoji list aside, which nothing shows.
    pub(crate) fn shows_the_same(&self, other: &Self) -> bool {
        let Self {
            id,
            name,
            icon,
            banner,
            owner_id,
            roles,
            member_count,
            large,
            default_message_notifications,
            community,
            emojis: _,
            age_restricted,
        } = self;
        *id == other.id
            && *name == other.name
            && *icon == other.icon
            && *banner == other.banner
            && *owner_id == other.owner_id
            && *roles == other.roles
            && *member_count == other.member_count
            && *large == other.large
            && *default_message_notifications == other.default_message_notifications
            && *community == other.community
            && *age_restricted == other.age_restricted
    }

    /// The icon's image; `None` without one (the app shows initials).
    pub fn icon_key(&self) -> Option<ImageKey> {
        let hash = self.icon.as_ref().filter(|hash| hash.is_cdn_hash())?;
        Some(ImageKey::GuildIcon {
            guild: self.id,
            hash: hash.clone(),
        })
    }
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
    /// Minutes of inactivity the official client preselects for a new thread here; `None` when
    /// Discord didn't say.
    pub default_auto_archive_duration: Option<u32>,
    /// The newest message. Forum and media channels keep their newest post's thread ID here,
    /// so it isn't a `MessageId`. It may be deleted already, and it never moves back.
    pub last_message_id: Option<Snowflake<GenericMarker>>,
    pub permission_overwrites: Box<[PermissionOverwrite]>,
    /// DM and group DM recipients without the current user; see `Store::user`.
    pub recipients: Box<[UserId]>,
    /// Group DM icon.
    pub icon: Option<ImageHash>,
    /// Owner of a group DM or thread.
    pub owner_id: Option<UserId>,
    /// Set for threads.
    pub thread: Option<Box<ThreadInfo>>,
    /// Set for forum and media channels.
    pub forum: Option<Box<ForumSettings>>,
    pub flags: u64,
    /// A DM from someone the user hasn't accepted yet. The official client lists neither
    /// these nor `spam` DMs.
    pub message_request: bool,
    pub spam: bool,
}

/// How the forum's page shows its posts: the user's choice, the channel's defaults until then.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ForumView {
    pub sort: ForumSort,
    /// Only posts with these tags; none for every post.
    pub tags: Box<[TagId]>,
    pub matching: TagMatching,
}

impl ForumView {
    pub fn new(sort: ForumSort, tags: Box<[TagId]>, matching: TagMatching) -> Self {
        Self {
            sort,
            tags,
            matching,
        }
    }
}

/// A forum's page: see [`Store::forum_posts`](super::Store::forum_posts).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ForumPosts {
    /// Active posts, pinned first, then by the view's sort. While the forum is open, who's listed
    /// and in what order changes only when the list refreshes, a post is deleted or pinned.
    pub active: Vec<Arc<Channel>>,
    /// Archived posts down to the oldest the loaded pages brought, by the view's sort.
    pub archived: Vec<Arc<Channel>>,
    pub older: OlderThreads,
    /// Others' posts created since the list last refreshed.
    pub new_posts: u32,
}

/// A forum post's first message, for its card: see [`Store::post_preview`](super::Store::post_preview).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostPreview {
    Loaded(Arc<Message>),
    /// Discord says the first message is gone.
    Deleted,
}

/// What a forum or media channel sets for its posts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ForumSettings {
    pub tags: Box<[ForumTag]>,
    /// The reaction a post's card offers before it has any.
    pub default_reaction: Option<TagEmoji>,
    /// The post list's order until the user picks another.
    pub sort: ForumSort,
    /// The post list's view until the user picks another.
    pub layout: ForumLayout,
    /// How a filter of several tags matches until the user picks another.
    pub tag_matching: TagMatching,
    /// Slowmode in seconds for the messages of a new post.
    pub post_slowmode: u32,
    /// What a new post's message starts with.
    pub template: Option<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ForumTag {
    pub id: TagId,
    pub name: Box<str>,
    /// Only members who can manage threads apply it.
    pub moderated: bool,
    pub emoji: Option<TagEmoji>,
}

/// A tag's or a default reaction's emoji.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TagEmoji {
    Custom(EmojiId),
    Unicode(Box<str>),
}

impl TagEmoji {
    /// A custom emoji's image, still: Discord's pills don't animate.
    pub fn image_key(&self) -> Option<ImageKey> {
        match self {
            Self::Custom(id) => Some(ImageKey::Emoji {
                id: *id,
                animated: false,
            }),
            Self::Unicode(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ForumSort {
    /// The newest message first.
    #[default]
    LatestActivity,
    /// The newest post first.
    CreationDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ForumLayout {
    #[default]
    List,
    Gallery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TagMatching {
    /// A post with any of the tags.
    #[default]
    Some,
    /// A post with all of them.
    All,
}

/// Which threads the user may create in a channel, see `Store::thread_creation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ThreadCreation {
    pub public: bool,
    /// Only in text channels.
    pub private: bool,
}

// A forum post pinned to the top of its forum.
const PINNED: u64 = 1 << 1;

impl Channel {
    /// A group DM's icon; `None` without one.
    pub fn icon_key(&self) -> Option<ImageKey> {
        let hash = self.icon.as_ref().filter(|hash| hash.is_cdn_hash())?;
        Some(ImageKey::GroupIcon {
            channel: self.id,
            hash: hash.clone(),
        })
    }

    /// When a thread's "Hide After Inactivity" window ends: `auto_archive_duration` minutes after
    /// its newest message or its archive timestamp, whichever is later. `None` for pinned forum
    /// posts, which have none, and for channels that aren't threads.
    pub fn window_end(&self) -> Option<Timestamp> {
        let info = self.thread.as_deref()?;
        if self.is_pinned() {
            return None;
        }
        let newest = self.last_message_id.map_or(0, Snowflake::unix_millis);
        let start = newest.max(info.archive_timestamp.unix_millis());
        let minutes = i64::from(info.auto_archive_duration);
        Some(Timestamp::from_unix_millis(
            start.saturating_add(minutes * 60_000),
        ))
    }

    /// Announcement, public and private threads.
    pub fn is_thread(&self) -> bool {
        matches!(
            self.kind,
            ChannelType::NewsThread | ChannelType::PublicThread | ChannelType::PrivateThread
        )
    }

    /// A forum post pinned to the top of its forum.
    pub fn is_pinned(&self) -> bool {
        self.flags & PINNED != 0
    }

    /// Forum and media channels, whose page lists their posts.
    pub fn is_forum(&self) -> bool {
        matches!(self.kind, ChannelType::GuildForum | ChannelType::GuildMedia)
    }
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
    /// The current user's membership; `None` while the user hasn't joined the thread.
    pub member: Option<ThreadMembership>,
    pub archived: bool,
    pub locked: bool,
    /// Minutes of inactivity before the thread is archived.
    pub auto_archive_duration: u32,
    pub archive_timestamp: Timestamp,
    pub create_timestamp: Option<Timestamp>,
    pub message_count: Option<u32>,
    pub member_count: Option<u32>,
    /// Some of its members, as Discord picks them; not kept in the disk cache.
    pub member_ids_preview: Box<[UserId]>,
    /// A forum post's tags.
    pub applied_tags: Box<[TagId]>,
    /// Every message ever sent in it, deleted ones included.
    pub total_message_sent: Option<u32>,
}

impl ThreadInfo {
    /// Only joined threads are listed and count toward their guild's badge.
    pub fn joined(&self) -> bool {
        self.member.is_some()
    }
}

/// What a channel's Threads browser lists: see [`Store::thread_browser`](super::Store::thread_browser).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadBrowser {
    /// Active threads the user joined, the newest message first.
    pub joined: Vec<Arc<Channel>>,
    /// Active threads the user hasn't joined, the newest message first.
    pub others: Vec<Arc<Channel>>,
    /// Archived threads, the newest message first, down to the oldest one the loaded pages
    /// brought; all of them once Discord said there are no more.
    pub archived: Vec<Arc<Channel>>,
    pub older: OlderThreads,
}

/// Where loading a channel's archived threads stands in this connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OlderThreads {
    #[default]
    NotLoaded,
    Loading,
    /// Discord has more after the loaded pages.
    More,
    All,
    /// A page failed, or Discord hadn't indexed the channel: nothing more until the next
    /// connection.
    Failed,
}

/// The current user's membership of a thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadMembership {
    /// Muted by its own settings; see [`muted_now`](Self::muted_now).
    pub muted: bool,
    /// `None` for a mute without an end.
    pub mute_ends: Option<Timestamp>,
    /// The thread's own notification level; `None` follows its channel. No badge depends on it.
    pub level: Option<NotificationLevel>,
    /// When the user last joined the thread, which orders the list; `None` if Discord didn't say.
    pub joined_at: Option<Timestamp>,
}

impl ThreadMembership {
    pub fn muted_now(&self, now: Timestamp) -> bool {
        self.muted && self.mute_ends.is_none_or(|end| end > now)
    }
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
    /// `Pending` and `Failed` messages are ours, waiting in the window's outbox.
    pub delivery: Delivery,
    /// What a system message's line names beyond its author and content; `None` for most.
    pub system: Option<Box<SystemData>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SystemData {
    /// On a CALL or VOICE_SESSION message.
    pub call: Option<Call>,
    /// On a ROLE_SUBSCRIPTION_PURCHASE message.
    pub role_subscription: Option<RoleSubscription>,
    /// On a PURCHASE_NOTIFICATION message: the guild product's name.
    pub purchased_product: Option<Box<str>>,
    /// Who added a bot, on the bot's USER_JOIN.
    pub actor: Option<Arc<User>>,
    /// The app a GUILD_APPLICATION_PREMIUM_SUBSCRIPTION upgraded.
    pub application_name: Option<Box<str>>,
}

impl SystemData {
    // Boxed only when there's something to keep.
    pub(crate) fn boxed(self) -> Option<Box<Self>> {
        (self != Self::default()).then(|| Box::new(self))
    }
}

/// The call a CALL message records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Call {
    /// The recipients who joined it.
    pub participants: Box<[UserId]>,
    /// `None` while it runs.
    pub ended_timestamp: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoleSubscription {
    pub tier_name: Box<str>,
    pub total_months_subscribed: u32,
    pub is_renewal: bool,
}

impl Message {
    /// Discord's `IS_COMPONENTS_V2` message flag.
    pub const IS_COMPONENTS_V2: u64 = 1 << 15;
    /// Discord's `HAS_THREAD` message flag.
    pub const HAS_THREAD: u64 = 1 << 5;

    /// A thread was started from this message, or it is the line about a thread started without
    /// one; either way the thread has this message's ID.
    pub fn has_thread(&self) -> bool {
        self.flags & Self::HAS_THREAD != 0
    }

    /// Laid out with Components V2: the message has no content and is made of its components,
    /// which Akari doesn't read yet.
    pub fn uses_components_v2(&self) -> bool {
        self.flags & Self::IS_COMPONENTS_V2 != 0
    }

    /// The fields of the first embed of a poll result or an AutoMod alert, which their lines
    /// read; empty for other kinds.
    pub fn system_embed_fields(&self) -> &[EmbedField] {
        match self.kind {
            MessageType::PollResult | MessageType::AutoModerationAction => {
                self.embeds.first().map_or(&[], |embed| &embed.fields)
            }
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delivery {
    Sent,
    /// Sent to Discord, not confirmed yet. The ID is provisional (the nonce).
    Pending,
    /// Discord didn't take it; retry or discard it.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferencedMessage {
    /// Not a reply, or Discord didn't include it.
    NotIncluded,
    Deleted,
    Message(Arc<Message>),
}

/// A thread's first message when it started from a message in its parent channel.
// Exhaustive, so a new case can't reach the apps as one of these unnoticed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StarterMessage {
    /// The parent channel's message the thread started from.
    Loaded(Arc<Message>),
    /// Akari is fetching it; nothing to show yet.
    Loading,
    /// Deleted, or Akari can't read it.
    Unavailable,
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
    /// The name without its extension, when Discord had to change the file's name.
    pub title: Option<Box<str>>,
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
    /// A thumbhash to show while the image loads (Discord's placeholder version 1).
    pub placeholder: Option<Box<str>>,
    /// The explicit content scan that looked at it; `None` until one did, 0 when none can.
    pub content_scan_version: Option<u32>,
}

/// What an attachment is, by its file name's extension as the official client decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AttachmentKind {
    Image,
    Video,
    Audio,
    /// Anything else, SVGs too: a file row.
    File,
}

const IS_THUMBNAIL: u64 = 1 << 1;
const IS_SPOILER: u64 = 1 << 3;
const IS_ANIMATED: u64 = 1 << 5;

impl Attachment {
    pub fn kind(&self) -> AttachmentKind {
        AttachmentKind::of(&self.extension())
    }

    /// An image or video the message shows as media: its size known, and not a 1×1 pixel.
    pub fn is_visual(&self) -> bool {
        let (Some(width), Some(height)) = (self.width, self.height) else {
            return false;
        };
        matches!(self.kind(), AttachmentKind::Image | AttachmentKind::Video)
            && width > 0
            && height > 0
            && (width, height) != (1, 1)
    }

    pub fn is_spoiler(&self) -> bool {
        self.flags & IS_SPOILER != 0
    }

    /// A media post's chosen tile; the message itself doesn't show it.
    pub fn is_thumbnail(&self) -> bool {
        self.flags & IS_THUMBNAIL != 0
    }

    /// A GIF, or a WebP or AVIF Discord marks as animated.
    pub fn is_animated(&self) -> bool {
        match self.extension().as_str() {
            "gif" => true,
            "webp" | "avif" => self.flags & IS_ANIMATED != 0,
            _ => false,
        }
    }

    pub(crate) fn extension(&self) -> String {
        self.filename
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default()
    }
}

impl AttachmentKind {
    // Module 127038's lists; videos also need the proxy, which every attachment has.
    pub(crate) fn of(extension: &str) -> Self {
        match extension {
            "png" | "jpg" | "jpeg" | "jfif" | "webp" | "gif" | "heic" | "heif" | "dng" | "avif" => {
                Self::Image
            }
            "mp4" | "webm" | "mov" | "qt" => Self::Video,
            "mp3" | "m4a" | "ogg" | "opus" | "wav" | "flac" => Self::Audio,
            _ => Self::File,
        }
    }
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
    /// 16 explicit, 32 a content inventory entry, 64 gore, 128 self-harm.
    pub flags: u64,
    /// The explicit content scan that looked at it; `None` until one did.
    pub content_scan_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmbedMedia {
    pub url: Box<str>,
    pub proxy_url: Option<Box<str>>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// 32: animated.
    pub flags: u64,
    /// A thumbhash to show while it loads (Discord's placeholder version 1).
    pub placeholder: Option<Box<str>>,
}

impl EmbedMedia {
    /// A GIF, or a WebP or AVIF Discord marks animated (the client's `isAnimated`).
    pub fn is_animated(&self) -> bool {
        let path = self.url.split(['?', '#']).next().unwrap_or_default();
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "gif" => true,
            "webp" | "avif" => self.flags & IS_ANIMATED != 0,
            _ => false,
        }
    }
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

impl Sticker {
    /// The sticker's image in its format. A Lottie sticker's key is asked for like any other;
    /// the media layer answers `Unsupported` until Akari has a renderer.
    pub fn image_key(&self) -> ImageKey {
        ImageKey::Sticker {
            id: self.id,
            format: self.format,
        }
    }
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
    /// Keeps any text; Discord's 32 lowercase hex digits, with or without `a_`, take 17 bytes.
    pub fn parse(hash: &str) -> Self {
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

    /// One of the CDN's hashes (32 hex digits, `a_` or not), which has an image behind it;
    /// other text (`clyde`, the client's bundled avatars) doesn't.
    pub fn is_cdn_hash(&self) -> bool {
        matches!(self.0, Repr::Hex { .. })
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

    fn file(name: &str, size: Option<(u32, u32)>, flags: u64) -> Attachment {
        Attachment {
            id: Snowflake::new(1),
            filename: name.into(),
            title: None,
            description: None,
            content_type: None,
            size: 1,
            url: "https://cdn.discordapp.com/attachments/1/1/a".into(),
            proxy_url: "https://media.discordapp.net/attachments/1/1/a".into(),
            width: size.map(|size| size.0),
            height: size.map(|size| size.1),
            flags,
            placeholder: None,
            content_scan_version: None,
        }
    }

    // The official client's lists (module 127038): by extension, whatever the content type.
    #[test]
    fn attachments_have_discords_kinds_by_extension() {
        for name in [
            "a.png", "a.JPG", "a.jpeg", "a.jfif", "a.webp", "a.gif", "a.heic", "a.heif", "a.dng",
            "a.avif",
        ] {
            assert_eq!(file(name, None, 0).kind(), AttachmentKind::Image, "{name}");
        }
        for name in ["a.mp4", "a.webm", "a.MOV", "a.qt"] {
            assert_eq!(file(name, None, 0).kind(), AttachmentKind::Video, "{name}");
        }
        for name in ["a.mp3", "a.m4a", "a.ogg", "a.opus", "a.wav", "a.flac"] {
            assert_eq!(file(name, None, 0).kind(), AttachmentKind::Audio, "{name}");
        }
        for name in ["a.svg", "a.zip", "a.txt", "png", "a.png.exe", ""] {
            assert_eq!(file(name, None, 0).kind(), AttachmentKind::File, "{name}");
        }
    }

    // Shown as media only with its size known; a 1×1 image is a tracking pixel.
    #[test]
    fn only_sized_images_and_videos_are_visual() {
        assert!(file("a.png", Some((640, 480)), 0).is_visual());
        assert!(file("a.mp4", Some((1920, 1080)), 0).is_visual());
        assert!(!file("a.png", None, 0).is_visual());
        assert!(!file("a.png", Some((0, 480)), 0).is_visual());
        assert!(!file("a.png", Some((1, 1)), 0).is_visual());
        assert!(file("a.png", Some((1, 2)), 0).is_visual());
        assert!(!file("a.zip", Some((640, 480)), 0).is_visual());
        assert!(!file("a.mp3", Some((640, 480)), 0).is_visual());
    }

    #[test]
    fn attachment_flags_read_as_discord_sets_them() {
        assert!(file("a.png", None, 1 << 3).is_spoiler());
        assert!(!file("a.png", None, 0).is_spoiler());
        assert!(file("a.png", None, 1 << 1).is_thumbnail());
        // GIFs animate; WebP and AVIF only when Discord says so.
        assert!(file("a.gif", None, 0).is_animated());
        assert!(file("a.webp", None, 1 << 5).is_animated());
        assert!(file("a.avif", None, 1 << 5).is_animated());
        assert!(!file("a.webp", None, 0).is_animated());
        assert!(!file("a.png", None, 1 << 5).is_animated());
    }

    fn user(id: u64) -> User {
        User {
            id: Snowflake::new(id),
            username: "mira".into(),
            global_name: Some("Mira".into()),
            discriminator: Discriminator::Migrated,
            avatar: None,
            bot: false,
            system: false,
            public_flags: 0,
        }
    }

    #[test]
    fn display_name_prefers_the_global_name() {
        let mut user = user(1);
        assert_eq!(user.display_name(), "Mira");

        user.global_name = None;
        assert_eq!(user.display_name(), "mira");
    }

    // The client's order (canary 649848, module 486020): a bot with "0000" and no avatar first,
    // then a legacy tag mod 5, then the ID's timestamp bits mod 6.
    #[test]
    fn the_default_avatar_index_follows_the_client() {
        let legacy = |tag| User {
            discriminator: Discriminator::Tag(tag),
            ..user(80_351_110_224_678_912)
        };
        assert_eq!(legacy(1337).default_avatar_index(), 2);
        assert_eq!(legacy(5).default_avatar_index(), 0);
        assert_eq!(legacy(9999).default_avatar_index(), 4);

        for (id, index) in [
            (1_299_999_999_991_480_320, 0),
            (1_299_999_999_995_674_624, 1),
            (1_299_999_999_999_868_928, 2),
            (1_300_000_000_004_063_232, 3),
            (1_300_000_000_008_257_536, 4),
            (1_300_000_000_012_451_840, 5),
            (80_351_110_224_678_912, 5),
        ] {
            assert_eq!(user(id).default_avatar_index(), index, "{id}");
        }

        let webhook = User {
            bot: true,
            discriminator: Discriminator::Zeros,
            ..user(1_300_000_000_012_451_840)
        };
        assert_eq!(webhook.default_avatar_index(), 0);
        let webhook_with_avatar = User {
            avatar: Some(ImageHash::parse("0123456789abcdef0123456789abcdef")),
            ..webhook.clone()
        };
        assert_eq!(webhook_with_avatar.default_avatar_index(), 5);
        let bot = User {
            bot: true,
            ..user(1_300_000_000_012_451_840)
        };
        assert_eq!(bot.default_avatar_index(), 5);
        let zeros_not_bot = User {
            discriminator: Discriminator::Zeros,
            ..user(1_300_000_000_012_451_840)
        };
        assert_eq!(zeros_not_bot.default_avatar_index(), 5);
        assert_eq!(user(0).default_avatar_index(), 0);
    }

    #[test]
    fn a_stickers_key_carries_its_format() {
        let sticker = Sticker {
            id: StickerId::new(7),
            name: "wave".into(),
            format: StickerFormatType::Lottie,
        };
        assert_eq!(
            sticker.image_key(),
            ImageKey::Sticker {
                id: StickerId::new(7),
                format: StickerFormatType::Lottie,
            }
        );
    }

    #[test]
    fn the_avatar_key_is_the_avatar_or_the_default() {
        let hash = ImageHash::parse("0123456789abcdef0123456789abcdef");
        let with = User {
            avatar: Some(hash.clone()),
            ..user(7)
        };
        assert_eq!(
            with.avatar_key(),
            ImageKey::Avatar {
                user: Snowflake::new(7),
                hash
            }
        );
        assert_eq!(
            user(1_300_000_000_012_451_840).avatar_key(),
            ImageKey::DefaultAvatar { index: 5 }
        );
        // The client's bundled bot avatars aren't CDN hashes.
        let clyde = User {
            bot: true,
            avatar: Some(ImageHash::parse("clyde")),
            ..user(1_300_000_000_012_451_840)
        };
        assert_eq!(clyde.avatar_key(), ImageKey::DefaultAvatar { index: 5 });
    }

    #[test]
    fn guild_and_group_icons_have_keys_when_set() {
        let hash = ImageHash::parse("a_0123456789abcdef0123456789abcdef");
        let mut guild = Guild {
            id: Snowflake::new(3),
            name: "Lab".into(),
            icon: Some(hash.clone()),
            banner: None,
            owner_id: None,
            roles: Arc::from(Vec::new()),
            member_count: None,
            large: false,
            default_message_notifications: NotificationLevel::AllMessages,
            community: false,
            emojis: Box::new([]),
            age_restricted: false,
        };
        assert_eq!(
            guild.icon_key(),
            Some(ImageKey::GuildIcon {
                guild: Snowflake::new(3),
                hash: hash.clone()
            })
        );
        guild.icon = Some(ImageHash::parse("not-a-hash"));
        assert_eq!(guild.icon_key(), None);
        guild.icon = None;
        assert_eq!(guild.icon_key(), None);

        let group = Channel {
            id: Snowflake::new(9),
            kind: ChannelType::GroupDm,
            guild_id: None,
            parent_id: None,
            name: Some("Weekend plans".into()),
            position: 0,
            topic: None,
            nsfw: false,
            rate_limit_per_user: 0,
            default_auto_archive_duration: None,
            last_message_id: None,
            permission_overwrites: Box::new([]),
            recipients: Box::new([]),
            icon: Some(hash.clone()),
            owner_id: None,
            thread: None,
            forum: None,
            flags: 0,
            message_request: false,
            spam: false,
        };
        assert_eq!(
            group.icon_key(),
            Some(ImageKey::GroupIcon {
                channel: Snowflake::new(9),
                hash
            })
        );
    }
}
