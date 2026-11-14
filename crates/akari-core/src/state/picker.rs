//! What the emoji picker lists for reacting in a channel: the user's servers' custom emoji with
//! whether each may be used there, the user's recently used reactions, and search.

use std::borrow::Cow;

use super::reactions::ReactionEmoji;
use super::types::GuildEmoji;
use crate::emoji::UnicodeEmoji;
use crate::model::GuildId;

/// The most recently used reactions the account keeps, as Discord's client keeps 42 frequently
/// used emoji.
pub const RECENT_REACTIONS: usize = 42;

/// Whether the user may react with a custom emoji in a channel, and why not, as Discord's client
/// tells it apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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

impl EmojiAccess {
    pub fn usable(self) -> bool {
        self == Self::Usable
    }

    // Discord's picker leaves these out where they apply, and shows the rest dimmed.
    pub(crate) fn listed(self) -> bool {
        !matches!(self, Self::External | Self::RoleLocked)
    }
}

/// One of the user's servers' custom emoji, and whether the user may react with it here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CustomEmoji {
    pub guild_id: GuildId,
    pub emoji: GuildEmoji,
    pub access: EmojiAccess,
}

/// A server's emoji as the picker lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmojiSection {
    pub guild_id: GuildId,
    pub emojis: Vec<CustomEmoji>,
}

/// An emoji the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PickerEmoji {
    Unicode(UnicodeEmoji),
    Custom(CustomEmoji),
}

impl PickerEmoji {
    /// The name its tooltip shows between colons.
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Self::Unicode(emoji) => emoji.name(),
            Self::Custom(custom) => Cow::Borrowed(&custom.emoji.name),
        }
    }

    pub fn access(&self) -> EmojiAccess {
        match self {
            Self::Unicode(_) => EmojiAccess::Usable,
            Self::Custom(custom) => custom.access,
        }
    }

    /// The emoji [`Account::react`](crate::Account::react) takes for it: a Unicode emoji as it
    /// is (Akari has no default skin tone), a custom one by its ID with its name.
    pub fn reaction(&self) -> ReactionEmoji {
        match self {
            Self::Unicode(emoji) => ReactionEmoji::unicode(emoji.as_str()),
            Self::Custom(custom) => ReactionEmoji::custom(
                custom.emoji.id,
                Some(&custom.emoji.name),
                custom.emoji.animated,
            ),
        }
    }
}
