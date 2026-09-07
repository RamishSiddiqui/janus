//! Tauri commands for native Kokoro TTS — model download/status, voice
//! listing, per-character voice assignment, and immediate synthesis for a
//! "preview this voice" button. The streaming-chat synthesis path itself
//! lives in `commands::chat::streaming`, not here — these commands are the
//! setup/management surface around it.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

use crate::db::characters::CharacterRepo;
use crate::error::MythicError;
use crate::models::character::Character;
use crate::tts::{chunker::split_complete_sentences, download, engine::VoiceInfo, KokoroEngine, TtsChunkEvent};
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
    // work (~4-5s measured: ONNX session commit ~1.3-2.4s, G2P engine build
    // ~2.3s). Blocking here is fine for `tts_list_voices`/`tts_test_speak`,
    // which the frontend already shows a loading state for — but see
    // `tts_preload_engine` below for warming this proactively at app start
    // so a user's *first* real interaction (opening Settings, sending a
    // message) doesn't pay this cost inline.
    let engine = KokoroEngine::load(&model_path, &voices_path, &runtime_path)?;
    *guard = Some(engine);
    Ok(())
}

/// Warms the TTS engine in the background if the model's already
/// downloaded — called from the frontend on app start when TTS is enabled,
/// so the ~4-5s load cost is paid once, quietly, before the user's first
/// real interaction rather than stacked onto it. A no-op, not an error,
/// when the model isn't downloaded yet (nothing to preload) or the engine
/// is already loaded — safe to call unconditionally on every app launch.
#[tauri::command]
#[specta::specta]
pub async fn tts_preload_engine(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    if !download::is_downloaded(&app_data_dir) {
        return Ok(());
    }
    ensure_engine_loaded(&app, &state).await
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
    let t0 = std::time::Instant::now();
    ensure_engine_loaded(&app, &state).await?;
    tracing::info!("[tts] tts_test_speak: ensure_engine_loaded took {:?}", t0.elapsed());

    let t1 = std::time::Instant::now();
    let tts_engine = state.read().await.tts_engine.clone();
    let mut guard = tts_engine.lock().await;
    let engine = guard
        .as_mut()
        .ok_or_else(|| MythicError::Provider("TTS engine failed to load".to_string()))?;
    // `synthesize_long`, not `synthesize` — defensive against any future
    // long preview text even though today's callers (Settings, character
    // editor) only ever pass a short fixed phrase; chunking a single short
    // sentence is a no-op either way. The per-message "replay this
    // message's voice" button does NOT use this command — see
    // `tts_replay_message` below, which streams chunk-by-chunk instead of
    // making the user wait for the entire (possibly multi-sentence, tens
    // of seconds of) message to finish before any audio plays.
    let wav = engine.synthesize_long(&text, &voice_id, 1.0)?;
    tracing::info!("[tts] tts_test_speak: synthesize_long() took {:?}", t1.elapsed());

    let t2 = std::time::Instant::now();
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wav);
    tracing::info!(
        "[tts] tts_test_speak: base64 encode took {:?}; command total {:?}",
        t2.elapsed(),
        t0.elapsed()
    );
    Ok(encoded)
}

/// Replays an already-saved chat message's voice, sentence by sentence —
/// emits one `tts-chunk` event per completed sentence (same event, same
/// shape, as the live-streaming path in `commands::chat::streaming`), so
/// the frontend's existing `ttsPlayback.ts` queue starts playing the first
/// sentence as soon as it's ready instead of waiting for the whole message.
/// Measured the difference directly: a 4-sentence, ~700-character message
/// took 42s end-to-end through `tts_test_speak`'s single-WAV-then-return
/// design (silence the entire time) versus first audible sound at ~8s here
/// (first sentence done), with the rest arriving progressively.
///
/// Fire-and-forget from the frontend's perspective — it doesn't need to
/// await this to know playback started; `ttsPlayback.ts`'s `isSpeaking`/
/// `currentMessageId` already track that live. Still returns `Result` so a
/// genuine failure (bad voice_id, engine not loaded) surfaces as an error
/// rather than silently doing nothing.
#[tauri::command]
#[specta::specta]
pub async fn tts_replay_message(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
    text: String,
    voice_id: String,
) -> Result<(), MythicError> {
    ensure_engine_loaded(&app, &state).await?;
    let tts_engine = state.read().await.tts_engine.clone();

    let (mut sentences, remainder) = split_complete_sentences(&text);
    let trimmed_remainder = remainder.trim();
    if !trimmed_remainder.is_empty() {
        sentences.push(trimmed_remainder.to_string());
    }
    if sentences.is_empty() {
        sentences.push(text.trim().to_string());
    }

    // A synthesis failure partway through must still let the end signal
    // fire below — otherwise the frontend never learns "no more chunks are
    // coming" and the Play button is stuck on "Playing…" forever, same bug
    // this whole event exists to fix, just via the error path instead of
    // the success path. Collecting the loop's Result rather than using `?`
    // directly is what makes that unconditional emit possible.
    let mut sequence: u32 = 0;
    let mut result: Result<(), MythicError> = Ok(());
    for sentence in sentences {
        if sentence.is_empty() {
            continue;
        }
        let wav = {
            let mut guard = tts_engine.lock().await;
            let engine = match guard
                .as_mut()
                .ok_or_else(|| MythicError::Provider("TTS engine failed to load".to_string()))
            {
                Ok(e) => e,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            };
            match engine.synthesize(&sentence, &voice_id, 1.0) {
                Ok(w) => w,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        };
        let audio = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wav);
        let _ = app.emit(
            "tts-chunk",
            TtsChunkEvent {
                conversation_id: conversation_id.clone(),
                message_id: message_id.clone(),
                sequence,
                audio,
                text: sentence,
            },
        );
        sequence += 1;
    }
    let _ = app.emit(
        "tts-stream-end",
        crate::tts::TtsStreamEndEvent {
            conversation_id,
            message_id,
        },
    );
    result
}
