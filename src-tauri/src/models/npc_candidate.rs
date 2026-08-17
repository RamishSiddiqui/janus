use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A named character seen in a conversation's narrative that the NPC
/// detector is tracking toward possible profile generation. Requires two
/// separate `recurring`/`pivotal`-tagged detector passes (`pass_count >= 2`)
/// before a full profile is generated for it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NpcCandidate {
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub conversation_id: Thing,
    /// Trimmed + lowercased `display_name`, used as the dedupe key.
    pub candidate_key: String,
    pub display_name: String,
    /// "recurring" | "pivotal" — the detector's most recent tag for this name.
    pub tag: String,
    /// Number of separate detector passes that tagged this name
    /// recurring/pivotal (background-tagged mentions never increment this).
    pub pass_count: i32,
    /// "pending" | "created"
    pub status: String,
    #[serde(
        default,
        serialize_with = "crate::models::serialize_option_thing",
        deserialize_with = "crate::models::deserialize_option_thing"
    )]
    #[specta(type = Option<String>)]
    pub resulting_character_id: Option<Thing>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub first_seen_at: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub last_seen_at: String,
}

/// Per-conversation cadence tracking for the periodic (non-forced) NPC
/// detection safety net — runs every `NPC_DETECTION_CADENCE` messages
/// regardless of whether the scene extractor's `notable_character_event`
/// flag ever fires.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NpcDetectionState {
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub conversation_id: Thing,
    pub messages_since_scan: i32,
    pub last_scanned_message_id: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub updated_at: String,
}
