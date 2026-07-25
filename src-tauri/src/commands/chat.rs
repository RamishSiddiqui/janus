//! Chat command handler — orchestrates message sending, prompt building,
//! and streaming responses from LLM providers via Tauri events.

use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::context::budget::ContextBudget;
use crate::context::rag::{embed_and_store, query_relevant_context, query_relevant_memories};
use crate::context::response_parser::{parse_multi_character_response, resolve_character_id};
use crate::context::scene_extractor::extract_scene_state;
use crate::context::summary::generate_rolling_summary;
use crate::context::tokenizer::count_message_tokens;
use crate::context::window::apply_sliding_window;

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::db::characters::CharacterRepo;
use crate::db::character_state::CharacterStateRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::lorebook::LorebookRepo;
use crate::db::memories::MemoryRepo;
use crate::db::messages::MessageRepo;
use crate::db::providers::ProviderRepo;
use crate::db::scene_states::SceneStateRepo;
use crate::db::summaries::SummaryRepo;
use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::models::provider::{ProviderAdapter, ProviderConfig};
use crate::providers::unified::RigProvider;
use crate::providers::traits::StreamChunk;
use crate::AppState;

/// Payload emitted to the frontend via Tauri events during streaming.
#[derive(Clone, serde::Serialize)]
struct StreamEvent {
    /// "delta" | "done" | "error"
    event_type: String,
    /// The text content (delta text, full response, or error message)
    content: String,
    /// The message ID of the assistant response being built
    message_id: String,
}

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
) -> Result<SendMessageResult, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    let _http = state_guard.http_client.clone(); // retained for image providers
    drop(state_guard);

    debug!("[send_message] conversation={}, content_len={}", conversation_id, content.len());

    // 1. Save the user message
    // Get current active message as parent for branching
    let conv = ConversationRepo::get(&db, &conversation_id).await?;
    let parent_id: Option<String> = conv.active_message_id.as_ref().map(|t| t.id.to_raw());

    // MessageRepo::create generates a UUID, inserts the message, and updates
    // the conversation's active_message_id automatically.
    let user_msg = MessageRepo::create(
        &db,
        &conversation_id,
        "user",
        &content,
        parent_id.as_deref(),
        None,
    ).await?;
    let user_msg_id = user_msg.id.id.to_raw();

    // Extract character_id early — needed for embed calls and build_prompt
    let conv_character_id: Option<String> = conv.character_id.as_ref().map(|t| t.id.to_raw());

    // Resolve multi-character list for this conversation (empty = single-char mode)
    let conv_chars = ConversationCharacterRepo::list(&db, &conversation_id).await.unwrap_or_default();
    let multi_char_names: Vec<String> = conv_chars.iter()
        .filter(|c| c.is_active)
        .map(|c| c.character_name.clone())
        .collect();
    let multi_char_pairs: Vec<(String, String)> = conv_chars.iter()
        .filter(|c| c.is_active)
        .map(|c| (c.character_name.clone(), c.character_id.id.to_raw()))
        .collect();

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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let provider = create_rig_provider(&provider_config)?;

        // Spawn the provider stream in a background task
        let stream_messages = messages.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.generate_stream(
                &model_id,
                &stream_messages,
                &gen_params,
                tx.clone(),
            ).await {
                error!("Stream generation error: {}", e);
                // Send error through channel so frontend gets notified
                // instead of isStreaming hanging forever
                let _ = tx.send(StreamChunk::Error(format!("Stream failed: {}", e))).await;
            }
        });

    // Forward stream chunks as Tauri events
    let db_for_save = db.clone();
    let conv_id = conversation_id.clone();
    let assist_id = assistant_msg_id.clone();
    let context_stats_clone = context_stats.clone();
    let stream_char_id = conv_character_id.clone();
    let stream_mc_names = multi_char_names.clone();
    let stream_mc_pairs = multi_char_pairs.clone();
    let stream_user_msg_id = user_msg_id.clone();

    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(text) => {
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "delta".to_string(),
                        content: text,
                        message_id: assist_id.clone(),
                    });
                }
                StreamChunk::Done(full_text) => {
                    // Save the complete response to the database
                    if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                        error!("Failed to save response: {}", e);
                    }

                    // ── Multi-character response parsing ──
                    let mut multi_char_handled = false;
                    // If this is a multi-char conversation, parse the response into
                    // per-character segments and create individual messages.
                    if stream_mc_names.len() > 1 {
                        let fallback = stream_mc_names.first()
                            .cloned()
                            .unwrap_or_else(|| "Character".to_string());

                        let segments = parse_multi_character_response(
                            &full_text, &stream_mc_names, &fallback,
                        );

                        if segments.len() > 1 {
                            info!("[multi-char] Parsed {} character segments", segments.len());

                            // Delete the combined parent message — it will be replaced
                            // by individual per-character messages chained sequentially.
                            if let Err(e) = MessageRepo::delete(&db_for_save, &assist_id).await {
                                warn!("[multi-char] Failed to delete combined parent message {}: {}", assist_id, e);
                            }

                            // Create individual character messages in a chain:
                            // user_msg → segment[0] → segment[1] → … → segment[N]
                            let mut prev_parent = stream_user_msg_id.clone();
                            for segment in &segments {
                                // Resolve character ID — fall back to primary character
                                // for unrecognized names (e.g., LLM wrote [Narrator]:)
                                // to avoid silently dropping content
                                let (char_id, full_name) = if let Some(cid) = resolve_character_id(
                                    &segment.character_name, &stream_mc_pairs,
                                ) {
                                    let name = stream_mc_pairs.iter()
                                        .find(|(_, id)| *id == cid)
                                        .map(|(name, _)| name.clone())
                                        .unwrap_or_else(|| segment.character_name.clone());
                                    (cid, name)
                                } else {
                                    // Fallback: attribute to primary character
                                    warn!("[multi-char] Unrecognized character '{}', attributing to primary", segment.character_name);
                                    let fallback_id = stream_mc_pairs.first()
                                        .map(|(_, id)| id.clone())
                                        .unwrap_or_default();
                                    let fallback_name = stream_mc_pairs.first()
                                        .map(|(name, _)| name.clone())
                                        .unwrap_or_else(|| segment.character_name.clone());
                                    (fallback_id, fallback_name)
                                };

                                match MessageRepo::create_with_character(
                                    &db_for_save,
                                    &conv_id,
                                    "assistant",
                                    &segment.content,
                                    Some(&prev_parent),
                                    &char_id,
                                    &full_name,
                                ).await {
                                    Ok(created) => {
                                        prev_parent = created.id.id.to_raw();
                                    }
                                    Err(e) => {
                                        warn!("[multi-char] Failed to create segment for {}: {}", full_name, e);
                                    }
                                }
                            }

                            multi_char_handled = true;

                            // Emit multi-char event for frontend rendering
                            let _ = app.emit("multi-char-response", serde_json::json!({
                                "conversation_id": conv_id,
                                "segments": segments,
                                "parent_message_id": assist_id,
                            }));
                        } else if segments.len() == 1 {
                            // Single character responded — still need to strip the
                            // [CharName]: prefix and set character attribution on the
                            // existing message so the UI shows the name badge properly.
                            let seg = &segments[0];
                            if let Some(char_id) = resolve_character_id(
                                &seg.character_name, &stream_mc_pairs,
                            ) {
                                info!("[multi-char] Single segment by {}, updating in-place", seg.character_name);
                                let _ = db_for_save.query(
                                    "UPDATE type::thing('messages', $id) SET content = $content, character_id = type::thing('characters', $char_id), character_name = $char_name"
                                )
                                    .bind(("id", assist_id.clone()))
                                    .bind(("content", seg.content.clone()))
                                    .bind(("char_id", char_id))
                                    .bind(("char_name", seg.character_name.clone()))
                                    .await;

                                multi_char_handled = true;

                                // Emit single-segment event so the frontend updates the live message
                                let _ = app.emit("multi-char-response", serde_json::json!({
                                    "conversation_id": conv_id,
                                    "segments": segments,
                                    "parent_message_id": assist_id,
                                }));
                            }
                        }
                    }

                    // Background: embed assistant message for vector RAG
                    spawn_embed_message(
                        db_for_save.clone(), app.clone(),
                        assist_id.clone(), conv_id.clone(), full_text.clone(),
                        stream_char_id.clone(),
                    );

                    // Background: extract and update scene state from the AI response.
                    // Runs a cheap secondary LLM call (max_tokens=300, temp=0.1) to parse
                    // location/time/weather/characters from the narrative.
                    {
                        let db_scene = db_for_save.clone();
                        let scene_conv_id = conv_id.clone();
                        let scene_response = full_text.clone();
                        let app_scene = app.clone();
                        tokio::spawn(async move {
                            if let Ok(provider_config) = get_default_llm_provider(&db_scene).await {
                                if let Ok(provider) = create_rig_provider(&provider_config) {
                                    let model = provider_config.config
                                        .get("model")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("default")
                                        .to_string();

                                    // Get current scene state as JSON for context
                                    let current_json = match SceneStateRepo::get(&db_scene, &scene_conv_id).await {
                                        Ok(Some(s)) => serde_json::to_string(&s).ok(),
                                        _ => None,
                                    };

                                    match extract_scene_state(
                                        &provider, &model, &scene_response,
                                        current_json.as_deref(),
                                    ).await {
                                        Ok(update) => {
                                            let changed = update.scene_changed;
                                            if let Ok(new_state) = SceneStateRepo::upsert(
                                                &db_scene, &scene_conv_id, &update
                                            ).await {
                                                info!("[scene_flow] Updated scene: {} (changed={})",
                                                    new_state.location_name, changed);
                                                if changed {
                                                    let _ = app_scene.emit("scene_state_changed",
                                                        serde_json::to_value(&new_state).unwrap_or_default());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            debug!("[scene_flow] Extraction failed (non-fatal): {}", e);
                                        }
                                    }
                                }
                            }
                        });
                    }

                    // When multi-char segments were processed, emit done
                    // with empty content — the multi-char-response event
                    // already handled rendering. Sending full_text here would
                    // cause the frontend done handler to re-create the combined
                    // message and show duplicates.
                    let done_content = if multi_char_handled {
                        String::new()
                    } else {
                        full_text
                    };
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "done".to_string(),
                        content: done_content,
                        message_id: assist_id.clone(),
                    });

                    info!("Chat response completed for conversation {}", conv_id);

                    // Trigger background summary generation if messages were evicted
                    if context_stats_clone.evicted_messages > 0 {
                        let db_summary = db_for_save.clone();
                        let conv_summary = conv_id.clone();
                        let assist_summary = assist_id.clone();
                        let evicted_n = context_stats_clone.evicted_messages;

                        tokio::spawn(async move {
                            // Debounce: only summarize if >= 10 new evictions since last summary
                            let existing = SummaryRepo::get(&db_summary, &conv_summary).await.ok().flatten();
                            let prev_covered = existing.as_ref().map(|s| s.covered_message_count).unwrap_or(0);

                            if evicted_n as u32 > prev_covered && (evicted_n as u32 - prev_covered) < 10 && existing.is_some() {
                                return; // Not enough new evictions yet
                            }

                            // Re-fetch the full branch to get evicted messages
                            let branch = match MessageRepo::get_branch(&db_summary, &assist_summary).await {
                                Ok(b) => b,
                                Err(e) => {
                                    warn!("[summary] Failed to fetch branch for conversation {}: {}", conv_summary, e);
                                    return;
                                }
                            };
                            if branch.len() <= evicted_n {
                                return;
                            }

                            let evicted: Vec<ChatMessage> = branch[..evicted_n]
                                .iter()
                                .map(|m| ChatMessage {
                                    role: m.role.clone(),
                                    content: m.content.clone(),
                                })
                                .collect();

                            let provider_config = match get_default_llm_provider(&db_summary).await {
                                Ok(pc) => pc,
                                Err(e) => {
                                    warn!("[summary] No LLM provider available for conversation {}: {}", conv_summary, e);
                                    return;
                                }
                            };
                            let provider = match create_rig_provider(&provider_config) {
                                Ok(p) => p,
                                Err(e) => {
                                    warn!("[summary] Failed to create provider for conversation {}: {}", conv_summary, e);
                                    return;
                                }
                            };

                            let model = provider_config.config
                                .get("model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("default")
                                .to_string();
                            let window_start_id = branch.get(evicted_n).map(|m| m.id.id.to_raw());

                            if let Err(e) = generate_rolling_summary(
                                &db_summary,
                                &provider,
                                &model,
                                &conv_summary,
                                &evicted,
                                existing.as_ref().map(|s| s.summary_text.as_str()),
                                window_start_id.as_deref(),
                            ).await {
                                warn!("[summary] Failed to generate rolling summary for conversation {}: {}", conv_summary, e);
                            }
                        });
                    }

                    break;
                }
                StreamChunk::Error(err) => {
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "error".to_string(),
                        content: err,
                        message_id: assist_id.clone(),
                    });
                    break;
                }
            }
        }
    });

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

        match provider.generate(&model_id, &messages, &gen_params).await {
            Ok(full_text) => {
                // Save to database
                MessageRepo::update(&db_for_save, &assist_id, &full_text).await?;

                // Background: embed assistant message for vector RAG
                spawn_embed_message(
                    db_for_save.clone(), app.clone(),
                    assist_id.clone(), conv_id.clone(), full_text.clone(),
                    conv_character_id.clone(),
                );

                // Emit as a single 'done' event
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "done".to_string(),
                    content: full_text,
                    message_id: assist_id,
                });

                info!("Non-streaming response completed for conversation {}", conv_id);
            }
            Err(e) => {
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: e.to_string(),
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

    // Delete any empty/failed assistant messages that were children of this user message.
    // No repo method covers this specific pattern, so use raw SurrealQL.
    db.query(
        "DELETE FROM messages WHERE parent_id = type::thing('messages', $parent_id) AND role = 'assistant' AND (content = '' OR content IS NONE)"
    )
    .bind(("parent_id", user_message_id.clone()))
    .await?;

    // Point the conversation back to the user message
    ConversationRepo::set_active_message(&db, &conversation_id, &user_message_id).await?;

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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);
        let provider = create_rig_provider(&provider_config)?;
        let stream_messages = messages.clone();

        tokio::spawn(async move {
            if let Err(e) = provider.generate_stream(
                &model_id, &stream_messages, &gen_params, tx.clone(),
            ).await {
                error!("Retry stream generation error: {}", e);
                let _ = tx.send(StreamChunk::Error(format!("Retry stream failed: {}", e))).await;
            }
        });

        let db_for_save = db.clone();
        let conv_id = conversation_id.clone();
        let assist_id = assistant_msg_id.clone();

        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Delta(text) => {
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "delta".to_string(),
                            content: text,
                            message_id: assist_id.clone(),
                        });
                    }
                    StreamChunk::Done(full_text) => {
                        if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                            error!("Failed to save retry response: {}", e);
                        }
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "done".to_string(),
                            content: full_text,
                            message_id: assist_id.clone(),
                        });
                        info!("Retry response completed for conversation {}", conv_id);
                        break;
                    }
                    StreamChunk::Error(err) => {
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "error".to_string(),
                            content: err,
                            message_id: assist_id.clone(),
                        });
                        break;
                    }
                }
            }
        });
    } else {
        let provider = create_rig_provider(&provider_config)?;
        match provider.generate(&model_id, &messages, &gen_params).await {
            Ok(full_text) => {
                MessageRepo::update(&db, &assistant_msg_id, &full_text).await?;
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "done".to_string(),
                    content: full_text,
                    message_id: assistant_msg_id.clone(),
                });
            }
            Err(e) => {
                let _ = app.emit("chat-stream", StreamEvent {
                    event_type: "error".to_string(),
                    content: e.to_string(),
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

    // Get the parent of the message to regenerate from
    let msg = MessageRepo::get(&db, &message_id).await?;
    let parent_id: Option<String> = msg.parent_id.as_ref().map(|t| t.id.to_raw());

    drop(state_guard);

    // Delete the old response
    MessageRepo::delete(&db, &message_id).await?;

    // If the message had a parent (it was an assistant response to a user message),
    // use the parent as the last user message
    if let Some(ref pid) = parent_id {
        // Update active to the parent so send_message builds from there
        ConversationRepo::set_active_message(&db, &conversation_id, pid).await?;
    }

    // Get the parent message content to re-send
    let parent_content: String = if let Some(ref pid) = parent_id {
        match MessageRepo::get(&db, pid).await {
            Ok(parent_msg) => parent_msg.content,
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    // Re-trigger send_message (which will create a new assistant response)
    // For regeneration, we just need to stream a new response from the same history
    send_message(app, state, conversation_id, parent_content, model, system_prompt, streaming, post_history_instructions).await
}

// --- Internal helpers ---

/// The IDs of the user/assistant message pair created by `send_message`,
/// `retry_failed_message`, or `regenerate_message` — the frontend uses
/// these to attach the streamed response to the right message bubbles.
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct SendMessageResult {
    pub user_message_id: String,
    pub assistant_message_id: String,
}

/// Statistics about the context window for observability.
/// Returned alongside the prompt so callers can log/surface token usage.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ContextStats {
    /// Total token budget for the context window.
    pub total_budget: usize,
    /// Tokens used by fixed layers (system, character, lorebook, memories, emotion, PHI).
    pub fixed_tokens: usize,
    /// Tokens used by conversation history (after sliding window).
    pub history_tokens: usize,
    /// Tokens used by the rolling summary (0 if no summary yet).
    pub summary_tokens: usize,
    /// Total messages in the full conversation branch.
    pub total_messages: usize,
    /// Messages included in the sliding window.
    pub included_messages: usize,
    /// Messages evicted (not sent to the LLM).
    pub evicted_messages: usize,
    /// Tokens used by RAG-retrieved context (0 if RAG disabled or no results).
    pub rag_tokens: usize,
}

/// Formats a list of memory facts as a bulleted block under `header` —
/// the shared shape behind every memory injection in `build_prompt()`
/// (per-character, semantic, recency-fallback, and no-character paths).
fn format_memory_block(header: &str, facts: &[String]) -> String {
    format!(
        "{}\n{}",
        header,
        facts.iter().map(|f| format!("• {}", f)).collect::<Vec<_>>().join("\n"),
    )
}

/// Builds the full prompt by combining system prompt, character data,
/// and conversation history. Now token-budgeted via sliding window.
async fn build_prompt(
    db: &Surreal<Db>,
    conversation_id: &str,
    up_to_message_id: &str,
    user_system_prompt: Option<&str>,
    post_history_instructions: Option<&str>,
    context_budget: &ContextBudget,
) -> Result<(Vec<ChatMessage>, ContextStats), MythicError> {
    let mut prompt = Vec::new();

    // Inject the user's global system prompt first (from Settings)
    if let Some(sys) = user_system_prompt {
        let trimmed = sys.trim();
        if !trimmed.is_empty() {
            prompt.push(ChatMessage {
                role: MessageRole::System,
                content: trimmed.to_string(),
            });
        }
    }

    // Get the character and memory scope associated with this conversation
    let conv = ConversationRepo::get(db, conversation_id).await.ok();

    let (character_id, memory_scope) = match conv {
        Some(ref c) => {
            let char_id = c.character_id.as_ref().map(|t| t.id.to_raw());
            (char_id, c.memory_scope.clone())
        }
        None => (None, "character".to_string()),
    };

    // ── Check for multi-character mode ──
    // If conversation_characters has entries, use multi-char prompt building.
    // Otherwise, fall back to the existing single-character path.
    let conv_chars = ConversationCharacterRepo::list(db, conversation_id).await.unwrap_or_default();
    let active_conv_chars: Vec<_> = conv_chars.iter().filter(|c| c.is_active).collect();
    let is_multi_char = !active_conv_chars.is_empty();

    if is_multi_char {
        // ── Multi-character mode: inject all character cards ──
        info!("[build_prompt] Multi-char mode: {} active characters", active_conv_chars.len());

        for conv_char in &active_conv_chars {
            let char_id_raw = conv_char.character_id.id.to_raw();
            if let Ok(character) = CharacterRepo::get(db, &char_id_raw).await {
                let card = &character.data;
                let name = &conv_char.character_name;
                let role = &conv_char.role;

                match role.as_str() {
                    "primary" => {
                        // Full character card for primary
                        let mut parts = Vec::new();
                        parts.push(format!("[Primary Character — {}]", name));

                        if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                            if !sys.is_empty() { parts.push(sys.to_string()); }
                        }
                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                        }
                        if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                            if !scenario.is_empty() { parts.push(format!("Scenario: {}", scenario)); }
                        }

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                    "secondary" => {
                        // Condensed card for secondary characters
                        let mut parts = Vec::new();
                        parts.push(format!("[Character — {}]", name));

                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                        }
                        parts.push(format!("(Talkativeness: {}/100)", conv_char.talkativeness));

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                    _ => {
                        // Minimal card for NPCs
                        let mut parts = Vec::new();
                        parts.push(format!("[NPC — {}]", name));

                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            if !desc.is_empty() { parts.push(desc.to_string()); }
                        }
                        parts.push("(Minor character — respond briefly when directly involved)".to_string());

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                }
            }
        }

        // ── Group Scene Directive ──
        let char_names: Vec<String> = active_conv_chars.iter()
            .map(|c| c.character_name.clone())
            .collect();
        let group_directive = format!(
            "[Group Scene Directive]\n\
             You are narrating a group roleplay scene. Multiple characters are present: {}.\n\
             When responding, write for ALL characters who are relevant to the current moment.\n\n\
             CRITICAL — Response format: You MUST prefix EVERY character's section with their full name \
             in square brackets followed by a colon. This is mandatory and must never be omitted.\n\n\
             Example format:\n\
             [Aria Silverleaf]: *Aria's actions and dialogue here*\n\n\
             [Finn Shadowcloak]: *Finn's actions and dialogue here*\n\n\
             Rules:\n\
             - EVERY section of your response MUST start with [FullCharacterName]: — never write \
             character dialogue or actions without this prefix tag\n\
             - When one character speaks TO or about another character present in the scene, \
             the addressed character MUST respond in the same generation. Do not wait for {{{{user}}}} input \
             between character exchanges\n\
             - Write natural back-and-forth dialogue between characters when the scene calls for it\n\
             - Only respond as characters listed above, never as {{{{user}}}}\n\
             - Each character must maintain their distinct voice, personality, and speech patterns\n\
             - Characters with higher talkativeness should respond more frequently and at greater length\n\
             - If a character has nothing meaningful to add, they may be omitted — but characters who are \
             directly addressed, challenged, or asked a question must always respond\n\
             - Never generate {{{{user}}}}'s dialogue or actions",
            char_names.join(", ")
        );
        prompt.push(ChatMessage {
            role: MessageRole::System,
            content: group_directive,
        });
    } else if let Some(ref char_id) = character_id {
        // ── Single-character mode (existing behavior) ──
        if let Ok(character) = CharacterRepo::get(db, char_id).await {
            let card = &character.data;
            let mut system_parts = Vec::new();

            // Character system prompt
            if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                if !sys.is_empty() {
                    system_parts.push(sys.to_string());
                }
            }

            // Character description
            if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                if !desc.is_empty() {
                    system_parts.push(format!("Character Description:\n{}", desc));
                }
            }

            // Personality
            if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                if !personality.is_empty() {
                    system_parts.push(format!("Personality:\n{}", personality));
                }
            }

            // Scenario
            if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                if !scenario.is_empty() {
                    system_parts.push(format!("Scenario:\n{}", scenario));
                }
            }

            if !system_parts.is_empty() {
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: system_parts.join("\n\n"),
                });
            }
        }
    }

    // Add lorebook entries — fetch all entries for this character, then filter in Rust
    if let Some(ref char_id) = character_id {
        if let Ok(all_entries) = LorebookRepo::list(db, char_id).await {
            // Always-active entries
            for entry in all_entries.iter() {
                if entry.enabled && entry.always_active {
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: entry.content.clone(),
                    });
                }
            }

            // Keyword-triggered entries will be processed after we build the message chain
            // (we need the chain to scan for keywords)

            // Walk the message tree using get_branch (returns root→leaf order)
            let branch = MessageRepo::get_branch(db, up_to_message_id).await?;
            let chain: Vec<ChatMessage> = branch.iter().map(|m| {
                ChatMessage {
                    role: match m.role {
                        MessageRole::User => MessageRole::User,
                        MessageRole::Assistant => MessageRole::Assistant,
                        MessageRole::System => MessageRole::System,
                    },
                    content: m.content.clone(),
                }
            }).collect();

            // Keyword-triggered lorebook entries: scan recent messages for matching keywords
            let keyword_entries: Vec<&crate::models::lorebook::LorebookEntry> = all_entries.iter()
                .filter(|e| e.enabled && !e.always_active && !e.keys.is_empty())
                .collect();

            if !keyword_entries.is_empty() {
                // Build a search corpus from the last N messages (for performance)
                let scan_depth = chain.len().min(20);
                let corpus: String = chain[chain.len().saturating_sub(scan_depth)..]
                    .iter()
                    .map(|m| m.content.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ");

                for entry in keyword_entries {
                    // keys is already Vec<String> — no JSON parsing needed
                    let triggered = entry.keys
                        .iter()
                        .map(|k| k.to_lowercase())
                        .filter(|k| !k.is_empty())
                        .any(|keyword| corpus.contains(&keyword));

                    if triggered {
                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: entry.content.clone(),
                        });
                    }
                }
            }

            // ── Inject saved memories as a persistent context layer ──
            // Two retrieval modes:
            // 1. Semantic: query vector DB for memories relevant to current message (preferred)
            // 2. Fallback: recency-ordered list if no memory embeddings exist yet
            //
            // Placement: after lorebook, before emotional state — the "knowledge layer".
            // Token-capped to context_budget.max_memory_tokens.
            if memory_scope != "none" {
                // Get the last user message for semantic query
                let last_user_content = chain.iter().rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();

                if is_multi_char {
                    // ── Multi-character memory injection ──
                    // Each character gets their own attributed memory block
                    for conv_char in &active_conv_chars {
                        let cc_char_id = conv_char.character_id.id.to_raw();
                        let char_memories = MemoryRepo::list_for_character_in_conv(
                            db, &cc_char_id, conversation_id,
                        ).await.unwrap_or_default();

                        if !char_memories.is_empty() {
                            let facts: Vec<String> = char_memories.iter()
                                .take(10) // cap per character
                                .map(|m| m.content.clone())
                                .collect();

                            let memory_block = format_memory_block(
                                &format!("[{}'s Memories]", conv_char.character_name),
                                &facts,
                            );
                            prompt.push(ChatMessage {
                                role: MessageRole::System,
                                content: memory_block,
                            });
                        }
                    }
                } else {
                    // ── Single-character memory injection (existing behavior) ──
                    let mut memory_facts: Vec<String> = Vec::new();

                    // Try semantic retrieval first
                    if !last_user_content.is_empty() {
                        if let Ok(pc) = get_default_llm_provider(db).await {
                            if let Ok(provider) = create_rig_provider(&pc) {
                                let embed_model = pc.config
                                    .get("embedding_model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("text-embedding-3-small");
                                if let Ok(results) = query_relevant_memories(
                                    db, &provider, embed_model,
                                    char_id, &last_user_content,
                                    10,   // top 10 relevant memories
                                    0.4,  // lower threshold — facts are short
                                ).await {
                                    if !results.is_empty() {
                                        memory_facts = results.iter()
                                            .map(|r| r.content.clone())
                                            .collect();
                                        info!("[build_prompt] Semantic memory retrieval: {} memories", memory_facts.len());
                                    }
                                }
                            }
                        }
                    }

                    // Fallback: recency-ordered list if semantic retrieval yielded nothing
                    if memory_facts.is_empty() {
                        let memory_list = if memory_scope == "character" {
                            MemoryRepo::list(db, Some(char_id), None).await.unwrap_or_default()
                        } else {
                            // Fix: conversation scope now includes canon memories
                            MemoryRepo::list_with_canon(db, conversation_id).await.unwrap_or_default()
                        };

                        memory_facts = memory_list.into_iter()
                            .take(15)
                            .map(|m| m.content)
                            .collect();

                        if !memory_facts.is_empty() {
                            memory_facts.reverse(); // oldest first for natural reading
                            info!("[build_prompt] Recency memory fallback: {} memories", memory_facts.len());
                        }
                    }

                    if !memory_facts.is_empty() {
                        let memory_block = format_memory_block(
                            "[Remembered Facts — things you know from past interactions]",
                            &memory_facts,
                        );

                        let memory_message = ChatMessage {
                            role: MessageRole::System,
                            content: memory_block,
                        };

                        // Enforce token cap — truncate if over budget
                        let mem_tokens = count_message_tokens(&memory_message);
                        if mem_tokens <= context_budget.max_memory_tokens {
                            prompt.push(memory_message);
                        } else {
                            // Progressively drop facts until under budget
                            let mut facts = memory_facts;
                            while facts.len() > 1 {
                                facts.pop();
                                let truncated_block = format_memory_block(
                                    "[Remembered Facts — things you know from past interactions]",
                                    &facts,
                                );
                                let truncated_msg = ChatMessage {
                                    role: MessageRole::System,
                                    content: truncated_block,
                                };
                                if count_message_tokens(&truncated_msg) <= context_budget.max_memory_tokens {
                                    prompt.push(truncated_msg);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Inject character emotional state as a dynamic context layer.
            // Placed last in the system prompt so it's closest to the conversation history
            // and carries the most weight in the attention window.
            if is_multi_char {
                // ── Multi-character emotional states ──
                let mut state_parts: Vec<String> = Vec::new();
                for conv_char in &active_conv_chars {
                    let cc_char_id = conv_char.character_id.id.to_raw();
                    if let Ok(Some(state)) = CharacterStateRepo::get(db, &cc_char_id, conversation_id).await {
                        state_parts.push(format!(
                            "  {} — {} (mood:{}/100 trust:{}/100 intensity:{}/100) — {}",
                            conv_char.character_name,
                            state.dominant_emotion,
                            state.mood, state.trust, state.arousal,
                            state.state_summary,
                        ));
                    }
                }
                if !state_parts.is_empty() {
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!(
                            "[Current Emotional States]\n{}\n(Let these emotional states colour each character's response naturally.)",
                            state_parts.join("\n")
                        ),
                    });
                }
            } else if let Ok(Some(state)) = CharacterStateRepo::get(db, char_id, conversation_id).await {
                let state_block = format!(
                    "[Current Emotional State]\n\
                     Dominant emotion: {emotion}\n\
                     Mood: {mood}/100  Trust: {trust}/100  Intensity: {arousal}/100\n\
                     Internal state: {summary}\n\
                     (Let this emotional state colour your response naturally — do not announce or describe it explicitly.)",
                    emotion = state.dominant_emotion,
                    mood = state.mood,
                    trust = state.trust,
                    arousal = state.arousal,
                    summary = state.state_summary,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: state_block,
                });
            }

            // ── Inject current scene state ──
            // Placed after emotional state, before summary — gives the AI spatial awareness
            // of where the story is happening (location, time, weather, characters present).
            if let Ok(Some(scene)) = SceneStateRepo::get(db, conversation_id).await {
                let chars_list = if scene.characters_present.is_empty() {
                    "unspecified".to_string()
                } else {
                    scene.characters_present.join(", ")
                };
                let scene_block = format!(
                    "[Current Scene]\n\
                     Location: {name} — {desc}\n\
                     Time: {time} | Weather: {weather}\n\
                     Present: {chars}\n\
                     Atmosphere: {ambient}\n\
                     (Maintain scene consistency. Describe transitions naturally when the story moves to a new location or time.)",
                    name = scene.location_name,
                    desc = scene.location_description,
                    time = scene.time_period,
                    weather = scene.weather,
                    chars = chars_list,
                    ambient = scene.ambient_details,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: scene_block,
                });
            }

            // ── Inject rolling summary (if exists) ──
            let summary = SummaryRepo::get(db, conversation_id).await.ok().flatten();
            let summary_tokens = if let Some(ref s) = summary {
                let summary_message = ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "[Story So Far — summary of earlier conversation]\n{}",
                        s.summary_text
                    ),
                };
                let tokens = count_message_tokens(&summary_message);
                prompt.push(summary_message);
                tokens
            } else {
                0
            };

            // ── Apply sliding window to conversation history ──
            // Build the PHI message (if any) so it can be counted in the budget
            let phi_message = post_history_instructions.and_then(|phi| {
                let trimmed = phi.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ChatMessage {
                        role: MessageRole::System,
                        content: trimmed.to_string(),
                    })
                }
            });

            // Collect fixed layers for budget calculation (everything in prompt so far + PHI)
            let mut fixed_for_budget: Vec<ChatMessage> = prompt.clone();
            if let Some(ref phi) = phi_message {
                fixed_for_budget.push(phi.clone());
            }

            let allocation = context_budget.allocate(&fixed_for_budget);
            let window = apply_sliding_window(&chain, allocation.messages_budget);

            info!(
                "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
                allocation.fixed_layers_tokens + window.included_tokens,
                context_budget.max_context_tokens,
                window.included.len(),
                chain.len(),
                window.evicted_count,
            );

            let total_messages = chain.len();
            let included_messages = window.included.len();
            let evicted_messages = window.evicted_count;
            let history_tokens = window.included_tokens;

            prompt.extend(window.included);

            // Vector RAG: retrieve semantically relevant evicted messages
            // Fixes applied:
            // - Dedup: exclude messages already in the sliding window
            // - Budget: cap RAG injection to allocation.rag_budget tokens
            // - Cross-conv: when memory_scope = "character", search all conversations
            let rag_tokens = if window.evicted_count > 0 {
                let last_user_content = chain.iter().rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();

                if !last_user_content.is_empty() {
                    // Collect IDs of messages in the sliding window for dedup
                    let include_from = chain.len().saturating_sub(included_messages);
                    let window_msg_ids: Vec<String> = branch[include_from..]
                        .iter()
                        .map(|m| m.id.id.to_raw())
                        .collect();

                    let rag_results = match get_default_llm_provider(db).await {
                        Ok(pc) => match create_rig_provider(&pc) {
                            Ok(provider) => {
                                let embed_model = pc.config
                                    .get("embedding_model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("text-embedding-3-small");

                                // Choose scope: character-wide or conversation-only
                                let (conv_scope, char_scope) = if memory_scope == "character" {
                                    (None, character_id.as_deref())
                                } else {
                                    (Some(conversation_id as &str), None)
                                };

                                query_relevant_context(
                                    db, &provider, embed_model,
                                    conv_scope, char_scope,
                                    &last_user_content,
                                    5, 0.7, &window_msg_ids,
                                ).await.unwrap_or_default()
                            }
                            Err(_) => vec![],
                        },
                        Err(_) => vec![],
                    };

                    if !rag_results.is_empty() {
                        let rag_text = rag_results.iter()
                            .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let rag_message = ChatMessage {
                            role: MessageRole::System,
                            content: format!(
                                "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                                rag_text
                            ),
                        };
                        let tokens = count_message_tokens(&rag_message);

                        // Enforce RAG budget cap
                        if tokens <= allocation.rag_budget {
                            prompt.push(rag_message);
                            tokens
                        } else {
                            // Try with fewer results until within budget
                            let mut truncated = rag_results.clone();
                            while truncated.len() > 1 {
                                truncated.pop();
                                let text = truncated.iter()
                                    .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                let msg = ChatMessage {
                                    role: MessageRole::System,
                                    content: format!(
                                        "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                                        text
                                    ),
                                };
                                let t = count_message_tokens(&msg);
                                if t <= allocation.rag_budget {
                                    prompt.push(msg);
                                    break;
                                }
                            }
                            // Return whatever tokens we actually used
                            prompt.last()
                                .map(|m| count_message_tokens(m))
                                .unwrap_or(0)
                        }
                    } else { 0 }
                } else { 0 }
            } else { 0 };

            // PHI goes last — after history, maximum attention weight
            if let Some(phi) = phi_message {
                prompt.push(phi);
            }

            return Ok((prompt, ContextStats {
                total_budget: context_budget.max_context_tokens,
                fixed_tokens: allocation.fixed_layers_tokens,
                history_tokens,
                summary_tokens,
                total_messages,
                included_messages,
                evicted_messages,
                rag_tokens,
            }));
        } else {
            // Lorebook fetch failed — still build the message chain
            let branch = MessageRepo::get_branch(db, up_to_message_id).await?;
            let chain: Vec<ChatMessage> = branch.iter().map(|m| {
                ChatMessage {
                    role: match m.role {
                        MessageRole::User => MessageRole::User,
                        MessageRole::Assistant => MessageRole::Assistant,
                        MessageRole::System => MessageRole::System,
                    },
                    content: m.content.clone(),
                }
            }).collect();

            // Memories (character-less path or fallback)
            if memory_scope != "none" {
                // Use list_with_canon to include canon facts regardless of scope
                let memory_list = MemoryRepo::list_with_canon(db, conversation_id).await.unwrap_or_default();
                let memory_rows: Vec<_> = memory_list.into_iter().take(15).collect();
                if !memory_rows.is_empty() {
                    let mut facts: Vec<String> = memory_rows.into_iter().map(|m| m.content).collect();
                    facts.reverse();
                    let memory_block = format_memory_block(
                        "[Remembered Facts — things you know from past interactions]",
                        &facts,
                    );
                    let memory_message = ChatMessage {
                        role: MessageRole::System,
                        content: memory_block,
                    };
                    // Enforce token cap
                    if count_message_tokens(&memory_message) <= context_budget.max_memory_tokens {
                        prompt.push(memory_message);
                    }
                }
            }

            // ── Inject rolling summary (if exists) ──
            let summary = SummaryRepo::get(db, conversation_id).await.ok().flatten();
            let summary_tokens = if let Some(ref s) = summary {
                let summary_message = ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "[Story So Far — summary of earlier conversation]\n{}",
                        s.summary_text
                    ),
                };
                let tokens = count_message_tokens(&summary_message);
                prompt.push(summary_message);
                tokens
            } else {
                0
            };

            // ── Apply sliding window (lorebook-fetch-failed branch) ──
            let phi_message = post_history_instructions.and_then(|phi| {
                let trimmed = phi.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ChatMessage {
                        role: MessageRole::System,
                        content: trimmed.to_string(),
                    })
                }
            });

            let mut fixed_for_budget: Vec<ChatMessage> = prompt.clone();
            if let Some(ref phi) = phi_message {
                fixed_for_budget.push(phi.clone());
            }

            let allocation = context_budget.allocate(&fixed_for_budget);
            let window = apply_sliding_window(&chain, allocation.messages_budget);

            info!(
                "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
                allocation.fixed_layers_tokens + window.included_tokens,
                context_budget.max_context_tokens,
                window.included.len(),
                chain.len(),
                window.evicted_count,
            );

            let total_messages = chain.len();
            let included_messages = window.included.len();
            let evicted_messages = window.evicted_count;
            let history_tokens = window.included_tokens;

            prompt.extend(window.included);

            // Vector RAG: retrieve semantically relevant evicted messages (fallback path)
            let rag_tokens = if window.evicted_count > 0 {
                let last_user_content = chain.iter().rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();

                if !last_user_content.is_empty() {
                    // Collect IDs of messages in the sliding window for dedup
                    let include_from = chain.len().saturating_sub(included_messages);
                    let window_msg_ids: Vec<String> = branch[include_from..]
                        .iter()
                        .map(|m| m.id.id.to_raw())
                        .collect();

                    let rag_results = match get_default_llm_provider(db).await {
                        Ok(pc) => match create_rig_provider(&pc) {
                            Ok(provider) => {
                                let embed_model = pc.config
                                    .get("embedding_model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("text-embedding-3-small");
                                query_relevant_context(
                                    db, &provider, embed_model,
                                    Some(conversation_id), None,
                                    &last_user_content,
                                    5, 0.7, &window_msg_ids,
                                ).await.unwrap_or_default()
                            }
                            Err(_) => vec![],
                        },
                        Err(_) => vec![],
                    };

                    if !rag_results.is_empty() {
                        let rag_text = rag_results.iter()
                            .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let rag_message = ChatMessage {
                            role: MessageRole::System,
                            content: format!(
                                "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                                rag_text
                            ),
                        };
                        let tokens = count_message_tokens(&rag_message);
                        // Enforce RAG budget cap
                        if tokens <= allocation.rag_budget {
                            prompt.push(rag_message);
                            tokens
                        } else { 0 }
                    } else { 0 }
                } else { 0 }
            } else { 0 };

            if let Some(phi) = phi_message {
                prompt.push(phi);
            }

            return Ok((prompt, ContextStats {
                total_budget: context_budget.max_context_tokens,
                fixed_tokens: allocation.fixed_layers_tokens,
                history_tokens,
                summary_tokens,
                total_messages,
                included_messages,
                evicted_messages,
                rag_tokens,
            }));
        }
    } else {
        // No character — just build the message chain
        let branch = MessageRepo::get_branch(db, up_to_message_id).await?;
        let chain: Vec<ChatMessage> = branch.iter().map(|m| {
            ChatMessage {
                role: match m.role {
                    MessageRole::User => MessageRole::User,
                    MessageRole::Assistant => MessageRole::Assistant,
                    MessageRole::System => MessageRole::System,
                },
                content: m.content.clone(),
            }
        }).collect();

        // Conversation-scoped memories even without a character
        if memory_scope != "none" {
            let memory_list = MemoryRepo::list(db, None, Some(conversation_id)).await.unwrap_or_default();
            let memory_rows: Vec<_> = memory_list.into_iter().take(20).collect();
            if !memory_rows.is_empty() {
                let mut facts: Vec<String> = memory_rows.into_iter().map(|m| m.content).collect();
                facts.reverse();
                let memory_block = format_memory_block(
                    "[Remembered Facts — things you know from past interactions]",
                    &facts,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: memory_block,
                });
            }
        }

        // ── Inject rolling summary (if exists) ──
        let summary = SummaryRepo::get(db, conversation_id).await.ok().flatten();
        let summary_tokens = if let Some(ref s) = summary {
            let summary_message = ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "[Story So Far — summary of earlier conversation]\n{}",
                    s.summary_text
                ),
            };
            let tokens = count_message_tokens(&summary_message);
            prompt.push(summary_message);
            tokens
        } else {
            0
        };

        // ── Apply sliding window (no-character branch) ──
        let phi_message = post_history_instructions.and_then(|phi| {
            let trimmed = phi.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(ChatMessage {
                    role: MessageRole::System,
                    content: trimmed.to_string(),
                })
            }
        });

        let mut fixed_for_budget: Vec<ChatMessage> = prompt.clone();
        if let Some(ref phi) = phi_message {
            fixed_for_budget.push(phi.clone());
        }

        let allocation = context_budget.allocate(&fixed_for_budget);
        let window = apply_sliding_window(&chain, allocation.messages_budget);

        info!(
            "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
            allocation.fixed_layers_tokens + window.included_tokens,
            context_budget.max_context_tokens,
            window.included.len(),
            chain.len(),
            window.evicted_count,
        );

        let total_messages = chain.len();
        let included_messages = window.included.len();
        let evicted_messages = window.evicted_count;
        let history_tokens = window.included_tokens;

        prompt.extend(window.included);

        // Vector RAG: retrieve semantically relevant evicted messages (non-character path)
        let rag_tokens = if window.evicted_count > 0 {
            let last_user_content = chain.iter().rev()
                .find(|m| m.role == MessageRole::User)
                .map(|m| m.content.clone())
                .unwrap_or_default();

            if !last_user_content.is_empty() {
                let exclude_ids: Vec<String> = vec![];
                let rag_results = match get_default_llm_provider(db).await {
                    Ok(pc) => match create_rig_provider(&pc) {
                        Ok(provider) => {
                            let embed_model = pc.config
                                .get("embedding_model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("text-embedding-3-small");
                            query_relevant_context(
                                db, &provider, embed_model,
                                Some(conversation_id), None,
                                &last_user_content,
                                5, 0.7, &exclude_ids,
                            ).await.unwrap_or_default()
                        }
                        Err(_) => vec![],
                    },
                    Err(_) => vec![],
                };

                if !rag_results.is_empty() {
                    let rag_text = rag_results.iter()
                        .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    let rag_message = ChatMessage {
                        role: MessageRole::System,
                        content: format!(
                            "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                            rag_text
                        ),
                    };
                    let tokens = count_message_tokens(&rag_message);
                    if tokens <= allocation.rag_budget {
                        prompt.push(rag_message);
                        tokens
                    } else { 0 }
                } else { 0 }
            } else { 0 }
        } else { 0 };

        if let Some(phi) = phi_message {
            prompt.push(phi);
        }

        Ok((prompt, ContextStats {
            total_budget: context_budget.max_context_tokens,
            fixed_tokens: allocation.fixed_layers_tokens,
            history_tokens,
            summary_tokens,
            total_messages,
            included_messages,
            evicted_messages,
            rag_tokens,
        }))
    }
}

/// Finds the default LLM provider configuration from the database.
pub(crate) async fn get_default_llm_provider(
    db: &Surreal<Db>,
) -> Result<ProviderConfig, MythicError> {
    match ProviderRepo::get_default(db, "llm").await? {
        Some(config) => Ok(config),
        None => Err(MythicError::Config(
            "No LLM provider configured. Add one in Settings → Models.".to_string()
        )),
    }
}

/// Resolves the model ID to use, falling back through stored config then enabled models.
async fn resolve_model_id(
    model: Option<String>,
    provider_config: &ProviderConfig,
    db: &Surreal<Db>,
) -> Result<String, MythicError> {
    match model {
        Some(m) if !m.is_empty() && m != "unknown" => Ok(m),
        _ => {
            let stored = provider_config.config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !stored.is_empty() && stored != "unknown" {
                Ok(stored.to_string())
            } else {
                // Fall back to first enabled LLM model for this provider
                // (explicitly exclude embedding models)
                let provider_id_str = provider_config.id.id.to_raw();
                let enabled = ProviderRepo::list_enabled_models(db, Some(&provider_id_str)).await?;
                match enabled.into_iter().find(|m| m.model_type != "embedding") {
                    Some(m) => Ok(m.model_id),
                    None => Err(MythicError::Config(
                        "No chat model selected. Go to LLM Models, enable at least one non-embedding model.".to_string()
                    )),
                }
            }
        }
    }
}

/// Creates a unified rig-backed LLM provider from DB config.
pub(crate) fn create_rig_provider(config: &ProviderConfig) -> Result<RigProvider, MythicError> {
    // Convert enum to string for RigProvider::from_config
    let adapter_str = match config.adapter {
        ProviderAdapter::Ollama => "ollama",
        ProviderAdapter::OpenRouter => "openrouter",
        ProviderAdapter::OpenAiCompatible => "openai",
        ProviderAdapter::SiliconFlow => "openai", // OpenAI-compatible
        ProviderAdapter::HuggingFace => "huggingface",
        ProviderAdapter::Anthropic => "anthropic",
        ProviderAdapter::Gemini => "gemini",
        ProviderAdapter::Cohere => "cohere",
        ProviderAdapter::DeepSeek => "deepseek",
        ProviderAdapter::Groq => "groq",
        ProviderAdapter::Perplexity => "perplexity",
        ProviderAdapter::Xai => "xai",
        ProviderAdapter::Hyperbolic => "hyperbolic",
        ProviderAdapter::Moonshot => "moonshot",
        ProviderAdapter::Together => "together",
        ProviderAdapter::ComfyUi => return Err(MythicError::Config(
            "ComfyUI is an image provider, not an LLM provider".to_string()
        )),
    };

    let api_key = config.config.get("api_key").and_then(|v| v.as_str());
    let base_url = config.config.get("base_url").and_then(|v| v.as_str());

    RigProvider::from_config(adapter_str, api_key, base_url)
}

/// Resolves the default LLM provider and its configured embedding model in
/// one step — the shared first half of every background embed task.
async fn resolve_embedding_provider(db: &Surreal<Db>) -> Result<(RigProvider, String), MythicError> {
    let provider_config = get_default_llm_provider(db).await?;
    let provider = create_rig_provider(&provider_config)?;
    let embedding_model = provider_config.config
        .get("embedding_model")
        .and_then(|v| v.as_str())
        .unwrap_or("text-embedding-3-small")
        .to_string();
    Ok((provider, embedding_model))
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
                    &db, &provider, &embedding_model,
                    &message_id, &conversation_id, &content,
                    character_id.as_deref(),
                ).await {
                    Ok(_) => { let _ = app.emit("embedding_updated", ()); }
                    Err(e) => warn!("[embed] Failed to embed message {}: {}", message_id, e),
                }
            }
            Err(e) => warn!("[embed] No embedding provider available for message {}: {}", message_id, e),
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
                    &db, &provider, &embedding_model, &memory_id, &character_id, &content,
                ).await {
                    warn!("[embed] Failed to re-embed memory {}: {}", memory_id, e);
                }
            }
            Err(e) => warn!("[embed] No embedding provider available for memory {}: {}", memory_id, e),
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
    let result = provider.generate(&model_id, &messages, &gen_params).await?;

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
    let max_context = provider_config.config
        .get("context_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384) as usize;

    let max_tokens = provider_config.config
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
    ).await?;

    Ok(stats)
}
