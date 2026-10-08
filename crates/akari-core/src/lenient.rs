use std::any::type_name;
use std::fmt;

use serde::de::{Deserialize, DeserializeOwned, Deserializer};
use serde_json::value::RawValue;

use crate::model::{GenericMarker, Snowflake};

pub(crate) fn skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let entries = Option::<Vec<&'de RawValue>>::deserialize(deserializer)?;
    Ok(parse_valid(entries.unwrap_or_default()))
}

// The outer list never drops an entry, so it stays aligned with another list.
pub(crate) fn skip_invalid_in_each<'de, D, T>(deserializer: D) -> Result<Vec<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let lists = Option::<Vec<Option<Vec<&'de RawValue>>>>::deserialize(deserializer)?;
    Ok(lists
        .unwrap_or_default()
        .into_iter()
        .map(|list| parse_valid(list.unwrap_or_default()))
        .collect())
}

fn parse_valid<T: DeserializeOwned>(entries: Vec<&RawValue>) -> Vec<T> {
    entries
        .into_iter()
        .filter_map(|entry| match serde_json::from_str(entry.get()) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::warn!(
                    id = ?entry_id(entry),
                    error = %JsonErrorSummary(&err),
                    "skipping a {} that failed to parse",
                    type_name::<T>()
                );
                None
            }
        })
        .collect()
}

// Members carry `user_id` instead of `id`.
fn entry_id(entry: &RawValue) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Ids {
        id: Option<Snowflake<GenericMarker>>,
        user_id: Option<Snowflake<GenericMarker>>,
    }

    let ids: Ids = serde_json::from_str(entry.get()).ok()?;
    ids.id.or(ids.user_id).map(Snowflake::get)
}

/// Describes a serde_json error without its message, which can quote the payload.
pub(crate) struct JsonErrorSummary<'a>(pub(crate) &'a serde_json::Error);

impl fmt::Display for JsonErrorSummary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.0.classify() {
            serde_json::error::Category::Io => "I/O",
            serde_json::error::Category::Syntax => "syntax",
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "unexpected end",
        };
        write!(
            f,
            "{kind} error at line {}, column {}",
            self.0.line(),
            self.0.column()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::User;

    #[test]
    fn summaries_never_quote_the_payload() {
        let err = serde_json::from_str::<User>(
            r#"{"id": "1", "username": "akari", "accent_color": "secret-name"}"#,
        )
        .unwrap_err();

        let summary = JsonErrorSummary(&err).to_string();

        assert!(err.to_string().contains("secret-name"), "{err}");
        assert!(summary.contains("line 1"), "{summary}");
        assert!(!summary.contains("secret-name"), "{summary}");
    }
}
