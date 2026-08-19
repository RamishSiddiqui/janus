use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::commands::character_state::CharacterState;
use crate::error::MythicError;

pub struct CharacterStateRepo;

impl CharacterStateRepo {
    /// Gets the emotional state for a character in a conversation.
    pub async fn get(
        db: &Surreal<Db>,
        character_id: &str,
        conversation_id: &str,
    ) -> Result<Option<CharacterState>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM character_states
                 WHERE character_id = type::record('characters', $char_id)
                   AND conversation_id = type::record('conversations', $conv_id)",
            )
            .bind(("char_id", character_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .await?;

        let states: Vec<CharacterState> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(states.into_iter().next())
    }

    /// Upserts the emotional state using a deterministic composite ID.
    pub async fn upsert(
        db: &Surreal<Db>,
        character_id: &str,
        conversation_id: &str,
        mood: i32,
        trust: i32,
        arousal: i32,
        dominant_emotion: &str,
        state_summary: &str,
    ) -> Result<CharacterState, MythicError> {
        let composite_id = format!("{}_{}", character_id, conversation_id);

        let mut result = db
            .query(
                "UPSERT type::record('character_states', $composite_id) CONTENT {
                    character_id: type::record('characters', $char_id),
                    conversation_id: type::record('conversations', $conv_id),
                    mood: $mood,
                    trust: $trust,
                    arousal: $arousal,
                    dominant_emotion: $dominant_emotion,
                    state_summary: $state_summary,
                    updated_at: time::now(),
                }",
            )
            .bind(("composite_id", composite_id))
            .bind(("char_id", character_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("mood", mood))
            .bind(("trust", trust))
            .bind(("arousal", arousal))
            .bind(("dominant_emotion", dominant_emotion.to_string()))
            .bind(("state_summary", state_summary.to_string()))
            .await?;

        let upserted: Option<CharacterState> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        upserted.ok_or_else(|| MythicError::DatabaseOp("Failed to upsert character state".into()))
    }
}
