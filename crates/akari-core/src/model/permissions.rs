use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use serde::{Deserialize, Deserializer};

use super::snowflake::U64Visitor;

/// A permission bitfield, which Discord sends as a decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions(pub u64);

impl Permissions {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u64::MAX);
    pub const ADMINISTRATOR: Self = Self(1 << 3);
    pub const VIEW_CHANNEL: Self = Self(1 << 10);
    pub const SEND_MESSAGES: Self = Self(1 << 11);
    pub const SEND_TTS_MESSAGES: Self = Self(1 << 12);
    pub const EMBED_LINKS: Self = Self(1 << 14);
    pub const ATTACH_FILES: Self = Self(1 << 15);
    pub const READ_MESSAGE_HISTORY: Self = Self(1 << 16);
    pub const MENTION_EVERYONE: Self = Self(1 << 17);
    pub const CHANGE_NICKNAME: Self = Self(1 << 26);
    pub const SEND_MESSAGES_IN_THREADS: Self = Self(1 << 38);
    pub const BYPASS_SLOWMODE: Self = Self(1 << 52);

    /// Whether every bit of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Permissions {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitAnd for Permissions {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Not for Permissions {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl BitOrAssign for Permissions {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitAndAssign for Permissions {
    fn bitand_assign(&mut self, other: Self) {
        self.0 &= other.0;
    }
}

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(U64Visitor).map(Permissions)
    }
}
