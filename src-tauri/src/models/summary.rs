use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

/// A rolling summary of evicted conversation messages.
///
/// One summary per conversation (upserted via unique index on conversation_id).
/// Stores a narrative summary of messages that have been pushed out of the
/// sliding context window, giving the LLM long-term recall of earlier events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    pub id: Thing,
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    pub conversation_id: Thing,
    pub summary_text: String,
    pub covered_message_count: u32,
    pub token_count: u32,
    #[serde(
        default,
        serialize_with = "crate::models::serialize_option_thing",
        deserialize_with = "crate::models::deserialize_option_thing"
    )]
    pub window_start_message_id: Option<Thing>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    pub created_at: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    pub updated_at: String,
}
