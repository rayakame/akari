use serde::Deserialize;

use super::guild::GuildEmoji;
use super::int_enum::int_enum;
use super::message::StickerFormatType;
use super::snowflake::{GenericMarker, GuildMarker, Snowflake, StickerMarker, StickerPackMarker};
use crate::lenient::skip_invalid;

/// GET /emojis/{emoji.id}/source: the guild or application that owns an emoji. A guild is only
/// answered while it is discoverable.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmojiSource {
    /// `GUILD` or `APPLICATION`.
    #[serde(rename = "type")]
    pub kind: String,
    pub guild: Option<EmojiSourceGuild>,
    pub application: Option<EmojiSourceApplication>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmojiSourceGuild {
    pub id: Snowflake<GuildMarker>,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub emojis: Vec<GuildEmoji>,
    pub approximate_presence_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmojiSourceApplication {
    pub id: Snowflake<GenericMarker>,
    pub name: String,
}

int_enum! {
    pub enum StickerType {
        /// From one of Discord's packs.
        Standard = 1,
        Guild = 2,
    }
}

/// A sticker object, as GET /stickers/{sticker.id} and the lists that carry stickers send it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Sticker {
    pub id: Snowflake<StickerMarker>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: StickerType,
    pub format_type: StickerFormatType,
    pub pack_id: Option<Snowflake<StickerPackMarker>>,
    pub guild_id: Option<Snowflake<GuildMarker>>,
    /// Guild stickers only: `false` after the guild lost the boosts it needs.
    pub available: Option<bool>,
}

/// GET /stickers/{sticker.id}/guild: the discoverable guild that owns a sticker.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StickerGuild {
    pub id: Snowflake<GuildMarker>,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    pub approximate_presence_count: Option<u32>,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub stickers: Vec<Sticker>,
}

/// GET /sticker-packs/{pack.id}.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StickerPack {
    pub id: Snowflake<StickerPackMarker>,
    pub name: String,
    #[serde(default, deserialize_with = "skip_invalid")]
    pub stickers: Vec<Sticker>,
}
