// The cache's own shape of each record, so neither the wire structs nor the state types can
// break files already written. A field added later decodes old rows through its default.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::{
    ChannelId, ChannelType, GuildId, MessageId, MessageReferenceType, MessageType, OverwriteType,
    Permissions, PremiumType, Snowflake, StickerFormatType, Timestamp, UserId,
};
use crate::state::{
    Attachment, Call, Channel, ChannelSettings, CurrentUser, Delivery, Discriminator,
    DisplaySettings, Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedMedia, EmbedProvider,
    ExplicitContentFilter, ExplicitContentSettings, FolderEntry, ForumLayout, ForumSettings,
    ForumSort, ForumTag, Guild, GuildSettings, ImageHash, Member, Message, MessageReference,
    NotificationLevel, PermissionOverwrite, ReadState, Redaction, ReferencedMessage, Role,
    RoleSubscription, SpoilerSetting, Sticker, StickerAnimation, SystemData, TagEmoji, TagMatching,
    ThreadInfo, ThreadMembership, Unreads, User,
};

fn is_false(value: &bool) -> bool {
    !value
}

fn nanos(timestamp: Timestamp) -> i64 {
    timestamp.unix_nanos()
}

fn timestamp(nanos: i64) -> Timestamp {
    Timestamp::from_unix_nanos(nanos)
}

fn hash(hash: Option<&ImageHash>) -> Option<Box<str>> {
    hash.map(|hash| hash.to_string().into())
}

fn image(hash: Option<Box<str>>) -> Option<ImageHash> {
    hash.as_deref().map(ImageHash::parse)
}

fn id<M>(id: Option<Snowflake<M>>) -> Option<u64> {
    id.map(Snowflake::get)
}

fn snowflake<M>(id: Option<u64>) -> Option<Snowflake<M>> {
    id.map(Snowflake::new)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct UserRow {
    id: u64,
    username: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    global_name: Option<Box<str>>,
    /// A legacy tag; 0 otherwise.
    discriminator: u16,
    /// "0000" rather than "0": rows from before this field read as "0".
    #[serde(skip_serializing_if = "is_false")]
    zeros: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar: Option<Box<str>>,
    #[serde(skip_serializing_if = "is_false")]
    bot: bool,
    #[serde(skip_serializing_if = "is_false")]
    system: bool,
    public_flags: u64,
}

impl From<&User> for UserRow {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.get(),
            username: user.username.clone(),
            global_name: user.global_name.clone(),
            discriminator: match user.discriminator {
                Discriminator::Tag(tag) => tag,
                _ => 0,
            },
            zeros: user.discriminator == Discriminator::Zeros,
            avatar: hash(user.avatar.as_ref()),
            bot: user.bot,
            system: user.system,
            public_flags: user.public_flags,
        }
    }
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self {
            id: Snowflake::new(row.id),
            username: row.username,
            global_name: row.global_name,
            discriminator: match (row.zeros, row.discriminator) {
                (true, _) => Discriminator::Zeros,
                (false, 0) => Discriminator::Migrated,
                (false, tag) => Discriminator::Tag(tag),
            },
            avatar: image(row.avatar),
            bot: row.bot,
            system: row.system,
            public_flags: row.public_flags,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct CurrentUserRow {
    user: UserRow,
    premium_type: u16,
    mfa_enabled: bool,
    verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nsfw_allowed: Option<bool>,
}

impl From<&CurrentUser> for CurrentUserRow {
    fn from(current: &CurrentUser) -> Self {
        Self {
            user: UserRow::from(&current.user),
            premium_type: current.premium_type.into(),
            mfa_enabled: current.mfa_enabled,
            verified: current.verified,
            nsfw_allowed: current.nsfw_allowed,
        }
    }
}

impl From<CurrentUserRow> for CurrentUser {
    fn from(row: CurrentUserRow) -> Self {
        Self {
            user: row.user.into(),
            premium_type: PremiumType::from(row.premium_type),
            mfa_enabled: row.mfa_enabled,
            verified: row.verified,
            nsfw_allowed: row.nsfw_allowed,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct RoleRow {
    id: u64,
    name: Box<str>,
    position: i32,
    permissions: u64,
    color: u32,
    #[serde(skip_serializing_if = "is_false")]
    hoist: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct GuildRow {
    name: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    banner: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_id: Option<u64>,
    roles: Vec<RoleRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    member_count: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    large: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    community: bool,
    // 0 all messages, 1 only mentions, 2 nothing.
    #[serde(default, skip_serializing_if = "is_zero")]
    default_level: u8,
    // Its custom emoji's IDs, sorted; absent in files written before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    emojis: Vec<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    age_restricted: bool,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

fn level_code(level: NotificationLevel) -> u8 {
    match level {
        NotificationLevel::OnlyMentions => 1,
        NotificationLevel::Nothing => 2,
        NotificationLevel::AllMessages => 0,
    }
}

fn level(code: u8) -> NotificationLevel {
    match code {
        1 => NotificationLevel::OnlyMentions,
        2 => NotificationLevel::Nothing,
        _ => NotificationLevel::AllMessages,
    }
}

impl From<&Guild> for GuildRow {
    fn from(guild: &Guild) -> Self {
        Self {
            name: guild.name.clone(),
            icon: hash(guild.icon.as_ref()),
            banner: hash(guild.banner.as_ref()),
            owner_id: id(guild.owner_id),
            roles: guild
                .roles
                .iter()
                .map(|role| RoleRow {
                    id: role.id.get(),
                    name: role.name.clone(),
                    position: role.position,
                    permissions: role.permissions.0,
                    color: role.color,
                    hoist: role.hoist,
                })
                .collect(),
            member_count: guild.member_count,
            large: guild.large,
            community: guild.community,
            default_level: level_code(guild.default_message_notifications),
            emojis: guild.emojis.iter().map(|emoji| emoji.get()).collect(),
            age_restricted: guild.age_restricted,
        }
    }
}

impl GuildRow {
    pub(super) fn into_guild(self, id: GuildId) -> Guild {
        Guild {
            id,
            name: self.name,
            icon: image(self.icon),
            banner: image(self.banner),
            owner_id: snowflake(self.owner_id),
            roles: self
                .roles
                .into_iter()
                .map(|role| Role {
                    id: Snowflake::new(role.id),
                    name: role.name,
                    position: role.position,
                    permissions: Permissions(role.permissions),
                    color: role.color,
                    hoist: role.hoist,
                })
                .collect(),
            member_count: self.member_count,
            large: self.large,
            default_message_notifications: level(self.default_level),
            community: self.community,
            emojis: self.emojis.into_iter().map(Snowflake::new).collect(),
            age_restricted: self.age_restricted,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct MemberRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    nick: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar: Option<Box<str>>,
    roles: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joined_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    communication_disabled_until: Option<i64>,
    flags: u64,
    #[serde(skip_serializing_if = "is_false")]
    pending: bool,
}

impl From<&Member> for MemberRow {
    fn from(member: &Member) -> Self {
        Self {
            nick: member.nick.clone(),
            avatar: hash(member.avatar.as_ref()),
            roles: member.roles.iter().map(|role| role.get()).collect(),
            joined_at: member.joined_at.map(nanos),
            communication_disabled_until: member.communication_disabled_until.map(nanos),
            flags: member.flags,
            pending: member.pending,
        }
    }
}

impl MemberRow {
    pub(super) fn into_member(self, guild_id: GuildId) -> Member {
        Member {
            guild_id,
            nick: self.nick,
            avatar: image(self.avatar),
            roles: self.roles.into_iter().map(Snowflake::new).collect(),
            joined_at: self.joined_at.map(timestamp),
            communication_disabled_until: self.communication_disabled_until.map(timestamp),
            flags: self.flags,
            pending: self.pending,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct OverwriteRow {
    id: u64,
    kind: u16,
    allow: u64,
    deny: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ThreadMemberRow {
    #[serde(skip_serializing_if = "is_false")]
    muted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mute_ends: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joined_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ThreadRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    member: Option<ThreadMemberRow>,
    archived: bool,
    locked: bool,
    auto_archive_duration: u32,
    archive_timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    create_timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    member_count: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    applied_tags: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_message_sent: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ForumRow {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<TagRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_reaction: Option<EmojiRow>,
    #[serde(skip_serializing_if = "is_false")]
    sort_by_creation: bool,
    #[serde(skip_serializing_if = "is_false")]
    gallery: bool,
    #[serde(skip_serializing_if = "is_false")]
    match_all: bool,
    post_slowmode: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    template: Option<Box<str>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct TagRow {
    id: u64,
    name: Box<str>,
    #[serde(skip_serializing_if = "is_false")]
    moderated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<EmojiRow>,
}

// A custom emoji's ID or a Unicode emoji.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum EmojiRow {
    Custom(u64),
    Unicode(Box<str>),
}

impl EmojiRow {
    fn from_emoji(emoji: &TagEmoji) -> Self {
        match emoji {
            TagEmoji::Custom(id) => Self::Custom(id.get()),
            TagEmoji::Unicode(name) => Self::Unicode(name.clone()),
        }
    }

    fn into_emoji(self) -> TagEmoji {
        match self {
            Self::Custom(id) => TagEmoji::Custom(Snowflake::new(id)),
            Self::Unicode(name) => TagEmoji::Unicode(name),
        }
    }
}

impl From<&ForumSettings> for ForumRow {
    fn from(settings: &ForumSettings) -> Self {
        Self {
            tags: settings
                .tags
                .iter()
                .map(|tag| TagRow {
                    id: tag.id.get(),
                    name: tag.name.clone(),
                    moderated: tag.moderated,
                    emoji: tag.emoji.as_ref().map(EmojiRow::from_emoji),
                })
                .collect(),
            default_reaction: settings.default_reaction.as_ref().map(EmojiRow::from_emoji),
            sort_by_creation: settings.sort == ForumSort::CreationDate,
            gallery: settings.layout == ForumLayout::Gallery,
            match_all: settings.tag_matching == TagMatching::All,
            post_slowmode: settings.post_slowmode,
            template: settings.template.clone(),
        }
    }
}

impl ForumRow {
    fn into_settings(self) -> ForumSettings {
        ForumSettings {
            tags: self
                .tags
                .into_iter()
                .map(|tag| ForumTag {
                    id: Snowflake::new(tag.id),
                    name: tag.name,
                    moderated: tag.moderated,
                    emoji: tag.emoji.map(EmojiRow::into_emoji),
                })
                .collect(),
            default_reaction: self.default_reaction.map(EmojiRow::into_emoji),
            sort: if self.sort_by_creation {
                ForumSort::CreationDate
            } else {
                ForumSort::LatestActivity
            },
            layout: if self.gallery {
                ForumLayout::Gallery
            } else {
                ForumLayout::List
            },
            tag_matching: if self.match_all {
                TagMatching::All
            } else {
                TagMatching::Some
            },
            post_slowmode: self.post_slowmode,
            template: self.template,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct ChannelRow {
    kind: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    guild_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<Box<str>>,
    position: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<Box<str>>,
    #[serde(skip_serializing_if = "is_false")]
    nsfw: bool,
    rate_limit_per_user: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_auto_archive_duration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_id: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    permission_overwrites: Vec<OverwriteRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recipients: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread: Option<ThreadRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forum: Option<ForumRow>,
    flags: u64,
    #[serde(skip_serializing_if = "is_false")]
    message_request: bool,
    #[serde(skip_serializing_if = "is_false")]
    spam: bool,
}

impl From<&Channel> for ChannelRow {
    fn from(channel: &Channel) -> Self {
        Self {
            kind: channel.kind.into(),
            guild_id: id(channel.guild_id),
            parent_id: id(channel.parent_id),
            name: channel.name.clone(),
            position: channel.position,
            topic: channel.topic.clone(),
            nsfw: channel.nsfw,
            rate_limit_per_user: channel.rate_limit_per_user,
            default_auto_archive_duration: channel.default_auto_archive_duration,
            last_message_id: id(channel.last_message_id),
            permission_overwrites: channel
                .permission_overwrites
                .iter()
                .map(|overwrite| OverwriteRow {
                    id: overwrite.id.get(),
                    kind: overwrite.kind.into(),
                    allow: overwrite.allow.0,
                    deny: overwrite.deny.0,
                })
                .collect(),
            recipients: channel.recipients.iter().map(|user| user.get()).collect(),
            icon: hash(channel.icon.as_ref()),
            owner_id: id(channel.owner_id),
            thread: channel.thread.as_deref().map(|thread| ThreadRow {
                member: thread.member.map(|member| ThreadMemberRow {
                    muted: member.muted,
                    mute_ends: member.mute_ends.map(nanos),
                    level: member.level.map(level_code),
                    joined_at: member.joined_at.map(nanos),
                }),
                archived: thread.archived,
                locked: thread.locked,
                auto_archive_duration: thread.auto_archive_duration,
                archive_timestamp: nanos(thread.archive_timestamp),
                create_timestamp: thread.create_timestamp.map(nanos),
                message_count: thread.message_count,
                member_count: thread.member_count,
                applied_tags: thread.applied_tags.iter().map(|tag| tag.get()).collect(),
                total_message_sent: thread.total_message_sent,
            }),
            forum: channel.forum.as_deref().map(ForumRow::from),
            flags: channel.flags,
            message_request: channel.message_request,
            spam: channel.spam,
        }
    }
}

impl ChannelRow {
    pub(super) fn into_channel(self, id: ChannelId) -> Channel {
        Channel {
            id,
            kind: ChannelType::from(self.kind),
            guild_id: snowflake(self.guild_id),
            parent_id: snowflake(self.parent_id),
            name: self.name,
            position: self.position,
            topic: self.topic,
            nsfw: self.nsfw,
            rate_limit_per_user: self.rate_limit_per_user,
            default_auto_archive_duration: self.default_auto_archive_duration,
            last_message_id: snowflake(self.last_message_id),
            permission_overwrites: self
                .permission_overwrites
                .into_iter()
                .map(|overwrite| PermissionOverwrite {
                    id: Snowflake::new(overwrite.id),
                    kind: OverwriteType::from(overwrite.kind),
                    allow: Permissions(overwrite.allow),
                    deny: Permissions(overwrite.deny),
                })
                .collect(),
            recipients: self.recipients.into_iter().map(Snowflake::new).collect(),
            icon: image(self.icon),
            owner_id: snowflake(self.owner_id),
            thread: self.thread.map(|thread| {
                Box::new(ThreadInfo {
                    member: thread.member.map(|member| ThreadMembership {
                        muted: member.muted,
                        mute_ends: member.mute_ends.map(timestamp),
                        level: member.level.map(level),
                        joined_at: member.joined_at.map(timestamp),
                    }),
                    archived: thread.archived,
                    locked: thread.locked,
                    auto_archive_duration: thread.auto_archive_duration,
                    archive_timestamp: timestamp(thread.archive_timestamp),
                    create_timestamp: thread.create_timestamp.map(timestamp),
                    message_count: thread.message_count,
                    member_count: thread.member_count,
                    member_ids_preview: Box::default(),
                    applied_tags: thread
                        .applied_tags
                        .into_iter()
                        .map(Snowflake::new)
                        .collect(),
                    total_message_sent: thread.total_message_sent,
                })
            }),
            forum: self.forum.map(|forum| Box::new(forum.into_settings())),
            flags: self.flags,
            message_request: self.message_request,
            spam: self.spam,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct AttachmentRow {
    id: u64,
    filename: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<Box<str>>,
    size: u64,
    url: Box<str>,
    proxy_url: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_scan_version: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedMediaRow {
    url: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_url: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "is_zero_u64")]
    flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<Box<str>>,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

impl From<&EmbedMedia> for EmbedMediaRow {
    fn from(media: &EmbedMedia) -> Self {
        Self {
            url: media.url.clone(),
            proxy_url: media.proxy_url.clone(),
            width: media.width,
            height: media.height,
            flags: media.flags,
            placeholder: media.placeholder.clone(),
        }
    }
}

impl From<EmbedMediaRow> for EmbedMedia {
    fn from(row: EmbedMediaRow) -> Self {
        Self {
            url: row.url,
            proxy_url: row.proxy_url,
            width: row.width,
            height: row.height,
            flags: row.flags,
            placeholder: row.placeholder,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedAuthorRow {
    name: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_icon_url: Option<Box<str>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedProviderRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<Box<str>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedFooterRow {
    text: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_icon_url: Option<Box<str>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedFieldRow {
    name: Box<str>,
    value: Box<str>,
    #[serde(skip_serializing_if = "is_false")]
    inline: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct EmbedRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<EmbedAuthorRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<EmbedProviderRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    footer: Option<EmbedFooterRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<EmbedMediaRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<EmbedMediaRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<EmbedMediaRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<EmbedFieldRow>,
    #[serde(skip_serializing_if = "is_zero_u64")]
    flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_scan_version: Option<u32>,
}

impl From<&Embed> for EmbedRow {
    fn from(embed: &Embed) -> Self {
        Self {
            kind: embed.kind.clone(),
            title: embed.title.clone(),
            description: embed.description.clone(),
            url: embed.url.clone(),
            timestamp: embed.timestamp.map(nanos),
            color: embed.color,
            author: embed.author.as_ref().map(|author| EmbedAuthorRow {
                name: author.name.clone(),
                url: author.url.clone(),
                proxy_icon_url: author.proxy_icon_url.clone(),
            }),
            provider: embed.provider.as_ref().map(|provider| EmbedProviderRow {
                name: provider.name.clone(),
                url: provider.url.clone(),
            }),
            footer: embed.footer.as_ref().map(|footer| EmbedFooterRow {
                text: footer.text.clone(),
                proxy_icon_url: footer.proxy_icon_url.clone(),
            }),
            image: embed.image.as_ref().map(EmbedMediaRow::from),
            thumbnail: embed.thumbnail.as_ref().map(EmbedMediaRow::from),
            video: embed.video.as_ref().map(EmbedMediaRow::from),
            fields: embed
                .fields
                .iter()
                .map(|field| EmbedFieldRow {
                    name: field.name.clone(),
                    value: field.value.clone(),
                    inline: field.inline,
                })
                .collect(),
            flags: embed.flags,
            content_scan_version: embed.content_scan_version,
        }
    }
}

impl From<EmbedRow> for Embed {
    fn from(row: EmbedRow) -> Self {
        Self {
            kind: row.kind,
            title: row.title,
            description: row.description,
            url: row.url,
            timestamp: row.timestamp.map(timestamp),
            color: row.color,
            author: row.author.map(|author| EmbedAuthor {
                name: author.name,
                url: author.url,
                proxy_icon_url: author.proxy_icon_url,
            }),
            provider: row.provider.map(|provider| EmbedProvider {
                name: provider.name,
                url: provider.url,
            }),
            footer: row.footer.map(|footer| EmbedFooter {
                text: footer.text,
                proxy_icon_url: footer.proxy_icon_url,
            }),
            image: row.image.map(EmbedMedia::from),
            thumbnail: row.thumbnail.map(EmbedMedia::from),
            video: row.video.map(EmbedMedia::from),
            fields: row
                .fields
                .into_iter()
                .map(|field| EmbedField {
                    name: field.name,
                    value: field.value,
                    inline: field.inline,
                })
                .collect(),
            flags: row.flags,
            content_scan_version: row.content_scan_version,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct StickerRow {
    id: u64,
    name: Box<str>,
    format: u16,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ReferenceRow {
    kind: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guild_id: Option<u64>,
}

// The referenced message isn't kept: the reference holds its ID, and a fresh page brings it.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct MessageRow {
    kind: u16,
    author: UserRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    webhook_id: Option<u64>,
    content: Box<str>,
    timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    edited_timestamp: Option<i64>,
    flags: u64,
    #[serde(skip_serializing_if = "is_false")]
    pinned: bool,
    #[serde(skip_serializing_if = "is_false")]
    tts: bool,
    #[serde(skip_serializing_if = "is_false")]
    mention_everyone: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mentions: Vec<UserRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mention_roles: Vec<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<EmbedRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stickers: Vec<StickerRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<ReferenceRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    call: Option<CallRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role_subscription: Option<RoleSubscriptionRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    purchased_product: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<UserRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    application_name: Option<Box<str>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct CallRow {
    participants: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_timestamp: Option<i64>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct RoleSubscriptionRow {
    tier_name: Box<str>,
    total_months_subscribed: u32,
    is_renewal: bool,
}

impl From<&Message> for MessageRow {
    fn from(message: &Message) -> Self {
        let system = message.system.as_deref();
        Self {
            kind: message.kind.into(),
            author: UserRow::from(&*message.author),
            webhook_id: id(message.webhook_id),
            content: message.content.clone(),
            timestamp: nanos(message.timestamp),
            edited_timestamp: message.edited_timestamp.map(nanos),
            flags: message.flags,
            pinned: message.pinned,
            tts: message.tts,
            mention_everyone: message.mention_everyone,
            mentions: message
                .mentions
                .iter()
                .map(|user| UserRow::from(&**user))
                .collect(),
            mention_roles: message
                .mention_roles
                .iter()
                .map(|role| role.get())
                .collect(),
            attachments: message
                .attachments
                .iter()
                .map(|attachment| AttachmentRow {
                    id: attachment.id.get(),
                    filename: attachment.filename.clone(),
                    title: attachment.title.clone(),
                    description: attachment.description.clone(),
                    content_type: attachment.content_type.clone(),
                    size: attachment.size,
                    url: attachment.url.clone(),
                    proxy_url: attachment.proxy_url.clone(),
                    width: attachment.width,
                    height: attachment.height,
                    flags: attachment.flags,
                    placeholder: attachment.placeholder.clone(),
                    content_scan_version: attachment.content_scan_version,
                })
                .collect(),
            embeds: message.embeds.iter().map(EmbedRow::from).collect(),
            stickers: message
                .stickers
                .iter()
                .map(|sticker| StickerRow {
                    id: sticker.id.get(),
                    name: sticker.name.clone(),
                    format: sticker.format.into(),
                })
                .collect(),
            reference: message.reference.as_deref().map(|reference| ReferenceRow {
                kind: reference.kind.into(),
                message_id: id(reference.message_id),
                channel_id: id(reference.channel_id),
                guild_id: id(reference.guild_id),
            }),
            call: system
                .and_then(|system| system.call.as_ref())
                .map(|call| CallRow {
                    participants: call.participants.iter().map(|user| user.get()).collect(),
                    ended_timestamp: call.ended_timestamp.map(nanos),
                }),
            role_subscription: system
                .and_then(|system| system.role_subscription.as_ref())
                .map(|data| RoleSubscriptionRow {
                    tier_name: data.tier_name.clone(),
                    total_months_subscribed: data.total_months_subscribed,
                    is_renewal: data.is_renewal,
                }),
            purchased_product: system.and_then(|system| system.purchased_product.clone()),
            actor: system
                .and_then(|system| system.actor.as_deref())
                .map(UserRow::from),
            application_name: system.and_then(|system| system.application_name.clone()),
        }
    }
}

impl MessageRow {
    // Authors and mentions equal to one already read share its Arc, as they do in the store.
    pub(super) fn into_message(
        self,
        id: MessageId,
        channel_id: ChannelId,
        users: &mut HashMap<UserId, Arc<User>>,
    ) -> Message {
        let mut intern = |row: UserRow| {
            let user = User::from(row);
            match users.get(&user.id) {
                Some(known) if **known == user => known.clone(),
                _ => {
                    let user = Arc::new(user);
                    users.insert(user.id, user.clone());
                    user
                }
            }
        };
        Message {
            id,
            channel_id,
            kind: MessageType::from(self.kind),
            author: intern(self.author),
            webhook_id: snowflake(self.webhook_id),
            content: self.content,
            timestamp: timestamp(self.timestamp),
            edited_timestamp: self.edited_timestamp.map(timestamp),
            flags: self.flags,
            pinned: self.pinned,
            tts: self.tts,
            mention_everyone: self.mention_everyone,
            mentions: self.mentions.into_iter().map(&mut intern).collect(),
            mention_roles: self.mention_roles.into_iter().map(Snowflake::new).collect(),
            attachments: self
                .attachments
                .into_iter()
                .map(|attachment| Attachment {
                    id: Snowflake::new(attachment.id),
                    filename: attachment.filename,
                    description: attachment.description,
                    content_type: attachment.content_type,
                    size: attachment.size,
                    url: attachment.url,
                    proxy_url: attachment.proxy_url,
                    width: attachment.width,
                    height: attachment.height,
                    flags: attachment.flags,
                    title: attachment.title,
                    placeholder: attachment.placeholder,
                    content_scan_version: attachment.content_scan_version,
                })
                .collect(),
            embeds: self.embeds.into_iter().map(Embed::from).collect(),
            stickers: self
                .stickers
                .into_iter()
                .map(|sticker| Sticker {
                    id: Snowflake::new(sticker.id),
                    name: sticker.name,
                    format: StickerFormatType::from(sticker.format),
                })
                .collect(),
            reference: self.reference.map(|reference| {
                Box::new(MessageReference {
                    kind: MessageReferenceType::from(reference.kind),
                    message_id: snowflake(reference.message_id),
                    channel_id: snowflake(reference.channel_id),
                    guild_id: snowflake(reference.guild_id),
                })
            }),
            referenced_message: ReferencedMessage::NotIncluded,
            delivery: Delivery::Sent,
            system: SystemData {
                call: self.call.map(|call| Call {
                    participants: call.participants.into_iter().map(Snowflake::new).collect(),
                    ended_timestamp: call.ended_timestamp.map(timestamp),
                }),
                role_subscription: self.role_subscription.map(|data| RoleSubscription {
                    tier_name: data.tier_name,
                    total_months_subscribed: data.total_months_subscribed,
                    is_renewal: data.is_renewal,
                }),
                purchased_product: self.purchased_product,
                actor: self.actor.map(&mut intern),
                application_name: self.application_name,
            }
            .boxed(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct GuildSettingsRow {
    #[serde(default, skip_serializing_if = "is_false")]
    muted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mute_ends: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unreads: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    suppress_everyone: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    suppress_roles: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    hide_muted_channels: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    opt_in: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    channels: Vec<ChannelSettingsRow>,
}

#[derive(Serialize, Deserialize)]
struct ChannelSettingsRow {
    id: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    muted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mute_ends: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unreads: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    opted_in: bool,
}

// 0 all messages, 1 only mentions.
fn unreads_code(unreads: Unreads) -> u8 {
    match unreads {
        Unreads::OnlyMentions => 1,
        Unreads::AllMessages => 0,
    }
}

fn unreads(code: u8) -> Unreads {
    match code {
        1 => Unreads::OnlyMentions,
        _ => Unreads::AllMessages,
    }
}

impl From<&GuildSettings> for GuildSettingsRow {
    fn from(settings: &GuildSettings) -> Self {
        let mut channels: Vec<ChannelSettingsRow> = settings
            .channels
            .iter()
            .map(|(id, channel)| ChannelSettingsRow {
                id: id.get(),
                muted: channel.muted,
                mute_ends: channel.mute_ends.map(nanos),
                level: channel.level.map(level_code),
                unreads: channel.unreads.map(unreads_code),
                opted_in: channel.opted_in,
            })
            .collect();
        channels.sort_unstable_by_key(|channel| channel.id);
        Self {
            muted: settings.muted,
            mute_ends: settings.mute_ends.map(nanos),
            level: settings.level.map(level_code),
            unreads: settings.unreads.map(unreads_code),
            suppress_everyone: settings.suppress_everyone,
            suppress_roles: settings.suppress_roles,
            hide_muted_channels: settings.hide_muted_channels,
            opt_in: settings.opt_in,
            channels,
        }
    }
}

impl GuildSettingsRow {
    pub(super) fn into_settings(self) -> GuildSettings {
        GuildSettings {
            muted: self.muted,
            mute_ends: self.mute_ends.map(timestamp),
            level: self.level.map(level),
            unreads: self.unreads.map(unreads),
            suppress_everyone: self.suppress_everyone,
            suppress_roles: self.suppress_roles,
            hide_muted_channels: self.hide_muted_channels,
            opt_in: self.opt_in,
            channels: Arc::new(
                self.channels
                    .into_iter()
                    .map(|channel| {
                        let settings = ChannelSettings {
                            muted: channel.muted,
                            mute_ends: channel.mute_ends.map(timestamp),
                            level: channel.level.map(level),
                            unreads: channel.unreads.map(unreads),
                            opted_in: channel.opted_in,
                        };
                        (Snowflake::new(channel.id), settings)
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct FolderRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<Box<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<u32>,
    guilds: Vec<u64>,
}

// Discord's own numbers where it has them (stickers, the filter, redactions); the spoiler
// setting counts its three words. A value this build doesn't know takes the default.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DisplayRow {
    render_spoilers: u8,
    view_image_descriptions: bool,
    inline_attachment_media: bool,
    inline_embed_media: bool,
    gif_auto_play: bool,
    render_embeds: bool,
    animate_emoji: bool,
    animate_stickers: u8,
    explicit_content_filter: u8,
    explicit_content: [u8; 3],
    gore: [u8; 3],
    self_harm: [u8; 3],
}

impl Default for DisplayRow {
    fn default() -> Self {
        Self::from(&DisplaySettings::default())
    }
}

fn redactions_row(settings: &ExplicitContentSettings) -> [u8; 3] {
    [settings.guilds, settings.friend_dm, settings.non_friend_dm].map(|redaction| match redaction {
        Redaction::Show => 1,
        Redaction::Blur => 2,
        Redaction::Block => 3,
        _ => 0,
    })
}

fn redactions(row: [u8; 3]) -> ExplicitContentSettings {
    let redaction = |value: u8| match value {
        1 => Redaction::Show,
        2 => Redaction::Blur,
        3 => Redaction::Block,
        _ => Redaction::Unset,
    };
    ExplicitContentSettings {
        guilds: redaction(row[0]),
        friend_dm: redaction(row[1]),
        non_friend_dm: redaction(row[2]),
    }
}

impl From<&DisplaySettings> for DisplayRow {
    fn from(display: &DisplaySettings) -> Self {
        Self {
            render_spoilers: match display.render_spoilers {
                SpoilerSetting::Always => 0,
                SpoilerSetting::IfModerator => 2,
                _ => 1,
            },
            view_image_descriptions: display.view_image_descriptions,
            inline_attachment_media: display.inline_attachment_media,
            inline_embed_media: display.inline_embed_media,
            gif_auto_play: display.gif_auto_play,
            render_embeds: display.render_embeds,
            animate_emoji: display.animate_emoji,
            animate_stickers: match display.animate_stickers {
                StickerAnimation::OnInteraction => 1,
                StickerAnimation::Never => 2,
                _ => 0,
            },
            explicit_content_filter: match display.explicit_content_filter {
                ExplicitContentFilter::Disabled => 0,
                ExplicitContentFilter::All => 2,
                _ => 1,
            },
            explicit_content: redactions_row(&display.explicit_content),
            gore: redactions_row(&display.gore),
            self_harm: redactions_row(&display.self_harm),
        }
    }
}

impl From<DisplayRow> for DisplaySettings {
    fn from(row: DisplayRow) -> Self {
        Self {
            render_spoilers: match row.render_spoilers {
                0 => SpoilerSetting::Always,
                2 => SpoilerSetting::IfModerator,
                _ => SpoilerSetting::OnClick,
            },
            view_image_descriptions: row.view_image_descriptions,
            inline_attachment_media: row.inline_attachment_media,
            inline_embed_media: row.inline_embed_media,
            gif_auto_play: row.gif_auto_play,
            render_embeds: row.render_embeds,
            animate_emoji: row.animate_emoji,
            animate_stickers: match row.animate_stickers {
                1 => StickerAnimation::OnInteraction,
                2 => StickerAnimation::Never,
                _ => StickerAnimation::Always,
            },
            explicit_content_filter: match row.explicit_content_filter {
                0 => ExplicitContentFilter::Disabled,
                2 => ExplicitContentFilter::All,
                _ => ExplicitContentFilter::NonFriends,
            },
            explicit_content: redactions(row.explicit_content),
            gore: redactions(row.gore),
            self_harm: redactions(row.self_harm),
        }
    }
}

impl From<&FolderEntry> for FolderRow {
    fn from(folder: &FolderEntry) -> Self {
        Self {
            id: folder.id,
            name: folder.name.clone(),
            color: folder.color,
            guilds: folder.guilds.iter().map(|guild| guild.get()).collect(),
        }
    }
}

impl From<FolderRow> for FolderEntry {
    fn from(row: FolderRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            color: row.color,
            guilds: row.guilds.into_iter().map(Snowflake::new).collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct ReadStateRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acked: Option<u64>,
    #[serde(default)]
    mentions: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    estimated: bool,
}

impl From<&ReadState> for ReadStateRow {
    fn from(state: &ReadState) -> Self {
        Self {
            acked: id(state.acked),
            mentions: state.mentions,
            estimated: state.estimated,
        }
    }
}

impl ReadStateRow {
    pub(super) fn into_read_state(self) -> ReadState {
        ReadState {
            acked: snowflake(self.acked),
            mentions: self.mentions,
            estimated: self.estimated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_row_from_before_zeros_reads_as_migrated() {
        let row: UserRow = serde_json::from_str(
            r#"{"id":7,"username":"hook","discriminator":0,"bot":true,"public_flags":0}"#,
        )
        .unwrap();

        assert_eq!(User::from(row).discriminator, Discriminator::Migrated);
    }

    #[test]
    fn discriminators_round_trip() {
        for discriminator in [
            Discriminator::Migrated,
            Discriminator::Zeros,
            Discriminator::Tag(1337),
        ] {
            let user = User {
                id: Snowflake::new(7),
                username: "hook".into(),
                global_name: None,
                discriminator,
                avatar: None,
                bot: true,
                system: false,
                public_flags: 0,
            };
            let json = serde_json::to_string(&UserRow::from(&user)).unwrap();
            let read: UserRow = serde_json::from_str(&json).unwrap();

            assert_eq!(User::from(read).discriminator, discriminator, "{json}");
        }
    }
}
