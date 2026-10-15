use serde::Deserialize;

use crate::model::{RelationshipType, UserId};

/// An entry of READY's `relationships`, and RELATIONSHIP_ADD, which also announces a change
/// of type (blocking a friend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Relationship {
    /// The other user.
    pub id: UserId,
    #[serde(rename = "type")]
    pub kind: RelationshipType,
    /// The user ignores the other user.
    #[serde(default)]
    pub user_ignored: bool,
}

/// RELATIONSHIP_UPDATE: a relationship changed apart from its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RelationshipUpdate {
    pub id: UserId,
    /// `None` when unchanged.
    pub user_ignored: Option<bool>,
}

/// RELATIONSHIP_REMOVE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RelationshipRemove {
    pub id: UserId,
}
