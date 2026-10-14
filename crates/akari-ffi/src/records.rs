use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use akari_core::model::{
    AttachmentId, ChannelId, ChannelType, GuildId, MessageId, MessageType, Timestamp, UserId,
};
use akari_core::state;

use crate::errors::GatewayError;
use crate::formatted::FormattedText;

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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Guild {
    pub id: GuildId,
    pub name: String,
}

impl From<&state::Guild> for Guild {
    fn from(guild: &state::Guild) -> Self {
        Self {
            id: guild.id,
            name: guild.name.to_string(),
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
    pub sticker_names: Vec<String>,
    /// Laid out with Components V2, which Akari can't show yet; such a message has no content.
    pub components_v2: bool,
    pub delivery: Delivery,
    pub formatted: FormattedText,
    /// Mentions the current user, by ID, @everyone or @here, or a role of theirs. The last
    /// three don't count where the server suppresses them.
    pub mentions_me: bool,
}

impl Message {
    pub(crate) fn new(
        message: &state::Message,
        formatted: FormattedText,
        mentions_me: bool,
    ) -> Self {
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
            sticker_names: message
                .stickers
                .iter()
                .map(|sticker| sticker.name.to_string())
                .collect(),
            components_v2: message.uses_components_v2(),
            delivery: message.delivery.into(),
            formatted,
            mentions_me,
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

/// Where one history load's time went. Numbers only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct LoadReport {
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
