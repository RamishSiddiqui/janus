use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A character's membership in a conversation — tracks role, talkativeness, and active status.
/// Each character in a multi-character conversation gets one record.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConversationCharacter {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub conversation_id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub character_id: Thing,
    /// "primary" | "secondary" | "npc"
    pub role: String,
    /// 0-100: how often the AI should include this character's response
    pub talkativeness: i32,
    /// Whether the character is currently active in the scene (false = muted)
    pub is_active: bool,
    /// Character name (denormalized for convenience in prompt building)
    #[serde(default)]
    pub character_name: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
}
