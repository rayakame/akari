use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// A Discord ID, typed by what it identifies, such as `Snowflake<UserMarker>`.
pub struct Snowflake<M> {
    value: u64,
    marker: PhantomData<fn(M) -> M>,
}

impl<M> Snowflake<M> {
    pub const fn new(value: u64) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    pub const fn get(self) -> u64 {
        self.value
    }

    /// Reinterprets the ID, e.g. a guild ID as the ID of its `@everyone` role.
    pub const fn cast<N>(self) -> Snowflake<N> {
        Snowflake::new(self.value)
    }

    pub(crate) fn from_unix_millis(unix_millis: i64, sequence: u64) -> Self {
        let since_epoch = u64::try_from(unix_millis - DISCORD_EPOCH).unwrap_or(0);
        Self::new((since_epoch << TIMESTAMP_SHIFT) | (sequence & SEQUENCE_MASK))
    }

    #[cfg(test)]
    pub(crate) fn unix_millis(self) -> i64 {
        i64::try_from(self.value >> TIMESTAMP_SHIFT).unwrap_or(i64::MAX - DISCORD_EPOCH)
            + DISCORD_EPOCH
    }
}

// The first second of 2015, from which snowflake timestamps count.
const DISCORD_EPOCH: i64 = 1_420_070_400_000;
const TIMESTAMP_SHIFT: u32 = 22;
const SEQUENCE_MASK: u64 = (1 << TIMESTAMP_SHIFT) - 1;

// Implemented by hand so markers need no derives of their own.
impl<M> Clone for Snowflake<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for Snowflake<M> {}

impl<M> PartialEq for Snowflake<M> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<M> Eq for Snowflake<M> {}

impl<M> PartialOrd for Snowflake<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<M> Ord for Snowflake<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<M> Hash for Snowflake<M> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<M> fmt::Debug for Snowflake<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Snowflake").field(&self.value).finish()
    }
}

impl<'de, M> Deserialize<'de> for Snowflake<M> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(U64Visitor).map(Self::new)
    }
}

// Discord sends IDs as strings but echoes small ones back as JSON integers.
pub(super) struct U64Visitor;

impl Visitor<'_> for U64Visitor {
    type Value = u64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an unsigned integer or a string containing one")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        Ok(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<u64, E> {
        value
            .parse()
            .map_err(|_| E::invalid_value(de::Unexpected::Str(value), &self))
    }
}

/// Marks the ID of a file attached to a message.
pub enum AttachmentMarker {}

/// Marks the ID of a channel, thread, DM or group DM.
pub enum ChannelMarker {}

/// Marks the ID of a custom emoji.
pub enum EmojiMarker {}

/// Marks an ID that can identify more than one kind, such as a permission overwrite's
/// role or member.
pub enum GenericMarker {}

/// Marks the ID of a guild.
pub enum GuildMarker {}

/// Marks the ID of a message.
pub enum MessageMarker {}

/// Marks the ID of a role.
pub enum RoleMarker {}

/// Marks the ID of a store SKU, such as an avatar decoration's.
pub enum SkuMarker {}

/// Marks the ID of a sticker.
pub enum StickerMarker {}

/// Marks the ID of a user.
pub enum UserMarker {}

/// Marks the ID of a webhook.
pub enum WebhookMarker {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snowflakes_round_trip_through_unix_millis() {
        let id = Snowflake::<MessageMarker>::from_unix_millis(1_420_070_401_000, 5);

        assert_eq!(id.get(), (1000 << 22) + 5);
        assert_eq!(id.unix_millis(), 1_420_070_401_000);
        assert_eq!(
            Snowflake::<MessageMarker>::new(4_194_304_005).unix_millis(),
            1_420_070_401_000
        );
    }
}
