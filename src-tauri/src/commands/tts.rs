//! Tauri commands for native Kokoro TTS — model download/status, voice
//! listing, per-character voice assignment, and immediate synthesis for a
//! "preview this voice" button. The streaming-chat synthesis path itself
//! lives in `commands::chat::streaming`, not here — these commands are the
//! setup/management surface around it.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::db::characters::CharacterRepo;
use crate::error::MythicError;
use crate::models::character::Character;
use crate::tts::{download, engine::VoiceInfo, KokoroEngine};
use crate::AppState;

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct TtsModelStatus {
    pub downloaded: bool,
}

/// Whether the Kokoro model + voice pack are already cached on disk. Pure
/// filesystem check — doesn't load the engine.
#[tauri::command]
#[specta::specta]
pub async fn tts_model_status(app: AppHandle) -> Result<TtsModelStatus, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    Ok(TtsModelStatus {
        downloaded: download::is_downloaded(&app_data_dir),
    })
}

/// Downloads the model + voice pack if not already cached, emitting
/// `tts-download-progress` events as it goes. A no-op (returns
/// immediately) if already downloaded — callers don't need to check
/// `tts_model_status` first.
#[tauri::command]
#[specta::specta]
pub async fn tts_download_model(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    if download::is_downloaded(&app_data_dir) {
        return Ok(());
    }
    let http_client = state.read().await.http_client.clone();
    download::download_all(&app, &http_client).await
}

/// Loads the engine into `AppState.tts_engine` if not already loaded, then
/// returns a clone-free lock guard's worth of access via the callback —
/// callers go through `with_engine` rather than reaching into the mutex
/// directly, since "load if absent" is shared, load-bearing logic that a
/// direct `.lock().await` at each call site would risk duplicating
/// incorrectly (e.g. loading twice concurrently).
async fn ensure_engine_loaded(
    app: &AppHandle,
    state: &State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), MythicError> {
    let tts_engine = state.read().await.tts_engine.clone();
    let mut guard = tts_engine.lock().await;
    if guard.is_some() {
        return Ok(());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    if !download::is_downloaded(&app_data_dir) {
        return Err(MythicError::Validation(
            "TTS model not downloaded yet — call tts_download_model first".to_string(),
        ));
    }

    let model_path = download::model_path(&app_data_dir);
    let voices_path = download::voices_path(&app_data_dir);
    let runtime_path = download::runtime_path(&app_data_dir);
    // ONNX session load + voice-pack parse is real, possibly multi-second
    // work — done inline here since this command is already expected to
    // block until the engine is ready (first-use latency is an accepted
    // tradeoff for never loading eagerly at app startup).
    let engine = KokoroEngine::load(&model_path, &voices_path, &runtime_path)?;
    *guard = Some(engine);
    Ok(())
}

/// Lists all voices in the loaded voice pack (loading the engine first if
/// needed).
#[tauri::command]
#[specta::specta]
pub async fn tts_list_voices(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<VoiceInfo>, MythicError> {
    ensure_engine_loaded(&app, &state).await?;
    let tts_engine = state.read().await.tts_engine.clone();
    let guard = tts_engine.lock().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| MythicError::Provider("TTS engine failed to load".to_string()))?;
    Ok(engine.list_voices())
}

/// Assigns (or clears, via `voice_id: None`) the voice a character speaks
/// with.
#[tauri::command]
#[specta::specta]
pub async fn tts_set_character_voice(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
    voice_id: Option<String>,
) -> Result<Character, MythicError> {
    let db = state.read().await.db.clone();
    CharacterRepo::set_voice(&db, &character_id, voice_id.as_deref()).await
}

/// Synthesizes `text` immediately and returns WAV bytes — for a "preview
/// this voice" button in the character editor. Base64-encoded rather than
/// raw `Vec<u8>`: a plain `Vec<u8>` return still serializes as a JSON
/// array of numbers over Tauri IPC (3-5x the byte size in JSON text),
/// while base64 is ~1.33x — meaningfully cheaper for a payload that can be
/// several hundred KB.
#[tauri::command]
#[specta::specta]
pub async fn tts_test_speak(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    text: String,
    voice_id: String,
) -> Result<String, MythicError> {
    ensure_engine_loaded(&app, &state).await?;
    let tts_engine = state.read().await.tts_engine.clone();
    let mut guard = tts_engine.lock().await;
    let engine = guard
        .as_mut()
        .ok_or_else(|| MythicError::Provider("TTS engine failed to load".to_string()))?;
    let wav = engine.synthesize(&text, &voice_id, 1.0)?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        wav,
    ))
}
