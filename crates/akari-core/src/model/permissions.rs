use serde::{Deserialize, Deserializer};

use super::snowflake::U64Visitor;

/// A permission bitfield, which Discord sends as a decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions(pub u64);

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(U64Visitor).map(Permissions)
    }
}
