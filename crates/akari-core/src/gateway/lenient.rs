use std::any::type_name;

use serde::de::{Deserialize, DeserializeOwned, Deserializer};
use serde_json::value::RawValue;

use crate::model::{GenericMarker, GuildMarker, Snowflake};

#[derive(serde::Deserialize)]
pub(super) struct IdOnly {
    pub(super) id: Snowflake<GuildMarker>,
}

pub(super) fn skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let entries = Vec::<&'de RawValue>::deserialize(deserializer)?;
    Ok(parse_valid(entries))
}

// The outer list stays strict so it keeps its alignment with another list.
pub(super) fn skip_invalid_in_each<'de, D, T>(deserializer: D) -> Result<Vec<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let lists = Vec::<Vec<&'de RawValue>>::deserialize(deserializer)?;
    Ok(lists.into_iter().map(parse_valid).collect())
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
