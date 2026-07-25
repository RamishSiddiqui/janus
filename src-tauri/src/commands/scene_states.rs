//! Scene state commands — get, upsert, and delete per conversation.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::db::scene_states::SceneStateRepo;
use crate::error::MythicError;
use crate::models::scene_state::SceneState;
use crate::AppState;

/// Returns the current scene state for a conversation.
/// Returns `None` if no scene state has been established yet.
#[tauri::command]
#[specta::specta]
pub async fn get_scene_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Option<SceneState>, MythicError> {
    let g = state.read().await;
    SceneStateRepo::get(&g.db, &conversation_id).await
}

/// Manually upserts the scene state for a conversation.
/// Used by the frontend for manual overrides.
#[tauri::command]
#[specta::specta]
pub async fn upsert_scene_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    location_name: Option<String>,
    location_description: Option<String>,
    time_period: Option<String>,
    weather: Option<String>,
    characters_present: Option<Vec<String>>,
    ambient_details: Option<String>,
    scene_mood: Option<String>,
) -> Result<SceneState, MythicError> {
    use crate::models::scene_state::SceneStateUpdate;
    let update = SceneStateUpdate {
        location_name,
        location_description,
        time_period,
        weather,
        characters_present,
        ambient_details,
        scene_mood,
        scene_changed: false,
    };
    let g = state.read().await;
    SceneStateRepo::upsert(&g.db, &conversation_id, &update).await
}

/// Deletes the scene state for a conversation.
#[tauri::command]
#[specta::specta]
pub async fn delete_scene_state(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<(), MythicError> {
    let g = state.read().await;
    SceneStateRepo::delete(&g.db, &conversation_id).await
}
