use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A user-controlled persona — the player's own stand-in for a conversation.
/// Shares the same CharacterCardV2-shaped `data` JSON as `characters`
/// (see `models::character::CharacterCardV2`/`CharacterData`), but carries
/// none of the NPC-pipeline-only fields (`origin`/`portrait_status`/
/// `profile_reviewed`) since every persona is user-facing by definition.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Persona {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    pub name: String,
    pub spec: String,

    /// Full Character Card V2 JSON, stored as native JSON in SurrealDB.
    #[specta(type = crate::models::JsonValue)]
    pub data: serde_json::Value,

    /// Path to the persona's avatar image file (relative to app data dir).
    pub avatar_path: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub updated_at: String,

    /// Set when the persona is in the Trash; None means it's live.
    #[serde(default, deserialize_with = "crate::models::deserialize_option_datetime")]
    #[specta(type = Option<String>)]
    pub deleted_at: Option<String>,
}
