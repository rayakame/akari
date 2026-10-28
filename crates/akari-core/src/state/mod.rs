//! The account's state as a UI reads it, kept current by the gateway.

pub(crate) mod apply;
mod badges;
mod clock;
mod convert;
mod embeds;
mod events;
mod expressions;
mod media;
mod order;
mod permissions;
mod read_states;
mod send;
mod settings;
mod store;
mod threads;
mod types;
mod windows;

pub(crate) use apply::{Learned, OLDER_PAGE, OlderPage, PageKey, wall_millis};
pub use badges::Badge;
pub use embeds::{
    EmbedKind, EmbedMediaRef, EmbedTextRules, ShownEmbed, ShownVideo, embed_text_rules,
};
pub use events::{ConnectionState, StoreEvent};
pub use expressions::{
    EmojiSource, SourceEmoji, SourceGuild, StickerDetails, StickerOrigin, StickerPack,
};
pub(crate) use media::AgeAgreements;
pub use media::{AgeGate, AttachmentDisplay, MediaCover, ShownAttachment};
pub use order::display_order;
pub use read_states::ReadState;
pub(crate) use read_states::{Ack, AckBody, ReadStates};
pub(crate) use send::limit_in;
pub use send::{Slowmode, message_length};
pub use settings::{
    ChannelSettings, DisplaySettings, ExplicitContentFilter, ExplicitContentSettings, Folder,
    GuildSettings, NotificationLevel, Redaction, ServerListItem, SpoilerSetting, StickerAnimation,
    Unreads,
};
pub(crate) use settings::{FolderEntry, UserSettings};
pub use store::{Store, Subscription};
#[cfg(test)]
pub(crate) use store::{deadline_reads, deadline_reads_reset};
pub use types::*;
pub use windows::MessageWindow;
pub(crate) use windows::{
    CachedWindow, Cursor, DEFAULT_LIMITS, LoadKind, LoadTicket, StarterFetch, WindowLimits,
};
