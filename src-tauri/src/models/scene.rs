use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A generated or imported scene (image/video) tied to a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Scene {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub conversation_id: Thing,
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub message_id: Option<Thing>,
    pub media_type: String,
    pub prompt: String,
    pub file_path: String,
    pub caption: Option<String>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    pub created_at: String,
}
