//! `send_message` — the primary chat endpoint: saves the user message,
//! builds the prompt, and streams (or generates) the AI response.

use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::context::budget::ContextBudget;
use crate::context::prompt_builder::build_prompt;
use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::messages::MessageRepo;
use crate::error::MythicError;
use crate::models::conversation::GenerationParams;
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};
use crate::providers::traits::StreamChunk;
use crate::AppState;

use super::attachments::load_message_images;
use super::pipeline::{spawn_embed_message, spawn_scene_extraction};
use super::streaming::{run_stream_completion, StreamCompletionCtx, StreamEvent, StreamOrigin};
use super::SendMessageResult;

/// Sends a user message and streams the AI response back.
///
/// This is the primary chat endpoint. It:
/// 1. Saves the user message to the database
/// 2. Builds the full prompt from character data + conversation history
/// 3. Streams the response from the active LLM provider
/// 4. Saves the completed AI response to the database
#[tauri::command]
#[specta::specta]
pub async fn send_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    content: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
    attachments: Option<Vec<crate::models::conversation::MessageAttachment>>,
) -> Result<SendMessageResult, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    let _http = state_guard.http_client.clone(); // retained for image providers
    drop(state_guard);

    debug!("[send_message] conversation={}, content_len={}", conversation_id, content.len());

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    // 1. Save the user message
    // Get current active message as parent for branching
    let conv = ConversationRepo::get(&db, &conversation_id).await?;
    let parent_id: Option<String> = conv.active_message_id.as_ref().map(|t| t.id.to_raw());

    let attachment_metadata = attachments
        .as_ref()
        .filter(|a| !a.is_empty())
        .map(|a| serde_json::json!({ "attachments": a }));

    // MessageRepo::create generates a UUID, inserts the message, and updates
    // the conversation's active_message_id automatically.
    let user_msg = MessageRepo::create(
        &db,
        &conversation_id,
        "user",
        &content,
        parent_id.as_deref(),
        attachment_metadata,
    ).await?;
    let user_msg_id = user_msg.id.id.to_raw();

    // Resolved once, reused by every generation attempt below (initial +
    // empty-response retry + non-streaming path) — see `load_message_images`.
    let images = load_message_images(&app_data_dir, user_msg.metadata.as_ref()).await;

    // Extract character_id early — needed for embed calls and build_prompt
    let conv_character_id: Option<String> = conv.character_id.as_ref().map(|t| t.id.to_raw());

    // Resolve multi-character list for this conversation (empty = single-char mode).
    // Always prepend the conversation's own primary character — conv_chars has no
    // row for a plain solo conversation's own character at all, and even when it
    // does (a group-cast conversation), ORDER BY role ASC sorts "npc" before
    // "primary" alphabetically, so relying on list order for a `.first()` fallback
    // elsewhere is unsafe. Segment-name resolution (below, and in the streaming
    // Done-handler) needs the primary present in this list unconditionally, since
    // parsing now always runs (see the "Other Characters Present" prompt addition).
    let conv_chars = ConversationCharacterRepo::list(&db, &conversation_id).await.unwrap_or_default();
    let mut multi_char_names: Vec<String> = Vec::new();
    let mut multi_char_pairs: Vec<(String, String)> = Vec::new();
    if let Some(char_id) = conv_character_id.clone() {
        if let Ok(primary) = CharacterRepo::get(&db, &char_id).await {
            multi_char_names.push(primary.name.clone());
            multi_char_pairs.push((primary.name.clone(), char_id));
        }
    }
    for c in conv_chars.iter().filter(|c| c.is_active) {
        let id = c.character_id.id.to_raw();
        if multi_char_pairs.iter().any(|(_, existing)| existing == &id) {
            continue; // already added as the primary above
        }
        multi_char_names.push(c.character_name.clone());
        multi_char_pairs.push((c.character_name.clone(), id));
    }

    // 2. Build the prompt
    debug!("[send_message] building prompt...");

    // Get the active LLM provider early — we need context_length for the budget
    let provider_config = get_default_llm_provider(&db).await?;
    let model_id = resolve_model_id(model, &provider_config, &db).await?;

    // Background: embed user message for vector RAG
    spawn_embed_message(
        db.clone(), app.clone(),
        user_msg_id.clone(), conversation_id.clone(), content.clone(),
        conv_character_id.clone(),
    );

    let gen_params = GenerationParams {
        max_tokens: provider_config.config
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(2048) as u32,
        temperature: provider_config.config
            .get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8) as f32,
        ..Default::default()
    };

    let max_context = provider_config.config
        .get("context_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384) as usize;

    let context_budget = ContextBudget {
        max_context_tokens: max_context,
        reserved_for_response: gen_params.max_tokens as usize,
        ..Default::default()
    };

    let (messages, context_stats) = build_prompt(
        &db, &conversation_id, &user_msg_id,
        system_prompt.as_deref(), post_history_instructions.as_deref(),
        &context_budget,
    ).await?;
    debug!(
        "[send_message] prompt built with {} messages, context stats: {:?}",
        messages.len(), context_stats
    );

    // 3. Provider + model already resolved above (needed for context budget)

    // 4. Create the assistant message placeholder
    // MessageRepo::create also updates active_message_id to this new message.
    let assistant_msg = MessageRepo::create(
        &db,
        &conversation_id,
        "assistant",
        "",
        Some(&user_msg_id),
        None,
    ).await?;
    let assistant_msg_id = assistant_msg.id.id.to_raw();

    // 5. Stream or generate the response
    let use_streaming = streaming.unwrap_or(true);
    debug!("[send_message] streaming={}, model={}", use_streaming, model_id);

    if use_streaming {
        // --- Streaming path ---
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let provider = create_rig_provider(&provider_config)?;

        // Cloned before being moved into the producer task below — reused
        // once, further down, if the first attempt comes back with a
        // genuinely empty response (a real failure mode observed with
        // OpenRouter-routed free models under load: the upstream returns a
        // 502 "worker request limit reached" that surfaces as a stream
        // completing successfully with zero content, not a hard error).
        let retry_provider_config = provider_config.clone();
        let retry_model_id = model_id.clone();
        let retry_gen_params = gen_params.clone();

        // Spawn the provider stream in a background task
        let stream_messages = messages.clone();
        let retry_messages = stream_messages.clone();
        let stream_images = images.clone();
        let retry_images = stream_images.clone();
        let gen_task = tokio::spawn(async move {
            if let Err(e) = provider.generate_stream(
                &model_id,
                &stream_messages,
                &stream_images,
                &gen_params,
                tx.clone(),
            ).await {
                error!("Stream generation error: {}", e);
                // Send error through channel so frontend gets notified
                // instead of isStreaming hanging forever
                let _ = tx.send(StreamChunk::Error(format!("Stream failed: {}", e))).await;
            }
        });

        // Register this generation so cancel_generation can abort it and
        // persist whatever had streamed so far.
        let partial = Arc::new(std::sync::Mutex::new(String::new()));
        // Reasoning is tracked separately from `partial` — it's never used for
        // cancel-recovery (a cancelled generation with only a thinking trace
        // and no real reply isn't worth resuming into), only persisted once
        // the stream completes normally.
        let reasoning_acc = Arc::new(std::sync::Mutex::new(String::new()));
        let active_gens = state.read().await.active_generations.clone();
        active_gens.lock().await.insert(conversation_id.clone(), crate::GenerationHandle {
            abort: gen_task.abort_handle(),
            partial: Some(partial.clone()),
            assistant_message_id: assistant_msg_id.clone(),
        });

        // Forward stream chunks as Tauri events, saving the response, handling
        // multi-character attribution, etc. — shared with `retry_failed_message`
        // via `run_stream_completion`.
        let ctx = StreamCompletionCtx {
            rx,
            app,
            partial: partial.clone(),
            reasoning_acc: reasoning_acc.clone(),
            assist_id: assistant_msg_id.clone(),
            db_for_save: db.clone(),
            conv_id: conversation_id.clone(),
            active_gens_cleanup: active_gens.clone(),
            conv_id_cleanup: conversation_id.clone(),
            stream_char_id: conv_character_id.clone(),
            stream_mc_names: multi_char_names.clone(),
            stream_mc_pairs: multi_char_pairs.clone(),
            stream_user_msg_id: user_msg_id.clone(),
            stream_user_content: content.clone(),
            retry_provider_config,
            retry_model_id,
            retry_gen_params,
            retry_messages,
            retry_images,
            context_stats: Some(context_stats.clone()),
            origin: StreamOrigin::Send,
        };
        tokio::spawn(run_stream_completion(ctx));

        Ok(SendMessageResult {
            user_message_id: user_msg_id,
            assistant_message_id: assistant_msg_id,
        })
    } else {
        // --- Non-streaming path ---
        let provider = create_rig_provider(&provider_config)?;

        let db_for_save = db.clone();
        let conv_id = conversation_id.clone();
        let assist_id = assistant_msg_id.clone();

        // Spawned (rather than awaited directly) so cancel_generation can
        // abort it — there's no partial content to preserve here, unlike
        // the streaming path, since generate() only returns once complete.
        let model_id_gen = model_id.clone();
        let messages_gen = messages.clone();
        let images_gen = images.clone();
        let gen_task = tokio::spawn(async move {
            provider.generate(&model_id_gen, &messages_gen, &images_gen, &gen_params).await
        });
        let active_gens = state.read().await.active_generations.clone();
        active_gens.lock().await.insert(conversation_id.clone(), crate::GenerationHandle {
            abort: gen_task.abort_handle(),
            partial: None,
            assistant_message_id: assist_id.clone(),
        });

        let result = gen_task.await;
        active_gens.lock().await.remove(&conversation_id);

        match result {
            Ok(Ok(full_text)) => {
                // Save to database
                MessageRepo::update(&db_for_save, &assist_id, &full_text).await?;

                // Background: embed assistant message for vector RAG
                spawn_embed_message(
                    db_for_save.clone(), app.clone(),
                    assist_id.clone(), conv_id.clone(), full_text.clone(),
                    conv_character_id.clone(),
                );

                // Background: extract and update scene state (also triggers
                // NPC detection internally) — same as the streaming branch
                // above, so non-streaming responses update the scene bar too.
                spawn_scene_extraction(
                    db_for_save.clone(), app.clone(), conv_id.clone(), assist_id.clone(),
                    full_text.clone(),
                );

                // Emit as a single 'done' event
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "done".to_string(),
                    content: full_text,
                    message_id: assist_id,
                });

                info!("Non-streaming response completed for conversation {}", conv_id);
            }
            Ok(Err(e)) => {
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: e.to_string(),
                    message_id: assist_id,
                });
            }
            Err(join_err) if join_err.is_cancelled() => {
                // cancel_generation already aborted the task and emitted the
                // "cancelled" event — nothing left to do here.
            }
            Err(join_err) => {
                error!("Non-streaming generation task panicked: {}", join_err);
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: "Generation task failed unexpectedly".to_string(),
                    message_id: assist_id,
                });
            }
        }

        Ok(SendMessageResult {
            user_message_id: user_msg_id,
            assistant_message_id: assistant_msg_id,
        })
    }
}
