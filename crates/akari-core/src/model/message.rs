use serde::{Deserialize, Deserializer};

use super::guild::GuildMember;
use super::int_enum::int_enum;
use super::snowflake::{
    AttachmentMarker, ChannelMarker, EmojiMarker, GuildMarker, MessageMarker, RoleMarker,
    Snowflake, StickerMarker, WebhookMarker,
};
use super::timestamp::Timestamp;
use super::user::User;

/// A message from REST or from MESSAGE_CREATE / MESSAGE_UPDATE.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Message {
    pub id: Snowflake<MessageMarker>,
    pub channel_id: Snowflake<ChannelMarker>,
    /// Only in gateway events, and only for guild messages.
    pub guild_id: Option<Snowflake<GuildMarker>>,
    /// Not a real user when `webhook_id` is set.
    pub author: User,
    /// The author's membership without `user`; only in gateway events for guild messages.
    pub member: Option<GuildMember>,
    #[serde(default)]
    pub content: String,
    pub timestamp: Timestamp,
    pub edited_timestamp: Option<Timestamp>,
    #[serde(default)]
    pub tts: bool,
    #[serde(default)]
    pub mention_everyone: bool,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub mention_roles: Vec<Snowflake<RoleMarker>>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub sticker_items: Vec<StickerItem>,
    #[serde(default)]
    pub pinned: bool,
    pub webhook_id: Option<Snowflake<WebhookMarker>>,
    #[serde(rename = "type")]
    pub kind: MessageType,
    #[serde(default)]
    pub flags: u64,
    pub message_reference: Option<MessageReference>,
    /// `None` if Discord didn't include the referenced message, `Some(None)` if it was deleted.
    #[serde(default, deserialize_with = "double_option")]
    pub referenced_message: Option<Option<Box<Message>>>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Attachment {
    pub id: Snowflake<AttachmentMarker>,
    pub filename: String,
    pub title: Option<String>,
    /// Alt text.
    pub description: Option<String>,
    pub content_type: Option<String>,
    /// Bytes.
    pub size: u64,
    pub url: String,
    pub proxy_url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// A thumbhash to show while the file loads.
    pub placeholder: Option<String>,
    /// Voice messages only.
    pub duration_secs: Option<f64>,
    /// Voice messages only: base64-encoded samples.
    pub waveform: Option<String>,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Embed {
    /// `rich`, `image`, `video`, `gifv`, `article`, `link` and others.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub timestamp: Option<Timestamp>,
    pub color: Option<u32>,
    pub footer: Option<EmbedFooter>,
    pub image: Option<EmbedMedia>,
    pub thumbnail: Option<EmbedMedia>,
    pub video: Option<EmbedMedia>,
    pub provider: Option<EmbedProvider>,
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmbedMedia {
    pub url: String,
    pub proxy_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub content_type: Option<String>,
    pub placeholder: Option<String>,
    /// Alt text.
    pub description: Option<String>,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmbedProvider {
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmbedAuthor {
    pub name: String,
    pub url: Option<String>,
    pub icon_url: Option<String>,
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmbedFooter {
    pub text: String,
    pub icon_url: Option<String>,
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Reaction {
    pub emoji: PartialEmoji,
    /// Normal and burst reactions together.
    pub count: u32,
    pub count_details: ReactionCountDetails,
    /// Whether the current user reacted normally.
    pub me: bool,
    /// Whether the current user burst-reacted.
    pub me_burst: bool,
    /// Hex colors such as `#f0ca59` for the burst animation.
    #[serde(default)]
    pub burst_colors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ReactionCountDetails {
    pub normal: u32,
    pub burst: u32,
}

/// A Unicode emoji (no `id`) or a custom emoji. A deleted custom emoji has no `name`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartialEmoji {
    pub id: Option<Snowflake<EmojiMarker>>,
    pub name: Option<String>,
    #[serde(default)]
    pub animated: bool,
}

/// The source of a reply, pin, crosspost, thread starter or forward.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MessageReference {
    #[serde(rename = "type", default)]
    pub kind: MessageReferenceType,
    pub message_id: Option<Snowflake<MessageMarker>>,
    pub channel_id: Option<Snowflake<ChannelMarker>>,
    pub guild_id: Option<Snowflake<GuildMarker>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StickerItem {
    pub id: Snowflake<StickerMarker>,
    pub name: String,
    pub format_type: StickerFormatType,
}

int_enum! {
    /// The kind of message. Everything but `Default` and `Reply` is rendered as a system
    /// message or a special layout.
    pub enum MessageType {
        Default = 0,
        RecipientAdd = 1,
        RecipientRemove = 2,
        Call = 3,
        ChannelNameChange = 4,
        ChannelIconChange = 5,
        ChannelPinnedMessage = 6,
        UserJoin = 7,
        PremiumGuildSubscription = 8,
        PremiumGuildSubscriptionTier1 = 9,
        PremiumGuildSubscriptionTier2 = 10,
        PremiumGuildSubscriptionTier3 = 11,
        ChannelFollowAdd = 12,
        GuildDiscoveryDisqualified = 14,
        GuildDiscoveryRequalified = 15,
        GuildDiscoveryGracePeriodInitialWarning = 16,
        GuildDiscoveryGracePeriodFinalWarning = 17,
        ThreadCreated = 18,
        Reply = 19,
        ChatInputCommand = 20,
        ThreadStarterMessage = 21,
        GuildInviteReminder = 22,
        ContextMenuCommand = 23,
        AutoModerationAction = 24,
        RoleSubscriptionPurchase = 25,
        InteractionPremiumUpsell = 26,
        StageStart = 27,
        StageEnd = 28,
        StageSpeaker = 29,
        StageRaiseHand = 30,
        StageTopic = 31,
        GuildApplicationPremiumSubscription = 32,
        PremiumReferral = 35,
        GuildIncidentAlertModeEnabled = 36,
        GuildIncidentAlertModeDisabled = 37,
        GuildIncidentReportRaid = 38,
        GuildIncidentReportFalseAlarm = 39,
        GuildDeadchatRevivePrompt = 40,
        CustomGift = 41,
        GuildGamingStatsPrompt = 42,
        PurchaseNotification = 44,
        PollResult = 46,
        Changelog = 47,
        NitroNotification = 48,
        ChannelLinkedToLobby = 49,
        GiftingPrompt = 50,
        InGameMessageNux = 51,
        GuildJoinRequestAcceptNotification = 52,
        GuildJoinRequestRejectNotification = 53,
        GuildJoinRequestWithdrawnNotification = 54,
        HdStreamingUpgraded = 55,
        ReportToModDeletedMessage = 58,
        ReportToModTimeoutUser = 59,
        ReportToModKickUser = 60,
        ReportToModBanUser = 61,
        ReportToModClosedReport = 62,
        PremiumGroupInvite = 64,
        VoiceSession = 65,
        GuildBoostUpsell = 66,
        FriendRequestAccepted = 67,
        MediaMentionMessage = 68,
    }
}

int_enum! {
    /// Whether a message reference is a reply-like link or a forward.
    #[derive(Default)]
    pub enum MessageReferenceType {
        #[default]
        Default = 0,
        Forward = 1,
    }
}

int_enum! {
    pub enum StickerFormatType {
        Png = 1,
        Apng = 2,
        Lottie = 3,
        Gif = 4,
    }
}
