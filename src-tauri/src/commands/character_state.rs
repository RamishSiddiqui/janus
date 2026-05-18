//! Character emotional state commands — get and upsert per (character, conversation).

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::MythicError;
use crate::AppState;

/// The persisted emotional state of a character within one conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct CharacterState {
    pub id:               String,
    pub character_id:     String,
    pub conversation_id:  String,
    /// 0 = devastated, 50 = neutral, 100 = elated
    pub mood:             i32,
    /// 0 = hostile, 50 = wary, 100 = devoted
    pub trust:            i32,
    /// 0 = withdrawn, 50 = engaged, 100 = intense
    pub arousal:          i32,
    /// Single lowercase word describing the dominant emotion (e.g. "curious")
    pub dominant_emotion: String,
    /// 1–2 sentence description of the character's internal state (third person)
    pub state_summary:    String,
    pub updated_at:       String,
}

/// Returns the current emotional state for a character in a conversation.
/// Returns `None` if no state has been recorded yet (first turn).
#[tauri::command]
pub async fn get_character_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id:    String,
    conversation_id: String,
) -> Result<Option<CharacterState>, MythicError> {
    let g = state.read().await;
    let row: Option<CharacterState> = sqlx::query_as(
        "SELECT id, character_id, conversation_id, mood, trust, arousal,
                dominant_emotion, state_summary, updated_at
         FROM character_states
         WHERE character_id = ? AND conversation_id = ?",
    )
    .bind(&character_id)
    .bind(&conversation_id)
    .fetch_optional(&g.db)
    .await?;

    Ok(row)
}

/// Upserts the emotional state for a character in a conversation.
/// All integer axes are clamped to [0, 100].
#[tauri::command]
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
    let g = state.read().await;
    let id = Uuid::new_v4().to_string();

    let mood    = mood.clamp(0, 100);
    let trust   = trust.clamp(0, 100);
    let arousal = arousal.clamp(0, 100);

    sqlx::query(
        "INSERT INTO character_states
             (id, character_id, conversation_id, mood, trust, arousal,
              dominant_emotion, state_summary, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(character_id, conversation_id) DO UPDATE SET
             mood             = excluded.mood,
             trust            = excluded.trust,
             arousal          = excluded.arousal,
             dominant_emotion = excluded.dominant_emotion,
             state_summary    = excluded.state_summary,
             updated_at       = datetime('now')",
    )
    .bind(&id)
    .bind(&character_id)
    .bind(&conversation_id)
    .bind(mood)
    .bind(trust)
    .bind(arousal)
    .bind(&dominant_emotion)
    .bind(&state_summary)
    .execute(&g.db)
    .await?;

    let row: CharacterState = sqlx::query_as(
        "SELECT id, character_id, conversation_id, mood, trust, arousal,
                dominant_emotion, state_summary, updated_at
         FROM character_states
         WHERE character_id = ? AND conversation_id = ?",
    )
    .bind(&character_id)
    .bind(&conversation_id)
    .fetch_one(&g.db)
    .await?;

    Ok(row)
}
