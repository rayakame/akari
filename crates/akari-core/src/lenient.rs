use std::any::type_name;

use serde::de::{Deserialize, DeserializeOwned, Deserializer};
use serde_json::value::RawValue;

use crate::JsonError;
use crate::model::{GenericMarker, Snowflake};

pub(crate) fn skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let entries = Option::<Vec<&'de RawValue>>::deserialize(deserializer)?;
    Ok(parse_valid(entries.unwrap_or_default()))
}

// A list that isn't one at all becomes empty instead of failing the payload.
pub(crate) fn skip_invalid_or_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(raw) = Option::<&'de RawValue>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    let entries = serde_json::from_str::<Vec<&'de RawValue>>(raw.get()).unwrap_or_else(|err| {
        tracing::warn!(
            error = %JsonError::from(&err),
            "using no {} for a list that failed to parse",
            type_name::<T>()
        );
        Vec::new()
    });
    Ok(parse_valid(entries))
}

// For partial updates: a missing or `null` list means "unchanged".
pub(crate) fn skip_invalid_option<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let entries = Option::<Vec<&'de RawValue>>::deserialize(deserializer)?;
    Ok(entries.map(parse_valid))
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

// A part of a payload that doesn't parse becomes its default instead of failing the payload.
pub(crate) fn default_on_error<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let Some(raw) = Option::<&'de RawValue>::deserialize(deserializer)? else {
        return Ok(T::default());
    };
    Ok(serde_json::from_str(raw.get()).unwrap_or_else(|err| {
        tracing::warn!(
            error = %JsonError::from(&err),
            "using the default for a {} that failed to parse",
            type_name::<T>()
        );
        T::default()
    }))
}

pub(crate) fn parse_list<T: DeserializeOwned>(
    json: &[u8],
) -> Result<(Vec<T>, usize), serde_json::Error> {
    let entries: Vec<&RawValue> = serde_json::from_slice(json)?;
    let count = entries.len();
    Ok((parse_valid(entries), count))
}

fn parse_valid<T: DeserializeOwned>(entries: Vec<&RawValue>) -> Vec<T> {
    entries
        .into_iter()
        .filter_map(|entry| match serde_json::from_str(entry.get()) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::warn!(
                    id = ?entry_id(entry),
                    error = %JsonError::from(&err),
                    "skipping a {} that failed to parse",
                    type_name::<T>()
                );
                None
            }
        })
        .collect()
}

// Members carry `user_id` instead of `id`, guild settings `guild_id` and their overrides
// `channel_id`.
fn entry_id(entry: &RawValue) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Ids {
        id: Option<Snowflake<GenericMarker>>,
        user_id: Option<Snowflake<GenericMarker>>,
        channel_id: Option<Snowflake<GenericMarker>>,
        guild_id: Option<Snowflake<GenericMarker>>,
    }

    let ids: Ids = serde_json::from_str(entry.get()).ok()?;
    ids.id
        .or(ids.user_id)
        .or(ids.channel_id)
        .or(ids.guild_id)
        .map(Snowflake::get)
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

        let summary = JsonError::from(&err).to_string();

        assert!(err.to_string().contains("secret-name"), "{err}");
        assert!(summary.contains("line 1"), "{summary}");
        assert!(!summary.contains("secret-name"), "{summary}");
    }
}
