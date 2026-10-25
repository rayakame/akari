//! UniFFI bindings over `akari-core` and `akari-markdown` for Swift and Kotlin.

mod account;
mod client;
mod contents;
mod errors;
mod formatted;
mod ids;
mod images;
mod logging;
mod login;
mod names;
mod records;
mod runtime;
mod store;
mod subscription;
mod token_store;

#[cfg(test)]
mod tests;

pub use account::{Account, MessageLoad};
pub use client::{DiscordClient, Endpoints, HostInfo, Token, discord_endpoints};
pub use errors::{
    ClientError, GatewayError, ImageError, LoginError, LogoutError, NetworkErrorKind, RequestError,
    TokenStoreError,
};
pub use formatted::{
    ChannelLink, FormattedText, GuildNavigation, MentionName, SpanKind, TextBlock, TextBlockKind,
    TextContainer, TextLink, TextSpan, TimestampStyle, formatted_text,
};
pub use images::{
    EmbedSlot, GalleryColumns, ImageFile, ImageKey, ImageRequest, MediaMosaic, MediaRect,
    MediaSize, PendingImage, PendingSave, PlaceholderImage, embed_image_size, embed_thumbnail_size,
    gallery_card_height, gallery_columns, media_box, media_contain, media_cover, media_fit_width,
    media_mosaic, placeholder_image, sticker_size,
};
pub use logging::enable_logging;
pub use login::{
    CaptchaChallenge, LoginStep, LoginSuccess, MfaChallenge, MfaMethod, NewLocation, PasswordLogin,
    QrEvent, QrLogin, ScannedUser,
};
pub use records::{
    Attachment, Channel, ConnectionState, ContentStats, Delivery, Guild, Message, MessageReference,
    MessageWindow, Slowmode, StarterMessage, User,
};
pub use store::{Store, message_length};
pub use subscription::{StoreEvent, StoreSubscription};
pub use token_store::TokenStore;

uniffi::setup_scaffolding!();
