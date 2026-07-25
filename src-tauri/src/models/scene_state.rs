use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// Dynamic scene state for a conversation — tracks location, time, weather,
/// characters present, and ambient atmosphere. Updated automatically after
/// each AI response via the scene extraction engine.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SceneState {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub conversation_id: Thing,
    pub location_name: String,
    pub location_description: String,
    /// morning | midday | afternoon | evening | night | late_night | dawn | unspecified
    pub time_period: String,
    /// clear | cloudy | raining | storming | snowing | foggy | windy
    pub weather: String,
    /// Names of characters currently present in the scene
    #[serde(default)]
    pub characters_present: Vec<String>,
    pub ambient_details: String,
    /// tense | calm | romantic | mysterious | dangerous | joyful | melancholic | neutral
    pub scene_mood: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub updated_at: String,
}

/// Partial update struct used by the scene extractor.
/// All fields are optional — only changed fields are updated.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneStateUpdate {
    pub location_name: Option<String>,
    pub location_description: Option<String>,
    pub time_period: Option<String>,
    pub weather: Option<String>,
    pub characters_present: Option<Vec<String>>,
    pub ambient_details: Option<String>,
    pub scene_mood: Option<String>,
    /// Whether the scene actually changed (used to decide if we emit an event)
    #[serde(default)]
    pub scene_changed: bool,
}
