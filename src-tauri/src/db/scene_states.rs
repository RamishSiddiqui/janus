use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::scene_state::{SceneState, SceneStateUpdate};

pub struct SceneStateRepo;

impl SceneStateRepo {
    /// Gets the scene state for a conversation. Returns None if no scene state exists yet.
    pub async fn get(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Option<SceneState>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM scene_states
                 WHERE conversation_id = type::thing('conversations', $conv_id)",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;

        let states: Vec<SceneState> = result.take(0)?;
        Ok(states.into_iter().next())
    }

    /// Upserts the scene state for a conversation using a deterministic ID.
    /// Merges the update with existing state — only overwrites fields that are Some.
    pub async fn upsert(
        db: &Surreal<Db>,
        conversation_id: &str,
        update: &SceneStateUpdate,
    ) -> Result<SceneState, MythicError> {
        // First get current state for merge
        let current = Self::get(db, conversation_id)
            .await?
            .unwrap_or_else(|| SceneState {
                id: surrealdb::sql::Thing::from(("scene_states", conversation_id)),
                conversation_id: surrealdb::sql::Thing::from(("conversations", conversation_id)),
                location_name: "Unknown".to_string(),
                location_description: String::new(),
                time_period: "unspecified".to_string(),
                weather: "clear".to_string(),
                characters_present: vec![],
                ambient_details: String::new(),
                scene_mood: "neutral".to_string(),
                updated_at: String::new(),
            });

        // Merge: use update value if Some, otherwise keep current
        let location_name = update
            .location_name
            .as_deref()
            .unwrap_or(&current.location_name);
        let location_description = update
            .location_description
            .as_deref()
            .unwrap_or(&current.location_description);
        let time_period = update
            .time_period
            .as_deref()
            .unwrap_or(&current.time_period);
        let weather = update.weather.as_deref().unwrap_or(&current.weather);
        let characters_present = update
            .characters_present
            .as_ref()
            .unwrap_or(&current.characters_present);
        let ambient_details = update
            .ambient_details
            .as_deref()
            .unwrap_or(&current.ambient_details);
        let scene_mood = update.scene_mood.as_deref().unwrap_or(&current.scene_mood);

        let composite_id = format!("ss_{}", conversation_id);

        let mut result = db
            .query(
                "UPSERT type::thing('scene_states', $composite_id) CONTENT {
                    conversation_id: type::thing('conversations', $conv_id),
                    location_name: $location_name,
                    location_description: $location_description,
                    time_period: $time_period,
                    weather: $weather,
                    characters_present: $characters_present,
                    ambient_details: $ambient_details,
                    scene_mood: $scene_mood,
                    updated_at: time::now(),
                }",
            )
            .bind(("composite_id", composite_id))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("location_name", location_name.to_string()))
            .bind(("location_description", location_description.to_string()))
            .bind(("time_period", time_period.to_string()))
            .bind(("weather", weather.to_string()))
            .bind(("characters_present", characters_present.clone()))
            .bind(("ambient_details", ambient_details.to_string()))
            .bind(("scene_mood", scene_mood.to_string()))
            .await?;

        let upserted: Option<SceneState> = result.take(0)?;
        upserted.ok_or_else(|| MythicError::DatabaseOp("Failed to upsert scene state".into()))
    }

    /// Deletes the scene state for a conversation (cleanup).
    pub async fn delete(db: &Surreal<Db>, conversation_id: &str) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM scene_states WHERE conversation_id = type::thing('conversations', $conv_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .await?;
        Ok(())
    }
}
