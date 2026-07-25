//! Character emotional state commands — get and upsert per (character, conversation).

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;
use tauri::State;
use tokio::sync::RwLock;

use crate::db::character_state::CharacterStateRepo;
use crate::error::MythicError;
use crate::AppState;

/// The persisted emotional state of a character within one conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CharacterState {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub character_id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub conversation_id: Thing,
    /// 0 = devastated, 50 = neutral, 100 = elated
    pub mood: i32,
    /// 0 = hostile, 50 = wary, 100 = devoted
    pub trust: i32,
    /// 0 = withdrawn, 50 = engaged, 100 = intense
    pub arousal: i32,
    /// Single lowercase word describing the dominant emotion (e.g. "curious")
    pub dominant_emotion: String,
    /// 1–2 sentence description of the character's internal state (third person)
    pub state_summary: String,
    pub updated_at: String,
}

/// Returns the current emotional state for a character in a conversation.
/// Returns `None` if no state has been recorded yet (first turn).
#[tauri::command]
#[specta::specta]
pub async fn get_character_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id:    String,
    conversation_id: String,
) -> Result<Option<CharacterState>, MythicError> {
    let g = state.read().await;
    CharacterStateRepo::get(&g.db, &character_id, &conversation_id).await
}

/// Upserts the emotional state for a character in a conversation.
/// All integer axes are clamped to [0, 100].
#[tauri::command]
#[specta::specta]
pub async fn upsert_character_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id:     String,
    conversation_id:  String,
    mood:             i32,
    trust:            i32,
    arousal:          i32,
    dominant_emotion: String,
    state_summary:    String,
) -> Result<CharacterState, MythicError> {
    let mood    = mood.clamp(0, 100);
    let trust   = trust.clamp(0, 100);
    let arousal = arousal.clamp(0, 100);

    let g = state.read().await;
    CharacterStateRepo::upsert(
        &g.db,
        &character_id,
        &conversation_id,
        mood,
        trust,
        arousal,
        &dominant_emotion,
        &state_summary,
    )
    .await
}
