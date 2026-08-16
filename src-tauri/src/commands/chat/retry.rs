//! Retrying a failed message, regenerating a response, and cancelling an
//! in-flight generation.

use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::context::budget::ContextBudget;
use crate::context::prompt_builder::build_prompt;
use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::messages::MessageRepo;
use crate::error::MythicError;
use crate::models::conversation::{GenerationParams, MessageRole};
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};
use crate::providers::traits::StreamChunk;
use crate::AppState;

use super::attachments::load_message_images;
use super::pipeline::spawn_scene_extraction;
use super::send::send_message;
use super::streaming::{run_stream_completion, StreamCompletionCtx, StreamEvent, StreamOrigin};
use super::SendMessageResult;

/// Retries a failed message by reusing the existing user message already in the DB.
/// Cleans up the empty/failed assistant placeholder from the previous attempt,
/// creates a fresh one, and re-triggers LLM generation. This avoids duplicating
/// the user message in both the UI and database.
#[tauri::command]
#[specta::specta]
pub async fn retry_failed_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    user_message_id: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
) -> Result<SendMessageResult, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    debug!("[retry_failed_message] conversation={}, user_message={}", conversation_id, user_message_id);

    // Verify the user message exists and is a user message in this conversation
    let user_msg = MessageRepo::get(&db, &user_message_id).await
        .map_err(|_| MythicError::Validation(
            "User message not found — cannot retry".to_string()
        ))?;

    let msg_conv_id = user_msg.conversation_id.id.to_raw();
    if msg_conv_id != conversation_id || user_msg.role != MessageRole::User {
        return Err(MythicError::Validation(
            "User message not found — cannot retry".to_string()
        ));
    }

    // Replay whatever images were attached to the original send — this is
    // also how `regenerate_message` gets its images, since it delegates
    // here rather than re-collecting attachments itself (see its doc comment).
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let images = load_message_images(&app_data_dir, user_msg.metadata.as_ref()).await;

    // Delete any empty/failed assistant messages that were children of this user message.
    // No repo method covers this specific pattern, so use raw SurrealQL.
    db.query(
        "DELETE FROM messages WHERE parent_id = type::thing('messages', $parent_id) AND role = 'assistant' AND (content = '' OR content IS NONE)"
    )
    .bind(("parent_id", user_message_id.clone()))
    .await?;

    // Point the conversation back to the user message
    ConversationRepo::set_active_message(&db, &conversation_id, &user_message_id).await?;

    // Needed later for the multi-char-response event's emotion-pipeline
    // context (mirrors `content` in `send_message`, which has it as a param
    // directly — a retry only has the message ID, so it's fetched here).
    let user_message_content = MessageRepo::get(&db, &user_message_id).await
        .map(|m| m.content)
        .unwrap_or_default();

    // Resolve multi-character list for this conversation — a retried
    // response is a real AI turn like any other and needs the same
    // [Name]: segment parsing/attribution `send_message` does, or a
    // retried reply from a secondary character silently loses its name
    // badge (saved verbatim under the primary character). See the matching
    // block in `send_message` for the full rationale.
    let conv = ConversationRepo::get(&db, &conversation_id).await?;
    let conv_character_id: Option<String> = conv.character_id.as_ref().map(|t| t.id.to_raw());
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

    // Get LLM provider + model (needed for context budget)
    let provider_config = get_default_llm_provider(&db).await?;
    let model_id = resolve_model_id(model, &provider_config, &db).await?;

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

    // Build prompt from this user message
    let (messages, context_stats) = build_prompt(
        &db, &conversation_id, &user_message_id,
        system_prompt.as_deref(), post_history_instructions.as_deref(),
        &context_budget,
    ).await?;
    debug!(
        "[retry_failed_message] prompt built with {} messages, context stats: {:?}",
        messages.len(), context_stats
    );

    // Create fresh assistant placeholder
    // MessageRepo::create also updates active_message_id automatically.
    let assistant_msg = MessageRepo::create(
        &db,
        &conversation_id,
        "assistant",
        "",
        Some(&user_message_id),
        None,
    ).await?;
    let assistant_msg_id = assistant_msg.id.id.to_raw();

    // Stream or generate
    let use_streaming = streaming.unwrap_or(true);

    if use_streaming {
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);
        let provider = create_rig_provider(&provider_config)?;
        let stream_messages = messages.clone();
        let stream_images = images.clone();
        let stream_char_id = conv_character_id.clone();
        let stream_mc_names = multi_char_names.clone();
        let stream_mc_pairs = multi_char_pairs.clone();
        let stream_user_msg_id = user_message_id.clone();
        let stream_user_content = user_message_content.clone();

        // Cloned before being moved into the producer task below — see the
        // matching comment in `send_message` for why this exists (retrying
        // once, transparently, on a genuinely empty response).
        let retry_provider_config = provider_config.clone();
        let retry_model_id = model_id.clone();
        let retry_gen_params = gen_params.clone();
        let retry_messages = stream_messages.clone();
        let retry_images = stream_images.clone();

        let gen_task = tokio::spawn(async move {
            if let Err(e) = provider.generate_stream(
                &model_id, &stream_messages, &stream_images, &gen_params, tx.clone(),
            ).await {
                error!("Retry stream generation error: {}", e);
                let _ = tx.send(StreamChunk::Error(format!("Retry stream failed: {}", e))).await;
            }
        });

        let partial = Arc::new(std::sync::Mutex::new(String::new()));
        let reasoning_acc = Arc::new(std::sync::Mutex::new(String::new()));
        let active_gens = state.read().await.active_generations.clone();
        active_gens.lock().await.insert(conversation_id.clone(), crate::GenerationHandle {
            abort: gen_task.abort_handle(),
            partial: Some(partial.clone()),
            assistant_message_id: assistant_msg_id.clone(),
        });

        // Forward stream chunks as Tauri events — shared with `send_message`
        // via `run_stream_completion` (see its doc comment for why this
        // never triggers a rolling-summary pass: `context_stats: None`).
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
            stream_char_id,
            stream_mc_names,
            stream_mc_pairs,
            stream_user_msg_id,
            stream_user_content,
            retry_provider_config,
            retry_model_id,
            retry_gen_params,
            retry_messages,
            retry_images,
            context_stats: None,
            origin: StreamOrigin::Retry,
        };
        tokio::spawn(run_stream_completion(ctx));
    } else {
        let provider = create_rig_provider(&provider_config)?;
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
            assistant_message_id: assistant_msg_id.clone(),
        });
        let result = gen_task.await;
        active_gens.lock().await.remove(&conversation_id);

        match result {
            Ok(Ok(full_text)) => {
                MessageRepo::update(&db, &assistant_msg_id, &full_text).await?;
                // Background: extract and update scene state (also triggers
                // NPC detection internally) — see the matching comment on
                // the streaming branch above.
                spawn_scene_extraction(
                    db.clone(), app.clone(), conversation_id.clone(), assistant_msg_id.clone(),
                    full_text.clone(),
                );
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "done".to_string(),
                    content: full_text,
                    message_id: assistant_msg_id.clone(),
                });
            }
            Ok(Err(e)) => {
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: e.to_string(),
                    message_id: assistant_msg_id.clone(),
                });
            }
            Err(join_err) if join_err.is_cancelled() => {
                // cancel_generation already aborted the task and emitted the
                // "cancelled" event — nothing left to do here.
            }
            Err(join_err) => {
                error!("Retry non-streaming generation task panicked: {}", join_err);
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: "Generation task failed unexpectedly".to_string(),
                    message_id: assistant_msg_id.clone(),
                });
            }
        }
    }

    Ok(SendMessageResult {
        user_message_id: user_message_id,
        assistant_message_id: assistant_msg_id,
    })
}

/// Regenerates the AI response for a given message by re-running generation
/// from the same parent point in the conversation tree.
#[tauri::command]
#[specta::specta]
pub async fn regenerate_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
) -> Result<SendMessageResult, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    // A multi-character reply is stored as a chain of assistant messages —
    // user -> segment[0] -> segment[1] -> ... — all belonging to the SAME
    // turn. Regenerating any one segment previously deleted only that exact
    // message and passed ITS OWN parent_id to retry_failed_message, which
    // silently misbehaved two ways:
    //   - Regenerating segment[1]+ : parent is another ASSISTANT message,
    //     which retry_failed_message rejects (it requires a user parent) —
    //     the segment was already deleted by the time the error came back,
    //     a permanent, silent data loss.
    //   - Regenerating segment[0] itself: succeeded, but left segment[1..]
    //     orphaned (parent_id pointing at the now-deleted segment[0]),
    //     silently vanishing from history since get_branch just stops
    //     walking the moment a parent lookup returns nothing.
    //
    // Fix: walk up to the real user message this whole turn is chained from
    // (skipping over any assistant-role ancestors), and walk down from the
    // clicked message to sweep up every assistant-role continuation after
    // it too — stopping the moment a REAL subsequent user turn is hit, so
    // actual later conversation history is never touched. Then delete the
    // entire turn (not just one segment) and regenerate it as a whole from
    // the user message, which is the only granularity multi-character
    // generation actually supports — there's no way to regenerate a single
    // segment in isolation without re-running the whole turn.
    let msg = MessageRepo::get(&db, &message_id).await?;

    let mut turn_ids: Vec<String> = vec![message_id.clone()];

    // Walk up through assistant-role ancestors to find the user parent.
    let mut root_user_id: Option<String> = None;
    let mut walk_id = msg.parent_id.as_ref().map(|t| t.id.to_raw());
    while let Some(ref pid) = walk_id {
        match MessageRepo::get(&db, pid).await {
            Ok(parent_msg) => {
                if parent_msg.role == MessageRole::User {
                    root_user_id = Some(pid.clone());
                    break;
                }
                turn_ids.push(pid.clone());
                walk_id = parent_msg.parent_id.as_ref().map(|t| t.id.to_raw());
            }
            Err(_) => break,
        }
    }

    // Walk down through assistant-role continuations chained after the
    // clicked message (later segments of the same multi-character turn).
    let mut frontier = vec![message_id.clone()];
    while let Some(current_id) = frontier.pop() {
        let mut result = db
            .query("SELECT * FROM messages WHERE parent_id = type::thing('messages', $id)")
            .bind(("id", current_id))
            .await?;
        let children: Vec<crate::models::conversation::Message> = result.take(0).unwrap_or_default();
        for child in children {
            if child.role == MessageRole::Assistant {
                let child_id = child.id.id.to_raw();
                turn_ids.push(child_id.clone());
                frontier.push(child_id);
            }
            // A user-role child is the next real turn — leave it alone.
        }
    }

    // Delete the whole turn (all collected segments) before regenerating.
    for id in &turn_ids {
        if let Err(e) = MessageRepo::delete(&db, id).await {
            warn!("[regenerate_message] Failed to delete segment {} while regenerating: {}", id, e);
        }
    }

    match root_user_id {
        Some(pid) => {
            // Regenerate a fresh assistant reply FROM the existing user
            // message. This must NOT go through send_message — it
            // unconditionally creates a brand-new user message, which
            // produced a duplicate user bubble every time a reply was
            // regenerated. retry_failed_message already does exactly the
            // right thing here: build the prompt from the given user
            // message, create only a new assistant placeholder tied to it,
            // and stream a response — it also points active_message_id back
            // to this user message itself, so that doesn't need repeating.
            retry_failed_message(app, state, conversation_id, pid, model, system_prompt, streaming, post_history_instructions).await
        }
        None => {
            // No parent (e.g. regenerating a greeting that was never a
            // response to a user message) — preserve the old behavior of
            // re-sending empty content through the normal send flow.
            send_message(app, state, conversation_id, String::new(), model, system_prompt, streaming, post_history_instructions, None).await
        }
    }
}

/// Cancels the in-flight generation for a conversation, if any.
///
/// Aborts the generation task and, for a streaming generation, persists
/// whatever content had already streamed so the response isn't lost just
/// because it was stopped early — a cancelled message is not the same as a
/// failed one. Emits a "cancelled" chat-stream event (not "done" or "error")
/// so the frontend can finalize the UI without triggering the auto-memory-
/// extraction/emotion-update pipelines that "done" runs, since those
/// shouldn't fire over content the user explicitly cut off.
///
/// A no-op (not an error) if nothing is in flight for this conversation —
/// e.g. the user clicked Stop just as generation finished on its own.
#[tauri::command]
#[specta::specta]
pub async fn cancel_generation(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<(), MythicError> {
    let (db, handle) = {
        let state_guard = state.read().await;
        let db = state_guard.db.clone();
        let mut gens = state_guard.active_generations.lock().await;
        let handle = gens.remove(&conversation_id);
        (db, handle)
    };

    let Some(handle) = handle else {
        return Ok(());
    };

    handle.abort.abort();

    let partial_text = handle.partial
        .as_ref()
        .and_then(|p| p.lock().ok().map(|s| s.clone()))
        .unwrap_or_default();

    if !partial_text.is_empty() {
        if let Err(e) = MessageRepo::update(&db, &handle.assistant_message_id, &partial_text).await {
            error!("Failed to save partial content on cancel: {}", e);
        }
    }

    let _ = app.emit("chat-stream", StreamEvent {
        event_type: "cancelled".to_string(),
        content: partial_text,
        message_id: handle.assistant_message_id,
    });

    Ok(())
}
