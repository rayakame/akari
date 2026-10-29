//! Discord objects as the API sends them.

mod channel;
mod expression;
mod guild;
mod int_enum;
mod message;
mod permissions;
mod snowflake;
mod timestamp;
mod user;

pub use channel::{
    Channel, ChannelType, DefaultReaction, ForumTag, OverwriteType, PermissionOverwrite, PostData,
    PostDataEntry, ThreadMember, ThreadMetadata, ThreadMuteConfig, ThreadSearchResults,
};
pub use expression::{
    EmojiSource, EmojiSourceApplication, EmojiSourceGuild, Sticker, StickerGuild, StickerPack,
    StickerType,
};
pub use guild::{
    EmojiRef, Guild, GuildEmoji, GuildMember, MessageNotificationLevel, NsfwLevel, PremiumTier,
    Role, RoleColors,
};
pub use message::{
    Attachment, Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedMedia, EmbedProvider,
    GuildProductPurchase, Message, MessageApplication, MessageCall, MessageReference,
    MessageReferenceType, MessageType, Nonce, PartialEmoji, PurchaseNotification, Reaction,
    ReactionCountDetails, RoleSubscriptionData, StickerFormatType, StickerItem,
};
pub use permissions::Permissions;
pub(crate) use snowflake::DISCORD_EPOCH;
pub use snowflake::{
    AttachmentMarker, ChannelMarker, EmojiMarker, GenericMarker, GuildMarker, MessageMarker,
    RoleMarker, SkuMarker, Snowflake, StickerMarker, StickerPackMarker, TagMarker, UserMarker,
    WebhookMarker,
};
pub use timestamp::Timestamp;
#[cfg(test)]
pub(crate) use timestamp::iso;
pub use user::{
    AvatarDecorationData, CurrentUser, PremiumType, PrimaryGuild, RelationshipType, User,
};

pub type AttachmentId = Snowflake<AttachmentMarker>;
pub type ChannelId = Snowflake<ChannelMarker>;
pub type EmojiId = Snowflake<EmojiMarker>;
pub type GuildId = Snowflake<GuildMarker>;
pub type MessageId = Snowflake<MessageMarker>;
pub type RoleId = Snowflake<RoleMarker>;
pub type StickerId = Snowflake<StickerMarker>;
pub type StickerPackId = Snowflake<StickerPackMarker>;
pub type TagId = Snowflake<TagMarker>;
pub type UserId = Snowflake<UserMarker>;
pub type WebhookId = Snowflake<WebhookMarker>;

pub(crate) fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(deserializer).map(Some)
}
