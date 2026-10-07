use std::any::type_name;

use serde::de::{Deserialize, DeserializeOwned, Deserializer};
use serde_json::value::RawValue;

use crate::model::{GenericMarker, Snowflake};

pub(super) fn skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let entries = Option::<Vec<&'de RawValue>>::deserialize(deserializer)?;
    Ok(parse_valid(entries.unwrap_or_default()))
}

// The outer list never drops an entry, so it stays aligned with another list.
pub(super) fn skip_invalid_in_each<'de, D, T>(deserializer: D) -> Result<Vec<Vec<T>>, D::Error>
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
                    error = %err,
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
