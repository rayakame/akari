//! Discord objects as the API sends them.

mod channel;
mod guild;
mod int_enum;
mod message;
mod permissions;
mod snowflake;
mod timestamp;
mod user;

pub use channel::{Channel, ChannelType, OverwriteType, PermissionOverwrite, ThreadMetadata};
pub use guild::{
    Guild, GuildMember, MessageNotificationLevel, NsfwLevel, PremiumTier, Role, RoleColors,
};
pub use message::{
    Attachment, Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedMedia, EmbedProvider, Message,
    MessageReference, MessageReferenceType, MessageType, Nonce, PartialEmoji, Reaction,
    ReactionCountDetails, StickerFormatType, StickerItem,
};
pub use permissions::Permissions;
pub use snowflake::{
    AttachmentMarker, ChannelMarker, EmojiMarker, GenericMarker, GuildMarker, MessageMarker,
    RoleMarker, SkuMarker, Snowflake, StickerMarker, UserMarker, WebhookMarker,
};
pub use timestamp::Timestamp;
pub use user::{AvatarDecorationData, CurrentUser, PremiumType, PrimaryGuild, User};

pub type AttachmentId = Snowflake<AttachmentMarker>;
pub type ChannelId = Snowflake<ChannelMarker>;
pub type GuildId = Snowflake<GuildMarker>;
pub type MessageId = Snowflake<MessageMarker>;
pub type RoleId = Snowflake<RoleMarker>;
pub type StickerId = Snowflake<StickerMarker>;
pub type UserId = Snowflake<UserMarker>;
pub type WebhookId = Snowflake<WebhookMarker>;

pub(crate) fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(deserializer).map(Some)
}
