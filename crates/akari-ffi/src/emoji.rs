use akari_core::emoji;
use akari_core::model::{EmojiId, GuildId};
use akari_core::state;

use crate::images::ImageKey;
use crate::records::ReactionEmoji;

/// An emoji version (UTS #51): when an emoji came. The host gives the newest its emoji font
/// draws, and newer emoji are left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct EmojiVersion {
    pub major: u16,
    pub minor: u16,
}

impl From<EmojiVersion> for emoji::EmojiVersion {
    fn from(version: EmojiVersion) -> Self {
        Self::new(version.major, version.minor)
    }
}

impl From<emoji::EmojiVersion> for EmojiVersion {
    fn from(version: emoji::EmojiVersion) -> Self {
        Self {
            major: version.major,
            minor: version.minor,
        }
    }
}

/// A CLDR emoji group, the picker's categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EmojiGroup {
    SmileysAndEmotion,
    PeopleAndBody,
    AnimalsAndNature,
    FoodAndDrink,
    TravelAndPlaces,
    Activities,
    Objects,
    Symbols,
    Flags,
}

impl From<emoji::EmojiGroup> for EmojiGroup {
    fn from(group: emoji::EmojiGroup) -> Self {
        match group {
            emoji::EmojiGroup::SmileysAndEmotion => Self::SmileysAndEmotion,
            emoji::EmojiGroup::PeopleAndBody => Self::PeopleAndBody,
            emoji::EmojiGroup::AnimalsAndNature => Self::AnimalsAndNature,
            emoji::EmojiGroup::FoodAndDrink => Self::FoodAndDrink,
            emoji::EmojiGroup::TravelAndPlaces => Self::TravelAndPlaces,
            emoji::EmojiGroup::Activities => Self::Activities,
            emoji::EmojiGroup::Objects => Self::Objects,
            emoji::EmojiGroup::Symbols => Self::Symbols,
            emoji::EmojiGroup::Flags => Self::Flags,
        }
    }
}

/// A Unicode emoji as the picker shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UnicodeEmoji {
    /// The emoji itself, which the system's emoji font draws.
    pub emoji: String,
    /// The short name Discord's users know it by, without colons (`thumbsup`).
    pub name: String,
    /// Other short codes (`+1`).
    pub shortcodes: Vec<String>,
    /// Unicode's English name (`thumbs up`).
    pub cldr_name: String,
    pub group: EmojiGroup,
    /// It comes in skin tones; the picker lists the default only.
    pub skin_tones: bool,
    pub version: EmojiVersion,
}

impl From<emoji::UnicodeEmoji> for UnicodeEmoji {
    fn from(emoji: emoji::UnicodeEmoji) -> Self {
        Self {
            emoji: emoji.as_str().to_owned(),
            name: emoji.name().into_owned(),
            shortcodes: emoji.shortcodes().map(str::to_owned).collect(),
            cldr_name: emoji.cldr_name().to_owned(),
            group: emoji.group().into(),
            skin_tones: emoji.has_skin_tones(),
            version: emoji.version().into(),
        }
    }
}

/// A group's emoji in Unicode's order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UnicodeEmojiGroup {
    pub group: EmojiGroup,
    pub emojis: Vec<UnicodeEmoji>,
}

/// Whether the user may react with a custom emoji in a channel, and why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EmojiAccess {
    Usable,
    /// The server lost the boosts the emoji needs: nobody can use it.
    Unavailable,
    /// Only some roles may use it, and the user has none of them.
    RoleLocked,
    /// Another server's emoji, where the channel doesn't allow external emoji.
    External,
    /// Needs Nitro: an animated emoji, or another server's (in a DM any server's) unless an
    /// integration's.
    NitroLocked,
}

impl From<state::EmojiAccess> for EmojiAccess {
    fn from(access: state::EmojiAccess) -> Self {
        match access {
            state::EmojiAccess::Usable => Self::Usable,
            state::EmojiAccess::Unavailable => Self::Unavailable,
            state::EmojiAccess::RoleLocked => Self::RoleLocked,
            state::EmojiAccess::External => Self::External,
            state::EmojiAccess::NitroLocked => Self::NitroLocked,
        }
    }
}

/// One of the user's servers' custom emoji, and whether the user may react with it here.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CustomEmoji {
    pub id: EmojiId,
    pub guild_id: GuildId,
    pub name: String,
    pub animated: bool,
    /// Its image, animated when `animated` says so.
    pub key: ImageKey,
    pub access: EmojiAccess,
}

impl From<&state::CustomEmoji> for CustomEmoji {
    fn from(custom: &state::CustomEmoji) -> Self {
        Self {
            id: custom.emoji.id,
            guild_id: custom.guild_id,
            name: custom.emoji.name.to_string(),
            animated: custom.emoji.animated,
            key: custom.emoji.image_key().into(),
            access: custom.access.into(),
        }
    }
}

/// A server's emoji as the picker lists them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmojiSection {
    pub guild_id: GuildId,
    pub emojis: Vec<CustomEmoji>,
}

impl From<&state::EmojiSection> for EmojiSection {
    fn from(section: &state::EmojiSection) -> Self {
        Self {
            guild_id: section.guild_id,
            emojis: section.emojis.iter().map(CustomEmoji::from).collect(),
        }
    }
}

/// An emoji the picker offers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PickerEmoji {
    Unicode { emoji: UnicodeEmoji },
    Custom { emoji: CustomEmoji },
}

impl From<&state::PickerEmoji> for PickerEmoji {
    fn from(picked: &state::PickerEmoji) -> Self {
        match picked {
            state::PickerEmoji::Unicode(emoji) => Self::Unicode {
                emoji: (*emoji).into(),
            },
            state::PickerEmoji::Custom(custom) => Self::Custom {
                emoji: custom.into(),
            },
        }
    }
}

/// The picker's Unicode emoji: groups in CLDR's order, each in Unicode's order, without
/// skin-toned variants and without emoji newer than `newest`; a group left empty is left out.
#[uniffi::export]
pub fn unicode_emoji(newest: EmojiVersion) -> Vec<UnicodeEmojiGroup> {
    emoji::unicode_groups(newest.into())
        .into_iter()
        .map(|group| UnicodeEmojiGroup {
            group: group.group.into(),
            emojis: group.emojis.into_iter().map(UnicodeEmoji::from).collect(),
        })
        .collect()
}

/// The newest emoji version Akari's catalog has.
#[uniffi::export]
pub fn newest_emoji_version() -> EmojiVersion {
    emoji::EmojiVersion::NEWEST.into()
}

/// The emoji `Account.react` takes for a picked one: a Unicode emoji as it is, a custom one by
/// its ID with its name.
#[uniffi::export]
pub fn picker_reaction(emoji: PickerEmoji) -> ReactionEmoji {
    let reaction = match &emoji {
        PickerEmoji::Unicode { emoji } => match emoji::UnicodeEmoji::get(&emoji.emoji) {
            Some(found) => state::PickerEmoji::Unicode(found).reaction(),
            None => state::ReactionEmoji::unicode(&emoji.emoji),
        },
        PickerEmoji::Custom { emoji } => {
            state::ReactionEmoji::custom(emoji.id, Some(&emoji.name), emoji.animated)
        }
    };
    ReactionEmoji::from(&reaction)
}
