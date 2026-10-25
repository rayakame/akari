use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use akari_core::model::{
    AttachmentId, ChannelId, ChannelType, GuildId, MessageId, MessageType, StickerFormatType,
    StickerId, Timestamp, UserId,
};
use akari_core::state;

use crate::errors::GatewayError;
use crate::formatted::FormattedText;
use crate::images::ImageKey;

/// A user, as the current user, a DM recipient or a message author.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub global_name: Option<String>,
    /// The global name, or the username if there is none.
    pub display_name: String,
    pub bot: bool,
    pub system: bool,
    /// The avatar to show: the user's own, or one of Discord's default avatars (every view
    /// asks for it the same way).
    pub avatar: ImageKey,
}

impl From<&state::User> for User {
    fn from(user: &state::User) -> Self {
        Self {
            id: user.id,
            username: user.username.to_string(),
            global_name: user.global_name.as_deref().map(str::to_owned),
            display_name: user.display_name().to_owned(),
            bot: user.bot,
            system: user.system,
            avatar: user.avatar_key().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    /// `None` without an icon: the host shows the name's initials.
    #[uniffi(default = None)]
    pub icon: Option<ImageKey>,
}

impl From<&state::Guild> for Guild {
    fn from(guild: &state::Guild) -> Self {
        Self {
            id: guild.id,
            name: guild.name.to_string(),
            icon: guild.icon_key().map(Into::into),
        }
    }
}

/// One entry of the server list, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ServerListItem {
    Guild { guild_id: GuildId },
    Folder { folder: Folder },
}

impl ServerListItem {
    // `None` for a kind of entry the bindings don't know yet.
    pub(crate) fn new(item: &state::ServerListItem) -> Option<Self> {
        Some(match item {
            state::ServerListItem::Guild(guild_id) => Self::Guild {
                guild_id: *guild_id,
            },
            state::ServerListItem::Folder(folder) => Self::Folder {
                folder: Folder {
                    id: folder.id,
                    name: folder.name.as_deref().map(str::to_owned),
                    color: folder.color,
                    guild_ids: folder.guilds.to_vec(),
                },
            },
            _ => return None,
        })
    }
}

/// A server folder from the user's settings.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Folder {
    pub id: i64,
    /// `None` when the user named it nothing: show its servers' names.
    pub name: Option<String>,
    /// 0xRRGGBB; `None` for the default color.
    pub color: Option<u32>,
    pub guild_ids: Vec<GuildId>,
}

/// What a list shows for a channel, a guild or (added up by the host) a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Badge {
    /// Shows as unread: the white dot or pill, a bright row.
    pub unread: bool,
    /// The red count, shown also when muted.
    pub mentions: u32,
}

impl From<state::Badge> for Badge {
    fn from(badge: state::Badge) -> Self {
        Self {
            unread: badge.unread,
            mentions: badge.mentions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ChannelBadge {
    pub channel_id: ChannelId,
    pub badge: Badge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct GuildBadge {
    pub guild_id: GuildId,
    pub badge: Badge,
}

/// Where the user stopped reading a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ReadState {
    /// The last acknowledged message; `None` when nothing is read yet.
    pub acked: Option<MessageId>,
    pub mentions: u32,
    /// Akari started it because Discord sent none: `acked` isn't a message the user read.
    pub estimated: bool,
}

impl From<state::ReadState> for ReadState {
    fn from(state: state::ReadState) -> Self {
        Self {
            acked: state.acked,
            mentions: state.mentions,
            estimated: state.estimated,
        }
    }
}

/// The user's notification settings for a guild, or for DMs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GuildSettings {
    /// Muted when it was read. `GuildSettingsChanged` follows when a timed mute ends.
    pub muted: bool,
    /// When the mute ends; `None` when it lasts until unmuted, or nothing is muted.
    pub mute_ends_at: Option<SystemTime>,
    /// @everyone and @here.
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    pub hide_muted_channels: bool,
}

impl GuildSettings {
    pub(crate) fn new(settings: &state::GuildSettings, muted: bool) -> Self {
        Self {
            muted,
            mute_ends_at: settings.mute_ends.filter(|_| muted).map(system_time),
            suppress_everyone: settings.suppress_everyone,
            suppress_roles: settings.suppress_roles,
            hide_muted_channels: settings.hide_muted_channels,
        }
    }
}

/// How media shows and animates, from the user's settings (`text_and_images`); Discord's
/// defaults where the user set nothing. `DisplaySettingsChanged` says to read it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct DisplaySettings {
    pub render_spoilers: SpoilerSetting,
    /// Show the ALT button on media with a description.
    pub view_image_descriptions: bool,
    /// Show attachments' media inline.
    pub inline_attachment_media: bool,
    /// Show linked media inline.
    pub inline_embed_media: bool,
    /// Play GIFs while the app is focused.
    pub gif_auto_play: bool,
    pub render_embeds: bool,
    pub animate_emoji: bool,
    pub animate_stickers: StickerAnimation,
    pub explicit_content_filter: ExplicitContentFilter,
    pub explicit_content: ExplicitContentSettings,
    pub gore: ExplicitContentSettings,
    pub self_harm: ExplicitContentSettings,
}

/// When spoilers are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SpoilerSetting {
    Always,
    /// Discord's default.
    OnClick,
    /// Always for those with Manage Messages.
    IfModerator,
}

/// When stickers animate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StickerAnimation {
    /// Discord's default.
    Always,
    /// While the message is hovered.
    OnInteraction,
    Never,
}

/// The legacy explicit content filter, which decides the per-context defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ExplicitContentFilter {
    Disabled,
    /// Discord's default.
    NonFriends,
    All,
}

/// How flagged media is shown in each context; `Unset` leaves it to the filter's defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ExplicitContentSettings {
    pub guilds: Redaction,
    pub friend_dm: Redaction,
    pub non_friend_dm: Redaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Redaction {
    Unset,
    Show,
    Blur,
    Block,
}

impl From<state::DisplaySettings> for DisplaySettings {
    fn from(display: state::DisplaySettings) -> Self {
        let redactions = |settings: state::ExplicitContentSettings| ExplicitContentSettings {
            guilds: settings.guilds.into(),
            friend_dm: settings.friend_dm.into(),
            non_friend_dm: settings.non_friend_dm.into(),
        };
        Self {
            render_spoilers: match display.render_spoilers {
                state::SpoilerSetting::Always => SpoilerSetting::Always,
                state::SpoilerSetting::IfModerator => SpoilerSetting::IfModerator,
                _ => SpoilerSetting::OnClick,
            },
            view_image_descriptions: display.view_image_descriptions,
            inline_attachment_media: display.inline_attachment_media,
            inline_embed_media: display.inline_embed_media,
            gif_auto_play: display.gif_auto_play,
            render_embeds: display.render_embeds,
            animate_emoji: display.animate_emoji,
            animate_stickers: match display.animate_stickers {
                state::StickerAnimation::OnInteraction => StickerAnimation::OnInteraction,
                state::StickerAnimation::Never => StickerAnimation::Never,
                _ => StickerAnimation::Always,
            },
            explicit_content_filter: match display.explicit_content_filter {
                state::ExplicitContentFilter::Disabled => ExplicitContentFilter::Disabled,
                state::ExplicitContentFilter::All => ExplicitContentFilter::All,
                _ => ExplicitContentFilter::NonFriends,
            },
            explicit_content: redactions(display.explicit_content),
            gore: redactions(display.gore),
            self_harm: redactions(display.self_harm),
        }
    }
}

impl From<state::Redaction> for Redaction {
    fn from(redaction: state::Redaction) -> Self {
        match redaction {
            state::Redaction::Show => Self::Show,
            state::Redaction::Blur => Self::Blur,
            state::Redaction::Block => Self::Block,
            _ => Self::Unset,
        }
    }
}

/// A guild channel, category, thread, DM or group DM.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Channel {
    pub id: ChannelId,
    pub kind: ChannelType,
    /// `None` for DMs and group DMs.
    pub guild_id: Option<GuildId>,
    /// A guild channel's category, or a thread's parent channel.
    pub parent_id: Option<ChannelId>,
    /// `None` for DMs.
    pub name: Option<String>,
    pub position: i32,
    pub topic: Option<String>,
    pub nsfw: bool,
    /// Slowmode in seconds.
    pub rate_limit_per_user: u32,
    /// DM and group DM recipients without the current user; see `Store::user`.
    pub recipient_ids: Vec<UserId>,
    /// A group DM's icon; `None` without one.
    #[uniffi(default = None)]
    pub icon: Option<ImageKey>,
    /// Set for threads.
    pub thread: Option<ThreadInfo>,
}

/// A thread's state.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ThreadInfo {
    /// The user is a member; only joined threads are listed.
    pub joined: bool,
    /// Closed: it can be opened, and sending reopens it.
    pub archived: bool,
    /// Only moderators can send.
    pub locked: bool,
    /// Only its members and moderators see it.
    pub private: bool,
    /// As the store keeps it: moved by every message in the thread and every delete, which
    /// `MessageUpdated` for its box's row reports, not `ChannelUpdated`.
    pub message_count: Option<u32>,
    /// Who started it.
    pub owner_id: Option<UserId>,
    /// Its newest message, moved by every message in it.
    pub last_message_id: Option<MessageId>,
    /// `None` for threads created before 2022-01-09.
    pub created_at: Option<SystemTime>,
    pub member_count: Option<u32>,
    /// Some of its members, as Discord picks them, for a row of faces; empty for a thread
    /// restored from the disk cache until Discord sends it again.
    pub member_ids_preview: Vec<UserId>,
    /// When it was last archived or unarchived.
    pub archived_at: SystemTime,
}

/// What a channel's Threads browser lists: see `Store::thread_browser`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ThreadBrowser {
    /// Active threads the user joined, the newest message first.
    pub joined: Vec<ChannelId>,
    /// Active threads the user hasn't joined, the newest message first.
    pub others: Vec<ChannelId>,
    /// Archived threads the loaded pages reach, the newest message first.
    pub archived: Vec<ChannelId>,
    pub older: OlderThreads,
}

/// Where loading a channel's archived threads stands in this connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OlderThreads {
    NotLoaded,
    Loading,
    /// Discord has more: `Account::load_older_threads` loads the next page.
    More,
    All,
    /// Nothing more until the next connection.
    Failed,
}

impl From<state::ThreadBrowser> for ThreadBrowser {
    fn from(browser: state::ThreadBrowser) -> Self {
        let ids =
            |threads: Vec<Arc<state::Channel>>| threads.iter().map(|thread| thread.id).collect();
        Self {
            joined: ids(browser.joined),
            others: ids(browser.others),
            archived: ids(browser.archived),
            older: match browser.older {
                state::OlderThreads::NotLoaded => OlderThreads::NotLoaded,
                state::OlderThreads::Loading => OlderThreads::Loading,
                state::OlderThreads::More => OlderThreads::More,
                state::OlderThreads::All => OlderThreads::All,
                _ => OlderThreads::Failed,
            },
        }
    }
}

impl From<&state::Channel> for Channel {
    fn from(channel: &state::Channel) -> Self {
        Self {
            id: channel.id,
            kind: channel.kind,
            guild_id: channel.guild_id,
            parent_id: channel.parent_id,
            name: channel.name.as_deref().map(str::to_owned),
            position: channel.position,
            topic: channel.topic.as_deref().map(str::to_owned),
            nsfw: channel.nsfw,
            rate_limit_per_user: channel.rate_limit_per_user,
            recipient_ids: channel.recipients.to_vec(),
            icon: channel.icon_key().map(Into::into),
            thread: channel.thread.as_deref().map(|info| ThreadInfo {
                joined: info.joined(),
                archived: info.archived,
                locked: info.locked,
                private: channel.kind == ChannelType::PrivateThread,
                message_count: info.message_count,
                owner_id: channel.owner_id,
                last_message_id: channel.last_message_id.map(|id| id.cast()),
                created_at: info.create_timestamp.map(system_time),
                member_count: info.member_count,
                member_ids_preview: info.member_ids_preview.to_vec(),
                archived_at: system_time(info.archive_timestamp),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub kind: MessageType,
    pub author: User,
    /// Sent by a webhook; the author is then not a real user.
    pub from_webhook: bool,
    pub content: String,
    pub timestamp: SystemTime,
    pub edited_timestamp: Option<SystemTime>,
    pub pinned: bool,
    pub mention_everyone: bool,
    pub attachments: Vec<Attachment>,
    /// Embeds aren't rendered yet; the count is enough for a placeholder.
    pub embed_count: u32,
    pub stickers: Vec<Sticker>,
    /// Laid out with Components V2, which Akari can't show yet; such a message has no content.
    pub components_v2: bool,
    pub delivery: Delivery,
    pub formatted: FormattedText,
    /// Mentions the current user, by ID, @everyone or @here, or a role of theirs.
    pub mentions_me: bool,
    /// What a reply, pin, crosspost, forward or thread notice points at; a "started a thread"
    /// message's `channel_id` is its thread.
    pub reference: Option<MessageReference>,
    /// A thread was started from this message.
    pub has_thread: bool,
    /// The users it mentions, in Discord's order: a system message's other person.
    pub mentions: Vec<User>,
    /// A CALL or VOICE_SESSION message's call.
    pub call: Option<MessageCall>,
    pub role_subscription: Option<RoleSubscription>,
    /// A PURCHASE_NOTIFICATION's product.
    pub purchased_product: Option<String>,
    /// Who added a bot, on the bot's USER_JOIN.
    pub actor: Option<User>,
    /// The app a GUILD_APPLICATION_PREMIUM_SUBSCRIPTION upgraded.
    pub application_name: Option<String>,
    /// A poll result's or an AutoMod alert's first embed's fields, which their lines read.
    pub embed_fields: Vec<EmbedField>,
    /// The first embed's title, which names a thread started from the message.
    #[uniffi(default = None)]
    pub embed_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageCall {
    /// The recipients who joined it.
    pub participants: Vec<UserId>,
    /// `None` while it runs.
    pub ended_timestamp: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RoleSubscription {
    pub tier_name: String,
    pub total_months_subscribed: u32,
    pub is_renewal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MessageReference {
    pub message_id: Option<MessageId>,
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
}

/// A thread's first message when it started from a message in its parent channel.
// Copied across once per read; UniFFI can't lower a boxed record.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum StarterMessage {
    /// The parent channel's message the thread started from.
    Loaded { message: Message },
    /// Akari is fetching it; nothing to show yet. `MessageUpdated` for the thread's first
    /// message says when to read it again.
    Loading,
    /// Deleted, or the user can't read it.
    Unavailable,
}

impl Message {
    pub(crate) fn new(
        message: &state::Message,
        formatted: FormattedText,
        mentions_me: bool,
    ) -> Self {
        let system = message.system.as_deref();
        Self {
            id: message.id,
            channel_id: message.channel_id,
            kind: message.kind,
            author: User::from(&*message.author),
            from_webhook: message.webhook_id.is_some(),
            content: message.content.to_string(),
            timestamp: system_time(message.timestamp),
            edited_timestamp: message.edited_timestamp.map(system_time),
            pinned: message.pinned,
            mention_everyone: message.mention_everyone,
            attachments: message.attachments.iter().map(Attachment::from).collect(),
            embed_count: u32::try_from(message.embeds.len()).unwrap_or(u32::MAX),
            stickers: message.stickers.iter().map(Sticker::from).collect(),
            components_v2: message.uses_components_v2(),
            delivery: message.delivery.into(),
            formatted,
            mentions_me,
            reference: message
                .reference
                .as_deref()
                .map(|reference| MessageReference {
                    message_id: reference.message_id,
                    channel_id: reference.channel_id,
                    guild_id: reference.guild_id,
                }),
            has_thread: message.has_thread(),
            mentions: message
                .mentions
                .iter()
                .map(|user| User::from(&**user))
                .collect(),
            call: system
                .and_then(|system| system.call.as_ref())
                .map(|call| MessageCall {
                    participants: call.participants.to_vec(),
                    ended_timestamp: call.ended_timestamp.map(system_time),
                }),
            role_subscription: system
                .and_then(|system| system.role_subscription.as_ref())
                .map(|data| RoleSubscription {
                    tier_name: data.tier_name.to_string(),
                    total_months_subscribed: data.total_months_subscribed,
                    is_renewal: data.is_renewal,
                }),
            purchased_product: system
                .and_then(|system| system.purchased_product.as_deref())
                .map(str::to_owned),
            actor: system
                .and_then(|system| system.actor.as_deref())
                .map(User::from),
            application_name: system
                .and_then(|system| system.application_name.as_deref())
                .map(str::to_owned),
            embed_fields: message
                .system_embed_fields()
                .iter()
                .map(|field| EmbedField {
                    name: field.name.to_string(),
                    value: field.value.to_string(),
                })
                .collect(),
            embed_title: message
                .embeds
                .first()
                .and_then(|embed| embed.title.as_deref())
                .map(str::to_owned),
        }
    }
}

fn system_time(timestamp: Timestamp) -> SystemTime {
    let millis = timestamp.unix_millis();
    let offset = Duration::from_millis(millis.unsigned_abs());
    if millis >= 0 {
        UNIX_EPOCH + offset
    } else {
        UNIX_EPOCH - offset
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Attachment {
    pub id: AttachmentId,
    pub filename: String,
    pub content_type: Option<String>,
    /// Bytes.
    pub size: u64,
}

impl From<&state::Attachment> for Attachment {
    fn from(attachment: &state::Attachment) -> Self {
        Self {
            id: attachment.id,
            filename: attachment.filename.to_string(),
            content_type: attachment.content_type.as_deref().map(str::to_owned),
            size: attachment.size,
        }
    }
}

/// A sticker a message carries; `key` is its image (a Lottie sticker has no image Akari shows).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Sticker {
    pub id: StickerId,
    pub name: String,
    pub format: StickerFormatType,
    pub key: ImageKey,
}

impl From<&state::Sticker> for Sticker {
    fn from(sticker: &state::Sticker) -> Self {
        Self {
            id: sticker.id,
            name: sticker.name.to_string(),
            format: sticker.format,
            key: ImageKey::Sticker {
                sticker_id: sticker.id,
                format: sticker.format,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Delivery {
    Sent,
    /// Sent, not confirmed yet; the ID is provisional.
    Pending,
    /// Discord didn't take it; retry or discard it.
    Failed,
}

impl From<state::Delivery> for Delivery {
    fn from(delivery: state::Delivery) -> Self {
        match delivery {
            state::Delivery::Pending => Self::Pending,
            state::Delivery::Failed => Self::Failed,
            _ => Self::Sent,
        }
    }
}

/// A channel's slowmode as it applies to the current user.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Slowmode {
    pub interval: Duration,
    /// The user's permissions bypass it.
    pub exempt: bool,
    /// When the user may send again; `None` when they may now.
    pub until: Option<SystemTime>,
}

/// Which threads the user may create in a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct ThreadCreation {
    pub public: bool,
    /// Only in text channels.
    pub private: bool,
}

impl From<state::ThreadCreation> for ThreadCreation {
    fn from(creation: state::ThreadCreation) -> Self {
        Self {
            public: creation.public,
            private: creation.private,
        }
    }
}

impl From<state::Slowmode> for Slowmode {
    fn from(slowmode: state::Slowmode) -> Self {
        Self {
            interval: slowmode.interval,
            exempt: slowmode.exempt,
            until: slowmode.until,
        }
    }
}

/// The message IDs and state of a viewed channel.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageWindow {
    /// Oldest first, without gaps.
    pub message_ids: Vec<MessageId>,
    /// Our pending and failed messages, in send order, shown after `message_ids`.
    pub pending_ids: Vec<MessageId>,
    /// Ends with the channel's newest message.
    pub latest: bool,
    /// Starts with the channel's first message.
    pub oldest: bool,
    /// A new session may have missed changes; a refresh is under way.
    pub stale: bool,
}

impl From<&state::MessageWindow> for MessageWindow {
    fn from(window: &state::MessageWindow) -> Self {
        let ids =
            |messages: &[Arc<state::Message>]| messages.iter().map(|message| message.id).collect();
        Self {
            message_ids: ids(&window.messages),
            pending_ids: ids(&window.pending),
            latest: window.latest,
            oldest: window.oldest,
            stale: window.stale,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    /// Before `connect()`, or after `disconnect()`.
    Offline,
    /// Connecting, resuming, or waiting to reconnect.
    Connecting,
    /// A session is ready.
    Online,
    /// Closed by `close()`, or by a fatal error; `AuthenticationFailed` means the user has to
    /// log in again.
    Closed { error: Option<GatewayError> },
}

impl From<&state::ConnectionState> for ConnectionState {
    fn from(state: &state::ConnectionState) -> Self {
        match state {
            state::ConnectionState::Offline => Self::Offline,
            state::ConnectionState::Connecting => Self::Connecting,
            state::ConnectionState::Online => Self::Online,
            state::ConnectionState::Closed { error } => Self::Closed {
                error: error.as_deref().map(GatewayError::from),
            },
            _ => Self::Offline,
        }
    }
}

/// The kind of channel, as akari-core decodes it.
#[uniffi::remote(Enum)]
pub enum ChannelType {
    GuildText,
    Dm,
    GuildVoice,
    GroupDm,
    GuildCategory,
    GuildNews,
    GuildStore,
    NewsThread,
    PublicThread,
    PrivateThread,
    GuildStageVoice,
    GuildDirectory,
    GuildForum,
    GuildMedia,
    Lobby,
    EphemeralDm,
    Unknown(u16),
}

/// The kind of message. Everything but `Default` and `Reply` is a system message or a
/// special layout.
#[uniffi::remote(Enum)]
pub enum MessageType {
    Default,
    RecipientAdd,
    RecipientRemove,
    Call,
    ChannelNameChange,
    ChannelIconChange,
    ChannelPinnedMessage,
    UserJoin,
    PremiumGuildSubscription,
    PremiumGuildSubscriptionTier1,
    PremiumGuildSubscriptionTier2,
    PremiumGuildSubscriptionTier3,
    ChannelFollowAdd,
    GuildDiscoveryDisqualified,
    GuildDiscoveryRequalified,
    GuildDiscoveryGracePeriodInitialWarning,
    GuildDiscoveryGracePeriodFinalWarning,
    ThreadCreated,
    Reply,
    ChatInputCommand,
    ThreadStarterMessage,
    GuildInviteReminder,
    ContextMenuCommand,
    AutoModerationAction,
    RoleSubscriptionPurchase,
    InteractionPremiumUpsell,
    StageStart,
    StageEnd,
    StageSpeaker,
    StageRaiseHand,
    StageTopic,
    GuildApplicationPremiumSubscription,
    PremiumReferral,
    GuildIncidentAlertModeEnabled,
    GuildIncidentAlertModeDisabled,
    GuildIncidentReportRaid,
    GuildIncidentReportFalseAlarm,
    GuildDeadchatRevivePrompt,
    CustomGift,
    GuildGamingStatsPrompt,
    PurchaseNotification,
    PollResult,
    Changelog,
    NitroNotification,
    ChannelLinkedToLobby,
    GiftingPrompt,
    InGameMessageNux,
    GuildJoinRequestAcceptNotification,
    GuildJoinRequestRejectNotification,
    GuildJoinRequestWithdrawnNotification,
    HdStreamingUpgraded,
    ReportToModDeletedMessage,
    ReportToModTimeoutUser,
    ReportToModKickUser,
    ReportToModBanUser,
    ReportToModClosedReport,
    PremiumGroupInvite,
    VoiceSession,
    GuildBoostUpsell,
    FriendRequestAccepted,
    MediaMentionMessage,
    Unknown(u16),
}

/// What became of a history load's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PageOutcome {
    /// It went into the window.
    Applied,
    /// Dropped for a later-placed request: an `Around` page for a `Latest`, `Around` or
    /// supersede; a `Latest` page that would replace the window for an `Around`.
    Overtaken,
    /// Dropped: the window was replaced, or the end the page continues from changed, meanwhile.
    Outdated,
}

impl From<akari_core::PageOutcome> for PageOutcome {
    fn from(outcome: akari_core::PageOutcome) -> Self {
        match outcome {
            akari_core::PageOutcome::Applied => Self::Applied,
            akari_core::PageOutcome::Overtaken => Self::Overtaken,
            _ => Self::Outdated,
        }
    }
}

/// Where one history load's time went, and what became of its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct LoadReport {
    pub outcome: PageOutcome,
    /// Behind earlier requests on the same channel (they run one at a time) or a warm-up.
    pub queued: Duration,
    /// Waiting for Discord's limits: a bucket reset, a 429 pause or the global limit.
    pub limited: Duration,
    /// From sending until the response headers, over all attempts.
    pub headers: Duration,
    /// A connection was opened for it (DNS, TCP, TLS). Approximate when requests overlap.
    pub new_connection: bool,
    pub body: Duration,
    /// After decompression.
    pub bytes: u32,
    pub decode: Duration,
    /// Into the window, under the store's write lock.
    pub store: Duration,
    /// Messages in the page, including ones that didn't decode.
    pub received: u16,
    /// 429 and 5xx answers that were retried.
    pub retries: u8,
    /// `X-RateLimit-Remaining` and `-Reset-After` of the last answer, when Discord sent them.
    pub remaining: Option<u32>,
    pub reset_after: Option<Duration>,
    /// Parsing the page's new messages on the runtime, and on a first page the disk cache's
    /// messages shown before it.
    pub parse: Duration,
    /// Messages parsed for this load; ones parsed before aren't parsed again.
    pub parsed: u32,
}

impl From<akari_core::LoadReport> for LoadReport {
    fn from(report: akari_core::LoadReport) -> Self {
        Self {
            outcome: report.outcome.into(),
            queued: report.queued,
            limited: report.limited,
            headers: report.headers,
            new_connection: report.new_connection,
            body: report.body,
            bytes: report.bytes,
            decode: report.decode,
            store: report.store,
            received: report.received,
            retries: report.retries,
            remaining: report.remaining,
            reset_after: report.reset_after,
            parse: report.prepare,
            parsed: report.prepared,
        }
    }
}

/// How an account's message contents were parsed. Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ContentStats {
    pub parsed: u64,
    /// Parsed when read though a load had parsed them: the cache dropped them since.
    pub misses: u64,
    /// Parsed when read because no load did: live messages and edits, on the reader's thread.
    pub event_parses: u64,
    /// Documents held now.
    pub cached: u32,
}
