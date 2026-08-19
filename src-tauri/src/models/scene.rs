use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::types::RecordId;

/// A generated or imported scene (image/video) tied to a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Scene {
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub id: RecordId,
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub conversation_id: RecordId,
    #[serde(
        default,
        serialize_with = "crate::models::serialize_option_thing",
        deserialize_with = "crate::models::deserialize_option_thing"
    )]
    #[specta(type = Option<String>)]
    pub message_id: Option<RecordId>,
    pub media_type: String,
    pub prompt: String,
    pub file_path: String,
    pub caption: Option<String>,
    #[specta(type = Option<crate::models::JsonValue>)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
}
