//! Character emotional state commands — get and upsert per (character, conversation).

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use surrealdb::types::RecordId;
use tauri::State;
use tokio::sync::RwLock;

use crate::db::character_state::CharacterStateRepo;
use crate::db::messages::MessageRepo;
use crate::error::MythicError;
use crate::models::DynamicJson;
use crate::AppState;

/// The persisted emotional state of a character within one conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CharacterState {
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
    pub character_id: RecordId,
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub conversation_id: RecordId,
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
    character_id: String,
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
    character_id: String,
    conversation_id: String,
    mood: i32,
    trust: i32,
    arousal: i32,
    dominant_emotion: String,
    state_summary: String,
) -> Result<CharacterState, MythicError> {
    let mood = mood.clamp(0, 100);
    let trust = trust.clamp(0, 100);
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

/// Freezes an emotional-state snapshot (one entry per character, keyed by
/// character id) onto a specific message's metadata, so its EmotionHUD pill
/// keeps showing what each character felt *at that point in the story*
/// instead of whatever `character_states` holds right now. `states` is
/// passed through as raw JSON rather than a typed map — the backend never
/// reads its shape, only stores and returns it verbatim to the frontend.
/// Uses `DynamicJson`, not a bare `serde_json::Value` param — see its doc
/// comment in `models::mod` for why (the same infinite-recursion crash
/// `JsonValue` was built to avoid).
#[tauri::command]
#[specta::specta]
pub async fn set_message_emotional_snapshot(
    state: State<'_, Arc<RwLock<AppState>>>,
    message_id: String,
    states: DynamicJson,
) -> Result<(), MythicError> {
    let g = state.read().await;
    MessageRepo::merge_metadata(
        &g.db,
        &message_id,
        serde_json::json!({ "emotional_states": states.0 }),
    )
    .await
}
