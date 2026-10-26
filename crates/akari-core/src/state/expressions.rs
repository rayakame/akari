//! What a custom emoji's or a sticker's popout shows about where it comes from, as the account's
//! lookups answer it.

use crate::media::ImageKey;
use crate::model::{self, EmojiId, GuildId, StickerPackId};

use super::types::{ImageHash, Sticker};

/// Where a custom emoji comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmojiSource {
    /// A discoverable server (Discord answers no other).
    Guild(SourceGuild),
    /// An app's own emoji.
    Application { name: Box<str> },
    /// A kind Akari doesn't know, such as a Nitro emoji pack.
    Unknown,
}

/// A server an emoji or a sticker is from, with what its popout shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceGuild {
    pub id: GuildId,
    pub name: Box<str>,
    pub icon: Option<ImageHash>,
    /// Listed in Server Discovery: others can find and preview it.
    pub discoverable: bool,
    pub verified: bool,
    pub partnered: bool,
    /// Members online, as Discord counts them roughly.
    pub presence_count: Option<u32>,
    /// Its emoji, for an emoji's source.
    pub emojis: Box<[SourceEmoji]>,
    /// Its stickers, for a sticker's server.
    pub stickers: Box<[Sticker]>,
}

impl SourceGuild {
    pub fn icon_key(&self) -> Option<ImageKey> {
        self.icon
            .as_ref()
            .filter(|hash| hash.is_cdn_hash())
            .map(|hash| ImageKey::GuildIcon {
                guild: self.id,
                hash: hash.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceEmoji {
    pub id: EmojiId,
    pub name: Box<str>,
    pub animated: bool,
}

impl SourceEmoji {
    pub fn image_key(&self) -> ImageKey {
        ImageKey::Emoji {
            id: self.id,
            animated: self.animated,
        }
    }
}

/// A sticker and where it's from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StickerDetails {
    pub sticker: Sticker,
    pub origin: StickerOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StickerOrigin {
    /// One of Discord's packs.
    Pack(StickerPackId),
    Guild(GuildId),
    Unknown,
}

/// One of Discord's sticker packs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StickerPack {
    pub id: StickerPackId,
    pub name: Box<str>,
    pub stickers: Box<[Sticker]>,
}

fn feature(features: &[String], name: &str) -> bool {
    features.iter().any(|feature| feature == name)
}

fn icon(hash: Option<String>) -> Option<ImageHash> {
    hash.as_deref().map(ImageHash::parse)
}

fn sticker(wire: model::Sticker) -> Sticker {
    Sticker {
        id: wire.id,
        name: wire.name.into(),
        format: wire.format_type,
    }
}

impl From<model::EmojiSource> for EmojiSource {
    fn from(wire: model::EmojiSource) -> Self {
        match (wire.kind.as_str(), wire.guild, wire.application) {
            ("GUILD", Some(guild), _) => Self::Guild(SourceGuild {
                id: guild.id,
                name: guild.name.into(),
                icon: icon(guild.icon),
                discoverable: feature(&guild.features, "DISCOVERABLE"),
                verified: feature(&guild.features, "VERIFIED"),
                partnered: feature(&guild.features, "PARTNERED"),
                presence_count: guild.approximate_presence_count,
                emojis: guild
                    .emojis
                    .into_iter()
                    .filter_map(|emoji| {
                        Some(SourceEmoji {
                            id: emoji.id,
                            name: emoji.name?.into(),
                            animated: emoji.animated,
                        })
                    })
                    .collect(),
                stickers: Box::new([]),
            }),
            ("APPLICATION", _, Some(application)) => Self::Application {
                name: application.name.into(),
            },
            _ => Self::Unknown,
        }
    }
}

impl From<model::StickerGuild> for SourceGuild {
    fn from(wire: model::StickerGuild) -> Self {
        Self {
            id: wire.id,
            name: wire.name.into(),
            icon: icon(wire.icon),
            discoverable: feature(&wire.features, "DISCOVERABLE"),
            verified: feature(&wire.features, "VERIFIED"),
            partnered: feature(&wire.features, "PARTNERED"),
            presence_count: wire.approximate_presence_count,
            emojis: Box::new([]),
            stickers: wire.stickers.into_iter().map(sticker).collect(),
        }
    }
}

impl From<model::Sticker> for StickerDetails {
    fn from(wire: model::Sticker) -> Self {
        let origin = match (wire.kind, wire.guild_id, wire.pack_id) {
            (model::StickerType::Guild, Some(guild), _) => StickerOrigin::Guild(guild),
            (model::StickerType::Standard, _, Some(pack)) => StickerOrigin::Pack(pack),
            (_, Some(guild), _) => StickerOrigin::Guild(guild),
            (_, _, Some(pack)) => StickerOrigin::Pack(pack),
            _ => StickerOrigin::Unknown,
        };
        Self {
            sticker: sticker(wire),
            origin,
        }
    }
}

impl From<model::StickerPack> for StickerPack {
    fn from(wire: model::StickerPack) -> Self {
        Self {
            id: wire.id,
            name: wire.name.into(),
            stickers: wire.stickers.into_iter().map(sticker).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn answers_discord_leaves_half_empty_read_as_unknown() {
        let guildless: model::EmojiSource =
            serde_json::from_value(json!({"type": "GUILD", "guild": null, "application": null}))
                .unwrap();
        assert_eq!(EmojiSource::from(guildless), EmojiSource::Unknown);
        let pack: model::EmojiSource =
            serde_json::from_value(json!({"type": "PACK", "guild": null, "application": null}))
                .unwrap();
        assert_eq!(EmojiSource::from(pack), EmojiSource::Unknown);
        let nameless = serde_json::from_value::<model::EmojiSource>(
            json!({"type": "APPLICATION", "guild": null, "application": {"id": "1"}}),
        );
        assert!(nameless.is_err(), "an app without a name doesn't decode");
    }

    #[test]
    fn a_stickers_origin_follows_its_ids() {
        let sticker = |kind: u8, guild: Option<&str>, pack: Option<&str>| -> StickerOrigin {
            let wire: model::Sticker = serde_json::from_value(json!({
                "id": "7", "name": "wave", "type": kind, "format_type": 1,
                "guild_id": guild, "pack_id": pack
            }))
            .unwrap();
            StickerDetails::from(wire).origin
        };
        assert_eq!(
            sticker(9, Some("5"), None),
            StickerOrigin::Guild(GuildId::new(5))
        );
        assert_eq!(
            sticker(9, None, Some("6")),
            StickerOrigin::Pack(StickerPackId::new(6))
        );
        assert_eq!(sticker(2, None, None), StickerOrigin::Unknown);
        assert_eq!(sticker(1, None, None), StickerOrigin::Unknown);
    }
}
