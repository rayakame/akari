use std::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// A point in time that Discord sends as an ISO 8601 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(OffsetDateTime);

impl Timestamp {
    /// Milliseconds since the Unix epoch.
    pub fn unix_millis(self) -> i64 {
        self.0.unix_timestamp() * 1000 + i64::from(self.0.millisecond())
    }

    // Outside the years 1–9999 it falls back to the Unix epoch.
    pub(crate) fn from_unix_millis(unix_millis: i64) -> Self {
        Self(
            OffsetDateTime::from_unix_timestamp_nanos(i128::from(unix_millis) * 1_000_000)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH),
        )
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(TimestampVisitor)
    }
}

struct TimestampVisitor;

impl Visitor<'_> for TimestampVisitor {
    type Value = Timestamp;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an ISO 8601 timestamp")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Timestamp, E> {
        OffsetDateTime::parse(value, &Rfc3339)
            .map(Timestamp)
            .map_err(|_| E::invalid_value(de::Unexpected::Str(value), &self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_round_trip_through_unix_millis() {
        let timestamp = Timestamp::from_unix_millis(1_700_000_000_123);

        assert_eq!(timestamp.unix_millis(), 1_700_000_000_123);
    }
}
