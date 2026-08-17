//! Background pipelines that run after a chat turn completes — embedding,
//! scene-state extraction, NPC detection — plus the two stateless/read-only
//! commands (`generate_raw`, `get_context_stats`) that don't belong with
//! either the send or retry flow.

use std::sync::Arc;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::context::budget::ContextBudget;
use crate::context::prompt_builder::{build_prompt, ContextStats};
use crate::context::rag::embed_and_store;
use crate::context::scene_extractor::extract_scene_state;
use crate::db::providers::ProviderRepo;
use crate::db::scene_states::SceneStateRepo;
use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};
use crate::providers::unified::RigProvider;
use crate::AppState;

/// Resolves the provider + model for background embedding calls — the
/// shared first half of every background embed task.
///
/// This resolves through the `enabled_models` table (AI Studio → Embedding
/// Models), the same source `rebuild_embedding_index`/backfill already use
/// — NOT the default LLM provider's own config. An embedding model is
/// frequently hosted by a different provider than whichever one is
/// currently the default for chat, and the two were previously resolved
/// through unrelated paths: live per-message embedding silently used the
/// default LLM provider plus a legacy `config.embedding_model` field (with
/// no UI left to set it), while rebuild/backfill correctly looked up the
/// enabled embedding model's own provider. That drift is exactly why
/// "missing" counts on the Memory page kept growing — live embedding calls
/// were failing quietly against a provider that doesn't serve the fallback
/// model, while manual rebuilds (which used the right provider) worked.
async fn resolve_embedding_provider(
    db: &Surreal<Db>,
) -> Result<(RigProvider, String), MythicError> {
    let enabled = ProviderRepo::list_enabled_models(db, None).await?;
    let embedding_entry = enabled
        .iter()
        .find(|m| m.model_type == "embedding")
        .ok_or_else(|| {
            MythicError::Config(
                "No embedding model enabled. Go to AI Studio → Embedding Models and enable one."
                    .to_string(),
            )
        })?;
    let provider_config = ProviderRepo::get(db, &embedding_entry.provider_id).await?;
    let provider = create_rig_provider(&provider_config)?;
    Ok((provider, embedding_entry.model_id.clone()))
}

/// Extracts scene state from a narrative response in the background and
/// emits `scene_state_changed` if it actually changed. Runs a cheap
/// secondary LLM call (max_tokens=300, temp=0.1) to parse location/time/
/// weather/characters out of the text. Best-effort: failures are logged,
/// never propagated.
///
/// Factored out so it can run against more than just streamed assistant
/// replies — in particular, a character's greeting is inserted directly by
/// the frontend (see `create_message` call in `chat.ts`) and never passes
/// through the streaming handler below, so without also calling this for
/// the greeting, scene state (and therefore default image-generation
/// prompts) stayed empty until the *second* AI turn.
pub(crate) fn spawn_scene_extraction(
    db: Surreal<Db>,
    app: tauri::AppHandle,
    conversation_id: String,
    message_id: String,
    ai_response: String,
) {
    tokio::spawn(async move {
        // Whether scene extraction itself flagged a notable character event —
        // forces an immediate (out-of-cadence) NPC detection pass below.
        // Defaults false: if extraction fails or no LLM provider is
        // configured, NPC detection still runs on its own periodic cadence.
        let mut notable = false;

        let provider_config_result = get_default_llm_provider(&db).await;
        if let Err(ref e) = provider_config_result {
            debug!(
                "[scene_flow] No LLM provider configured, skipping extraction (non-fatal): {}",
                e
            );
        }
        if let Ok(provider_config) = provider_config_result {
            let provider_result = create_rig_provider(&provider_config);
            if let Err(ref e) = provider_result {
                debug!(
                    "[scene_flow] Failed to build provider for extraction (non-fatal): {}",
                    e
                );
            }
            if let Ok(provider) = provider_result {
                // Resolves the model the same way real chat completions do
                // (falls back to the first enabled non-embedding model when
                // the provider config itself has no `model` field set — e.g.
                // OpenRouter, where the active model lives in the separate
                // enabled_models table, not provider_config.config.model).
                // A naive `config.get("model").unwrap_or("default")` here
                // previously sent the literal string "default" as a model
                // ID, which every real provider rejects with a 400.
                match resolve_model_id(None, &provider_config, &db).await {
                    Ok(model) => {
                        // Get current scene state as JSON for context
                        let current_json = match SceneStateRepo::get(&db, &conversation_id).await {
                            Ok(Some(s)) => serde_json::to_string(&s).ok(),
                            _ => None,
                        };

                        match extract_scene_state(
                            &provider,
                            &model,
                            &ai_response,
                            current_json.as_deref(),
                        )
                        .await
                        {
                            Ok(update) => {
                                let changed = update.scene_changed;
                                notable = update.notable_character_event;
                                if notable {
                                    info!("[scene_flow] notable_character_event flagged — forcing immediate NPC detection");
                                }
                                match SceneStateRepo::upsert(&db, &conversation_id, &update).await {
                                    Ok(new_state) => {
                                        info!("[scene_flow] Updated scene: {} (changed={}, present={:?})",
                                            new_state.location_name, changed, new_state.characters_present);
                                        if changed {
                                            let _ = app.emit(
                                                "scene_state_changed",
                                                serde_json::to_value(&new_state)
                                                    .unwrap_or_default(),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        warn!("[scene_flow] Failed to save updated scene state (non-fatal): {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("[scene_flow] Extraction failed (non-fatal): {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("[scene_flow] No usable model resolved (non-fatal): {}", e);
                    }
                }
            }
        }

        spawn_npc_detection(db, app, conversation_id, message_id, ai_response, notable);
    });
}

/// Runs the story-driven NPC detection pipeline in the background.
/// Best-effort: failures are logged, never propagated — NPC detection must
/// never block or fail the chat flow. `forced` bypasses the periodic
/// cadence check (used when scene extraction just flagged a notable
/// character event); otherwise this only does real work once every
/// `NPC_DETECTION_CADENCE` messages.
pub(crate) fn spawn_npc_detection(
    db: Surreal<Db>,
    app: tauri::AppHandle,
    conversation_id: String,
    message_id: String,
    ai_response: String,
    forced: bool,
) {
    tokio::spawn(async move {
        if let Err(e) = crate::context::npc::pipeline::run_npc_detection(
            &db,
            &app,
            &conversation_id,
            &message_id,
            &ai_response,
            forced,
        )
        .await
        {
            debug!("[npc_flow] Detection failed (non-fatal): {}", e);
        }
    });
}

/// Kicks off scene-state extraction for a message that never went through
/// the normal streaming pipeline — currently just the character's greeting.
/// Fire-and-forget: returns as soon as the background task is spawned.
#[tauri::command]
#[specta::specta]
pub async fn extract_initial_scene(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    text: String,
) -> Result<(), MythicError> {
    let db = state.read().await.db.clone();
    // Greetings fire only once per conversation lifetime, so cadence-dedup
    // precision doesn't matter here — a fresh id per call is sufficient.
    let message_id = uuid::Uuid::new_v4().to_string();
    spawn_scene_extraction(db, app, conversation_id, message_id, text);
    Ok(())
}

/// Embeds a chat message in the background and emits `embedding_updated` on
/// success. Best-effort: failures are logged, never propagated — RAG embedding
/// must never block or fail the chat flow.
pub(crate) fn spawn_embed_message(
    db: Surreal<Db>,
    app: tauri::AppHandle,
    message_id: String,
    conversation_id: String,
    content: String,
    character_id: Option<String>,
) {
    tokio::spawn(async move {
        match resolve_embedding_provider(&db).await {
            Ok((provider, embedding_model)) => {
                match embed_and_store(
                    &db,
                    &provider,
                    &embedding_model,
                    &message_id,
                    &conversation_id,
                    &content,
                    character_id.as_deref(),
                )
                .await
                {
                    Ok(_) => {
                        let _ = app.emit("embedding_updated", ());
                    }
                    Err(e) => warn!("[embed] Failed to embed message {}: {}", message_id, e),
                }
            }
            Err(e) => warn!(
                "[embed] No embedding provider available for message {}: {}",
                message_id, e
            ),
        }
    });
}

/// Re-embeds a memory in the background after its content changes. Deletes
/// the stale embedding first so a failed re-embed leaves the memory
/// un-embedded (and thus caught by the backfill indexer) rather than matched
/// against stale content.
pub(crate) fn spawn_embed_memory(
    db: Surreal<Db>,
    memory_id: String,
    character_id: String,
    content: String,
) {
    tokio::spawn(async move {
        match resolve_embedding_provider(&db).await {
            Ok((provider, embedding_model)) => {
                if let Err(e) = crate::context::rag::embed_memory(
                    &db,
                    &provider,
                    &embedding_model,
                    &memory_id,
                    &character_id,
                    &content,
                )
                .await
                {
                    warn!("[embed] Failed to re-embed memory {}: {}", memory_id, e);
                }
            }
            Err(e) => warn!(
                "[embed] No embedding provider available for memory {}: {}",
                memory_id, e
            ),
        }
    });
}

/// Stateless LLM generation — calls the configured provider without saving
/// anything to the database. Used by internal pipelines (memory extraction,
/// summarization) that need LLM inference without polluting conversations.
#[tauri::command]
#[specta::specta]
pub async fn generate_raw(
    state: State<'_, Arc<RwLock<AppState>>>,
    system_prompt: String,
    user_prompt: String,
    model: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
) -> Result<String, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    let _http = state_guard.http_client.clone(); // retained for image providers
    drop(state_guard);

    let provider_config = get_default_llm_provider(&db).await?;
    let model_id = resolve_model_id(model, &provider_config, &db).await?;

    let gen_params = GenerationParams {
        max_tokens: max_tokens.unwrap_or(512),
        temperature: temperature.unwrap_or(0.3),
        ..Default::default()
    };

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: system_prompt,
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let provider = create_rig_provider(&provider_config)?;
    let result = provider
        .generate(&model_id, &messages, &[], &gen_params)
        .await?;

    Ok(result)
}

/// Returns context window statistics for a conversation.
/// Used by the frontend to display token usage and context budget info.
#[tauri::command]
#[specta::specta]
pub async fn get_context_stats(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
    system_prompt: Option<String>,
    post_history_instructions: Option<String>,
) -> Result<ContextStats, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    let provider_config = get_default_llm_provider(&db).await?;
    let max_context = provider_config
        .config
        .get("context_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384) as usize;

    let max_tokens = provider_config
        .config
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(2048) as usize;

    let context_budget = ContextBudget {
        max_context_tokens: max_context,
        reserved_for_response: max_tokens,
        ..Default::default()
    };

    let (_, stats) = build_prompt(
        &db,
        &conversation_id,
        &message_id,
        system_prompt.as_deref(),
        post_history_instructions.as_deref(),
        &context_budget,
    )
    .await?;

    Ok(stats)
}
