//! Chat command handler — orchestrates message sending, prompt building,
//! and streaming responses from LLM providers via Tauri events.

use std::sync::Arc;
use tauri::{Emitter, Manager, State};
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
use crate::db::personas::PersonaRepo;
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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

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

    // Forward stream chunks as Tauri events
    let db_for_save = db.clone();
    let conv_id = conversation_id.clone();
    let assist_id = assistant_msg_id.clone();
    let context_stats_clone = context_stats.clone();
    let stream_char_id = conv_character_id.clone();
    let stream_mc_names = multi_char_names.clone();
    let stream_mc_pairs = multi_char_pairs.clone();
    let stream_user_msg_id = user_msg_id.clone();
    let active_gens_cleanup = active_gens.clone();
    let conv_id_cleanup = conversation_id.clone();

    tokio::spawn(async move {
        let mut attempted_retry = false;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(text) => {
                    if let Ok(mut p) = partial.lock() { p.push_str(&text); }
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "delta".to_string(),
                        content: text,
                        message_id: assist_id.clone(),
                    });
                }
                StreamChunk::ReasoningDelta(text) => {
                    if let Ok(mut r) = reasoning_acc.lock() { r.push_str(&text); }
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "reasoning".to_string(),
                        content: text,
                        message_id: assist_id.clone(),
                    });
                }
                StreamChunk::Done(full_text) => {
                    // Some providers fail "quietly" under load — the stream
                    // completes with zero content instead of a hard error
                    // (observed: OpenRouter-routed free models returning a
                    // 502 "worker request limit reached" that never
                    // surfaces as StreamChunk::Error). Retry once,
                    // transparently, before treating this as a real
                    // failure — exactly what a manual "Regenerate" click
                    // already does, just automatic.
                    if full_text.trim().is_empty() && !attempted_retry {
                        attempted_retry = true;
                        warn!(
                            "[send_message] Empty response for conversation {} — retrying once (likely a transient upstream overload)",
                            conv_id
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                        if let Ok(retry_provider) = create_rig_provider(&retry_provider_config) {
                            let (tx2, rx2) = tokio::sync::mpsc::channel::<StreamChunk>(64);
                            let rm = retry_model_id.clone();
                            let rmsgs = retry_messages.clone();
                            let rgp = retry_gen_params.clone();
                            let rimgs = retry_images.clone();
                            let tx2c = tx2.clone();
                            let retry_task = tokio::spawn(async move {
                                if let Err(e) = retry_provider.generate_stream(&rm, &rmsgs, &rimgs, &rgp, tx2c.clone()).await {
                                    let _ = tx2c.send(StreamChunk::Error(format!("Retry stream failed: {}", e))).await;
                                }
                            });
                            // Re-point the single-flight lock at the retry task — the
                            // original task has already finished (it's what produced
                            // the empty response being retried), so Stop must abort
                            // THIS task now. Without this, cancel_generation aborts an
                            // already-dead handle (a no-op) and the retry keeps running
                            // ungoverned after the user believed they'd cancelled.
                            active_gens_cleanup.lock().await.insert(conv_id_cleanup.clone(), crate::GenerationHandle {
                                abort: retry_task.abort_handle(),
                                partial: Some(partial.clone()),
                                assistant_message_id: assist_id.clone(),
                            });
                            rx = rx2;
                            continue;
                        }
                        // Couldn't even rebuild the provider — fall through
                        // and treat this as the final (failed) result.
                    }

                    // Save the complete response to the database
                    if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                        error!("Failed to save response: {}", e);
                    }
                    let reasoning_final = reasoning_acc.lock().map(|r| r.clone()).unwrap_or_default();
                    if !reasoning_final.is_empty() {
                        if let Err(e) = MessageRepo::set_reasoning(&db_for_save, &assist_id, &reasoning_final).await {
                            warn!("Failed to save reasoning trace: {}", e);
                        }
                    }

                    // ── Multi-character response parsing ──
                    let mut multi_char_handled = false;
                    // Parsing is now always attempted (not gated on the conversation
                    // already having 2+ registered characters) — a solo conversation
                    // can legitimately introduce a brand-new speaker's [Name]: marker
                    // (see the "Other Characters Present" prompt addition above). The
                    // `is_ordinary_turn` check below keeps the common case (no marker
                    // at all, entire reply is the primary character) a true no-op —
                    // zero extra DB writes, same as before this change.
                    let fallback = stream_mc_names.first()
                        .cloned()
                        .unwrap_or_else(|| "Character".to_string());

                    let segments = parse_multi_character_response(
                        &full_text, &stream_mc_names, &fallback,
                    );
                    let is_ordinary_turn = segments.len() == 1 && segments[0].character_name == fallback;

                    if !is_ordinary_turn {
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
                            // Mutable copy — a brand-new speaker registered mid-loop gets
                            // pushed in immediately, so a second appearance of the same
                            // new name later in this SAME response resolves against her
                            // just-created row instead of registering a duplicate.
                            let mut resolution_pairs = stream_mc_pairs.clone();
                            for segment in &segments {
                                // Resolve character ID — synchronously register a brand-new
                                // speaker the LLM voiced but who isn't in the cast yet (e.g. a
                                // solo conversation's [Lena]: line); only fall back to the
                                // primary character if that registration itself fails.
                                let (char_id, full_name) = if let Some(cid) = resolve_character_id(
                                    &segment.character_name, &resolution_pairs,
                                ) {
                                    let name = resolution_pairs.iter()
                                        .find(|(_, id)| *id == cid)
                                        .map(|(name, _)| name.clone())
                                        .unwrap_or_else(|| segment.character_name.clone());
                                    (cid, name)
                                } else if let Some((new_id, new_name)) = crate::context::npc::pipeline::register_transient_speaker(
                                    &db_for_save, &app, &conv_id, &segment.character_name,
                                ).await {
                                    resolution_pairs.push((new_name.clone(), new_id.clone()));
                                    (new_id, new_name)
                                } else {
                                    // Fallback: attribute to primary character
                                    warn!("[multi-char] Unrecognized character '{}', attributing to primary", segment.character_name);
                                    let fallback_id = resolution_pairs.first()
                                        .map(|(_, id)| id.clone())
                                        .unwrap_or_default();
                                    let fallback_name = resolution_pairs.first()
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
                                        let segment_id = created.id.id.to_raw();
                                        prev_parent = segment_id.clone();
                                        // The combined parent message (embedded below only
                                        // when multi_char_handled stays false) was deleted
                                        // above — these per-segment rows are the only
                                        // record of this turn's content now, so each needs
                                        // its own embed call or it never gets indexed at all.
                                        spawn_embed_message(
                                            db_for_save.clone(), app.clone(),
                                            segment_id, conv_id.clone(), segment.content.clone(),
                                            Some(char_id.clone()),
                                        );
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
                            // This also covers the entire reply being voiced by one
                            // brand-new speaker (e.g. only [Lena]: — nothing from the
                            // primary at all): without the synchronous registration
                            // fallback here too, resolve_character_id would fail and
                            // this branch would silently do nothing, leaving the raw
                            // unstripped marker sitting in the primary's own bubble.
                            let seg = &segments[0];
                            let resolved = match resolve_character_id(&seg.character_name, &stream_mc_pairs) {
                                Some(cid) => Some((cid, seg.character_name.clone())),
                                None => crate::context::npc::pipeline::register_transient_speaker(
                                    &db_for_save, &app, &conv_id, &seg.character_name,
                                ).await,
                            };
                            if let Some((char_id, char_name)) = resolved {
                                info!("[multi-char] Single segment by {}, updating in-place", char_name);
                                if let Err(e) = db_for_save.query(
                                    "UPDATE type::thing('messages', $id) SET content = $content, character_id = type::thing('characters', $char_id), character_name = $char_name"
                                )
                                    .bind(("id", assist_id.clone()))
                                    .bind(("content", seg.content.clone()))
                                    .bind(("char_id", char_id.clone()))
                                    .bind(("char_name", char_name))
                                    .await
                                {
                                    warn!("[multi-char] Failed to apply single-segment attribution to message {}: {}", assist_id, e);
                                }

                                // The row now holds the marker-stripped seg.content, not
                                // the raw full_text the trailing embed call below would
                                // otherwise use (and that call is skipped entirely once
                                // multi_char_handled is true) — embed the actual saved
                                // text here instead.
                                spawn_embed_message(
                                    db_for_save.clone(), app.clone(),
                                    assist_id.clone(), conv_id.clone(), seg.content.clone(),
                                    Some(char_id),
                                );

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

                    // Background: embed assistant message for vector RAG. Only when the
                    // multi-char branches above didn't already handle it themselves —
                    // the segments.len() > 1 branch deletes `assist_id` outright (its
                    // replacement segment rows embed themselves individually above), and
                    // the segments.len() == 1 non-primary-speaker branch already embedded
                    // the marker-stripped content it actually saved. Embedding here too
                    // would either target a message_id that no longer exists (an orphan
                    // `message_embeddings` row that inflates the index's "embedded" count
                    // without indexing anything real) or duplicate an embedding that was
                    // just created against the correct content.
                    if !multi_char_handled {
                        spawn_embed_message(
                            db_for_save.clone(), app.clone(),
                            assist_id.clone(), conv_id.clone(), full_text.clone(),
                            stream_char_id.clone(),
                        );
                    }

                    // Background: extract and update scene state from the AI response.
                    // (spawn_scene_extraction also triggers NPC detection internally.)
                    spawn_scene_extraction(
                        db_for_save.clone(), app.clone(), conv_id.clone(), assist_id.clone(), full_text.clone(),
                    );

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

                    active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
                    break;
                }
                StreamChunk::Error(err) => {
                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "error".to_string(),
                        content: err,
                        message_id: assist_id.clone(),
                    });
                    active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
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

/// Best-effort MIME guess from a file's extension — mirrors the frontend's
/// `MIME_BY_EXT` map in `src/lib/utils/blobUrl.ts`. No `mime_guess` crate
/// dependency exists in this project; this small match is cheaper than
/// adding one for four known extensions.
fn mime_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Writes attachment bytes into `app_data_dir/attachments/{uuid}.{ext}` and
/// returns the resulting `MessageAttachment` — the shared tail end of both
/// `upload_message_attachment` (source is a file on disk) and
/// `upload_message_attachment_bytes` (source is raw clipboard-paste bytes,
/// which have no file/extension to read from).
async fn write_attachment(
    app: &tauri::AppHandle,
    bytes: &[u8],
    ext: &str,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    let mime_type = mime_type_for_extension(ext).ok_or_else(|| {
        MythicError::Validation(format!(
            "Unsupported attachment type '.{}'. Supported: png, jpg, jpeg, webp, gif", ext
        ))
    })?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let attachments_dir = app_data_dir.join("attachments");
    tokio::fs::create_dir_all(&attachments_dir).await?;
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::write(attachments_dir.join(&filename), bytes).await?;

    Ok(crate::models::conversation::MessageAttachment {
        relative_path: format!("attachments/{}", filename),
        mime_type: mime_type.to_string(),
    })
}

/// Copies a user-picked image file (an absolute path from the frontend's
/// file dialog) into `app_data_dir/attachments/`, so it can be attached to
/// a chat message and later resolved via `crate::error::resolve_within`.
///
/// Unlike `upload_character_avatar`, this preserves the source file's real
/// extension instead of hardcoding `.png` — the extension is how
/// `mime_type_for_extension` (and, on replay, `load_message_images`) knows
/// what MIME type to hand the provider.
#[tauri::command]
#[specta::specta]
pub async fn upload_message_attachment(
    app: tauri::AppHandle,
    file_path: String,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    let source = std::path::PathBuf::from(&file_path);
    if !source.exists() {
        return Err(MythicError::NotFound(format!("File not found: {}", file_path)));
    }
    let ext = source.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let bytes = tokio::fs::read(&source).await?;
    write_attachment(&app, &bytes, &ext).await
}

/// Same as `upload_message_attachment`, but for an image pasted directly
/// from the clipboard (e.g. a screenshot) — there's no source file on disk
/// to read, just raw bytes and the clipboard blob's MIME type from the
/// frontend's `paste` event.
#[tauri::command]
#[specta::specta]
pub async fn upload_message_attachment_bytes(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    extension: String,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    if bytes.is_empty() {
        return Err(MythicError::Validation("Pasted image is empty".to_string()));
    }
    write_attachment(&app, &bytes, &extension.to_lowercase()).await
}

/// Resolves a user message's stored attachments (from its `metadata` JSON,
/// see `MessageAttachment`) into raw `(bytes, mime_type)` pairs ready to
/// hand to `RigProvider::generate`/`generate_stream`. Used both for a
/// freshly-sent message (`send_message`) and for replaying an already-
/// stored message's attachments on regenerate/retry (`retry_failed_message`).
///
/// Silently skips any entry that fails to resolve or read — an attachment
/// whose file got cleaned up shouldn't break generation, it should just be
/// dropped from what the model sees.
async fn load_message_images(
    app_data_dir: &std::path::Path,
    metadata: Option<&serde_json::Value>,
) -> Vec<(Vec<u8>, String)> {
    let Some(attachments) = metadata
        .and_then(|m| m.get("attachments"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut images = Vec::new();
    for entry in attachments {
        let (Some(relative_path), Some(mime_type)) = (
            entry.get("relativePath").and_then(|v| v.as_str()),
            entry.get("mimeType").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        match crate::error::resolve_within(app_data_dir, relative_path) {
            Ok(resolved) => match tokio::fs::read(&resolved).await {
                Ok(bytes) => images.push((bytes, mime_type.to_string())),
                Err(e) => warn!("[load_message_images] Failed to read attachment {}: {}", relative_path, e),
            },
            Err(e) => warn!("[load_message_images] Failed to resolve attachment {}: {}", relative_path, e),
        }
    }
    images
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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);
        let provider = create_rig_provider(&provider_config)?;
        let stream_messages = messages.clone();
        let stream_images = images.clone();
        let stream_char_id = conv_character_id.clone();
        let stream_mc_names = multi_char_names.clone();
        let stream_mc_pairs = multi_char_pairs.clone();
        let stream_user_msg_id = user_message_id.clone();

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

        let db_for_save = db.clone();
        let conv_id = conversation_id.clone();
        let assist_id = assistant_msg_id.clone();
        let active_gens_cleanup = active_gens.clone();
        let conv_id_cleanup = conversation_id.clone();

        tokio::spawn(async move {
            let mut attempted_retry = false;
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Delta(text) => {
                        if let Ok(mut p) = partial.lock() { p.push_str(&text); }
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "delta".to_string(),
                            content: text,
                            message_id: assist_id.clone(),
                        });
                    }
                    StreamChunk::ReasoningDelta(text) => {
                        if let Ok(mut r) = reasoning_acc.lock() { r.push_str(&text); }
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "reasoning".to_string(),
                            content: text,
                            message_id: assist_id.clone(),
                        });
                    }
                    StreamChunk::Done(full_text) => {
                        if full_text.trim().is_empty() && !attempted_retry {
                            attempted_retry = true;
                            warn!(
                                "[retry_failed_message] Empty response for conversation {} — retrying once (likely a transient upstream overload)",
                                conv_id
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                            if let Ok(retry_provider) = create_rig_provider(&retry_provider_config) {
                                let (tx2, rx2) = tokio::sync::mpsc::channel::<StreamChunk>(64);
                                let rm = retry_model_id.clone();
                                let rmsgs = retry_messages.clone();
                                let rgp = retry_gen_params.clone();
                                let rimgs = retry_images.clone();
                                let tx2c = tx2.clone();
                                let retry_task = tokio::spawn(async move {
                                    if let Err(e) = retry_provider.generate_stream(&rm, &rmsgs, &rimgs, &rgp, tx2c.clone()).await {
                                        let _ = tx2c.send(StreamChunk::Error(format!("Retry stream failed: {}", e))).await;
                                    }
                                });
                                // See the matching comment in `send_message` — Stop
                                // must abort this retry task now, not the original
                                // (already-finished) one still on record.
                                active_gens_cleanup.lock().await.insert(conv_id_cleanup.clone(), crate::GenerationHandle {
                                    abort: retry_task.abort_handle(),
                                    partial: Some(partial.clone()),
                                    assistant_message_id: assist_id.clone(),
                                });
                                rx = rx2;
                                continue;
                            }
                        }

                        if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                            error!("Failed to save retry response: {}", e);
                        }
                        let reasoning_final = reasoning_acc.lock().map(|r| r.clone()).unwrap_or_default();
                        if !reasoning_final.is_empty() {
                            if let Err(e) = MessageRepo::set_reasoning(&db_for_save, &assist_id, &reasoning_final).await {
                                warn!("Failed to save reasoning trace: {}", e);
                            }
                        }

                        // ── Multi-character response parsing ──
                        // A retried reply is a real AI turn like any other —
                        // without this, a retried [Lena]: line was saved
                        // verbatim under the primary character with no name
                        // badge. See the matching block in `send_message`
                        // for the full rationale; kept in lockstep with it.
                        let mut multi_char_handled = false;
                        let fallback = stream_mc_names.first()
                            .cloned()
                            .unwrap_or_else(|| "Character".to_string());
                        let segments = parse_multi_character_response(
                            &full_text, &stream_mc_names, &fallback,
                        );
                        let is_ordinary_turn = segments.len() == 1 && segments[0].character_name == fallback;

                        if !is_ordinary_turn {
                            if segments.len() > 1 {
                                info!("[multi-char] Parsed {} character segments (retry)", segments.len());

                                if let Err(e) = MessageRepo::delete(&db_for_save, &assist_id).await {
                                    warn!("[multi-char] Failed to delete combined parent message {}: {}", assist_id, e);
                                }

                                let mut prev_parent = stream_user_msg_id.clone();
                                let mut resolution_pairs = stream_mc_pairs.clone();
                                for segment in &segments {
                                    let (char_id, full_name) = if let Some(cid) = resolve_character_id(
                                        &segment.character_name, &resolution_pairs,
                                    ) {
                                        let name = resolution_pairs.iter()
                                            .find(|(_, id)| *id == cid)
                                            .map(|(name, _)| name.clone())
                                            .unwrap_or_else(|| segment.character_name.clone());
                                        (cid, name)
                                    } else if let Some((new_id, new_name)) = crate::context::npc::pipeline::register_transient_speaker(
                                        &db_for_save, &app, &conv_id, &segment.character_name,
                                    ).await {
                                        resolution_pairs.push((new_name.clone(), new_id.clone()));
                                        (new_id, new_name)
                                    } else {
                                        warn!("[multi-char] Unrecognized character '{}', attributing to primary (retry)", segment.character_name);
                                        let fallback_id = resolution_pairs.first()
                                            .map(|(_, id)| id.clone())
                                            .unwrap_or_default();
                                        let fallback_name = resolution_pairs.first()
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
                                            let segment_id = created.id.id.to_raw();
                                            prev_parent = segment_id.clone();
                                            // See the matching comment in `send_message` —
                                            // the deleted parent never gets embedded, so
                                            // each replacement segment needs its own call.
                                            spawn_embed_message(
                                                db_for_save.clone(), app.clone(),
                                                segment_id, conv_id.clone(), segment.content.clone(),
                                                Some(char_id.clone()),
                                            );
                                        }
                                        Err(e) => {
                                            warn!("[multi-char] Failed to create segment for {}: {}", full_name, e);
                                        }
                                    }
                                }

                                multi_char_handled = true;

                                let _ = app.emit("multi-char-response", serde_json::json!({
                                    "conversation_id": conv_id,
                                    "segments": segments,
                                    "parent_message_id": assist_id,
                                }));
                            } else if segments.len() == 1 {
                                let seg = &segments[0];
                                let resolved = match resolve_character_id(&seg.character_name, &stream_mc_pairs) {
                                    Some(cid) => Some((cid, seg.character_name.clone())),
                                    None => crate::context::npc::pipeline::register_transient_speaker(
                                        &db_for_save, &app, &conv_id, &seg.character_name,
                                    ).await,
                                };
                                if let Some((char_id, char_name)) = resolved {
                                    info!("[multi-char] Single segment by {}, updating in-place (retry)", char_name);
                                    if let Err(e) = db_for_save.query(
                                        "UPDATE type::thing('messages', $id) SET content = $content, character_id = type::thing('characters', $char_id), character_name = $char_name"
                                    )
                                        .bind(("id", assist_id.clone()))
                                        .bind(("content", seg.content.clone()))
                                        .bind(("char_id", char_id.clone()))
                                        .bind(("char_name", char_name))
                                        .await
                                    {
                                        warn!("[multi-char] Failed to apply single-segment attribution to message {} (retry): {}", assist_id, e);
                                    }

                                    // See the matching comment in `send_message` — embed
                                    // the marker-stripped content actually saved, not the
                                    // raw full_text the trailing call below would use.
                                    spawn_embed_message(
                                        db_for_save.clone(), app.clone(),
                                        assist_id.clone(), conv_id.clone(), seg.content.clone(),
                                        Some(char_id),
                                    );

                                    multi_char_handled = true;

                                    let _ = app.emit("multi-char-response", serde_json::json!({
                                        "conversation_id": conv_id,
                                        "segments": segments,
                                        "parent_message_id": assist_id,
                                    }));
                                }
                            }
                        }

                        // Background: embed assistant message for vector RAG. See the
                        // matching comment in `send_message` for why this is skipped
                        // once a multi-char branch above already embedded the real
                        // content itself (or the parent row no longer exists).
                        if !multi_char_handled {
                            spawn_embed_message(
                                db_for_save.clone(), app.clone(),
                                assist_id.clone(), conv_id.clone(), full_text.clone(),
                                stream_char_id.clone(),
                            );
                        }

                        // Background: extract and update scene state from the
                        // retried response (also triggers NPC detection
                        // internally) — a retried message is a real AI turn
                        // like any other and must update the scene bar too.
                        spawn_scene_extraction(
                            db_for_save.clone(), app.clone(), conv_id.clone(), assist_id.clone(),
                            full_text.clone(),
                        );

                        // When multi-char segments were processed, emit done
                        // with empty content — the multi-char-response event
                        // already handled rendering (see the matching comment
                        // in `send_message`).
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
                        info!("Retry response completed for conversation {}", conv_id);
                        active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
                        break;
                    }
                    StreamChunk::Error(err) => {
                        let _ = app.emit("chat-stream", StreamEvent {
                            event_type: "error".to_string(),
                            content: err,
                            message_id: assist_id.clone(),
                        });
                        active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
                        break;
                    }
                }
            }
        });
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
    #[specta(type = u32)]
    pub total_budget: usize,
    /// Tokens used by fixed layers (system, character, lorebook, memories, emotion, PHI).
    #[specta(type = u32)]
    pub fixed_tokens: usize,
    /// Tokens used by conversation history (after sliding window).
    #[specta(type = u32)]
    pub history_tokens: usize,
    /// Tokens used by the rolling summary (0 if no summary yet).
    #[specta(type = u32)]
    pub summary_tokens: usize,
    /// Total messages in the full conversation branch.
    #[specta(type = u32)]
    pub total_messages: usize,
    /// Messages included in the sliding window.
    #[specta(type = u32)]
    pub included_messages: usize,
    /// Messages evicted (not sent to the LLM).
    #[specta(type = u32)]
    pub evicted_messages: usize,
    /// Tokens used by RAG-retrieved context (0 if RAG disabled or no results).
    #[specta(type = u32)]
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

/// Whether `keyword` appears in `corpus` on a word boundary — i.e. not as a
/// substring buried inside a longer word. Both arguments are expected
/// already lowercased by the caller (this does no case handling itself).
/// A plain `corpus.contains(keyword)` used to let a short lorebook keyword
/// like "cat" fire on "catastrophe", triggering unrelated lore constantly.
/// Multi-word keys (e.g. "new york") are supported — the whole phrase must
/// be bounded by non-alphanumeric characters (or start/end of string) on
/// both sides, not each word individually.
fn keyword_matches_at_word_boundary(corpus: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }
    for (start, matched) in corpus.match_indices(keyword) {
        let end = start + matched.len();
        let before_ok = corpus[..start].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        let after_ok = corpus[end..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Case-insensitive `{{user}}` -> display-name substitution, applied to
/// character-card text fields. Uses the active persona's name when the
/// conversation has one selected; otherwise falls back to the generic
/// `"User"` token so `{{user}}`-authored cards still read naturally without
/// requiring anyone to set up a persona. A no-op (returns the input
/// unchanged) only when the text contains no `{{user}}` marker at all.
fn substitute_user_macro(text: &str, persona_name: Option<&str>) -> String {
    if !text.to_lowercase().contains("{{user}}") {
        return text.to_string();
    }
    let name = persona_name.unwrap_or("User");
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lower = rest.to_lowercase();
        match lower.find("{{user}}") {
            Some(idx) => {
                result.push_str(&rest[..idx]);
                result.push_str(name);
                rest = &rest[idx + "{{user}}".len()..];
            }
            None => {
                result.push_str(rest);
                break;
            }
        }
    }
    result
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

    // The conversation's active persona (the user's own stand-in), if any —
    // drives {{user}} macro substitution in character-card text below
    // (falls back to the generic "User" token when no persona is set) plus
    // an "About the User" context block (only shown when a persona IS set —
    // there's nothing persona-specific to say otherwise).
    let persona = match conv.as_ref().and_then(|c| c.persona_id.as_ref()) {
        Some(pid) => PersonaRepo::get(db, &pid.id.to_raw()).await.ok(),
        None => None,
    };
    let persona_name = persona.as_ref().map(|p| p.name.as_str());

    // ── Check for multi-character mode ──
    // "Genuine" multi-character mode requires at least one OTHER cast member
    // (not the conversation's own primary character) who is either manually
    // added (role: 'secondary' — always a real Gallery character, since the
    // Group Cast add-picker only lists Gallery characters) or an
    // auto-detected NPC the user has actually promoted to their Gallery
    // (origin: 'gallery'). A merely-transient or confirmed-but-unpromoted
    // NPC does NOT flip this — the conversation stays in solo mode (with its
    // own "may voice others" permission, see the Roleplay Directive below).
    // The heavier Group Scene Directive only kicks in once the user has
    // actually decided this is a deliberate multi-character story, not just
    // "someone spoke once" or "the story flagged them as significant."
    let conv_chars = ConversationCharacterRepo::list(db, conversation_id).await.unwrap_or_default();
    let active_conv_chars: Vec<_> = conv_chars.iter().filter(|c| c.is_active).collect();
    let mut is_multi_char = false;
    for c in &active_conv_chars {
        let char_id_raw = c.character_id.id.to_raw();
        if character_id.as_deref() == Some(char_id_raw.as_str()) {
            continue; // the primary herself never counts toward "genuine multi-char"
        }
        if c.role == "primary" || c.role == "secondary" {
            is_multi_char = true;
            break;
        }
        // role is 'npc' or 'transient' — only counts once actually promoted
        if let Ok(character) = CharacterRepo::get(db, &char_id_raw).await {
            if character.origin == "gallery" {
                is_multi_char = true;
                break;
            }
        }
    }

    if is_multi_char {
        // ── Multi-character mode: inject all character cards ──
        info!("[build_prompt] Multi-char mode: {} active characters", active_conv_chars.len());

        // The primary character may have no conversation_characters row at
        // all — that table is only ever populated by a manual Group Cast
        // add, or by this app's own auto-detected-NPC pipeline; a plain
        // solo conversation that later gains a promoted NPC never gets the
        // primary migrated in on its own. Without this, "genuine multi-char"
        // mode (see above) could fire with a prompt that describes every
        // *other* cast member but never the primary character herself.
        let primary_has_row = active_conv_chars.iter()
            .any(|c| Some(c.character_id.id.to_raw().as_str()) == character_id.as_deref());
        if !primary_has_row {
            if let Some(ref char_id) = character_id {
                if let Ok(character) = CharacterRepo::get(db, char_id).await {
                    let card = &character.data;
                    let mut parts = Vec::new();
                    parts.push(format!("[Primary Character — {}]", character.name));
                    if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                        let sys = substitute_user_macro(sys, persona_name);
                        if !sys.is_empty() { parts.push(sys); }
                    }
                    if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                        let desc = substitute_user_macro(desc, persona_name);
                        if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                    }
                    if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                        let personality = substitute_user_macro(personality, persona_name);
                        if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                    }
                    if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                        let scenario = substitute_user_macro(scenario, persona_name);
                        if !scenario.is_empty() { parts.push(format!("Scenario: {}", scenario)); }
                    }
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: parts.join("\n"),
                    });
                }
            }
        }

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
                            let sys = substitute_user_macro(sys, persona_name);
                            if !sys.is_empty() { parts.push(sys); }
                        }
                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            let personality = substitute_user_macro(personality, persona_name);
                            if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                        }
                        if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                            let scenario = substitute_user_macro(scenario, persona_name);
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
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            let personality = substitute_user_macro(personality, persona_name);
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
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(desc); }
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
        let mut char_names: Vec<String> = active_conv_chars.iter()
            .map(|c| c.character_name.clone())
            .collect();
        if !primary_has_row {
            if let Some(ref char_id) = character_id {
                if let Ok(character) = CharacterRepo::get(db, char_id).await {
                    char_names.insert(0, character.name);
                }
            }
        }
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
             - Normally only respond as characters listed above, never as {{{{user}}}}. The one exception: if the \
             current scene lists another character present who ISN'T in the list above, and they're directly \
             addressed, commanded, or the moment truly calls for their own reaction, you may voice them too using \
             the same [FullCharacterName]: format — never invent a name that isn't listed as present in the scene\n\
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

            // Who else is ALREADY registered in this conversation's cast
            // (excluding the primary herself) — the authoritative source for
            // "who may be voiced," used INSTEAD of the scene-state
            // extractor's "Present" list. That list is populated by a
            // separate, best-effort background LLM call that runs only
            // after each response and has repeatedly failed to pick up a
            // just-introduced/just-addressed character for several turns in
            // a row (observed directly: a registered cast member absent from
            // "Present" for 3+ consecutive extraction passes) — which, under
            // the old "never invent a name not listed as present" wording,
            // silently blocked her from ever being voiced even once
            // everyone (the user included) was already treating her as
            // present. A name already in the cast table doesn't need the
            // scene extractor's permission to exist.
            let known_others: Vec<String> = active_conv_chars.iter()
                .filter(|c| character_id.as_deref() != Some(c.character_id.id.to_raw().as_str()))
                .map(|c| c.character_name.clone())
                .collect();
            let known_others_clause = if known_others.is_empty() {
                String::new()
            } else {
                format!(" Already part of this story's cast: {}.", known_others.join(", "))
            };

            // Baseline roleplay behavior contract — established first so it
            // holds regardless of whether the character card defines its own
            // system_prompt. Without this, some models (observed with
            // Nemotron) default to narrating/analyzing the scene from an
            // outside perspective ("The user is greeting Aria...") instead
            // of actually responding in character from turn one.
            system_parts.push(format!(
                "[Roleplay Directive]\n\
                 You are {name}. Fully embody this character and respond only as {name} — never describe, \
                 narrate, or speak for the user, and never write from an outside narrator's perspective. \
                 Write immersive narrative prose: actions and body language in *asterisks*, spoken dialogue in \"quotes\". \
                 Stay grounded in {name}'s own personality, voice, knowledge, and worldview at all times. \
                 Never break character, add out-of-character commentary, disclaimers, or meta-analysis — you ARE {name} \
                 living this scene, not an assistant describing it. \
                 Do not summarize, use bullet points, or format like an AI assistant. Drive the scene forward naturally: \
                 react, initiate, and let the story progress rather than passively restating what the user just said.\n\n\
                 [Other Characters Present] {name} is not the only person in this scene.{known_others_clause} RULE: if \
                 another character — someone already established in this story, or someone the current scene clearly \
                 introduces — is directly addressed by name, spoken to, commanded, or asked a direct question, you MUST \
                 voice their response in this SAME reply — never stop and wait for {{{{user}}}}'s next message before \
                 they answer. Prefix their section with their full name in brackets, exactly like this:\n\n\
                 [FullCharacterName]: *their action* \"their dialogue\"\n\n\
                 For anyone present who ISN'T directly addressed or relevant to this exact moment, stay silent about \
                 them — {name}'s own voice is still the default and should make up most of your response. Never \
                 invent a name with no basis anywhere in this story; never use this to speak for {{{{user}}}}; if \
                 truly no one else was addressed, write only as {name} and use no bracket markers at all.",
                name = character.name,
                known_others_clause = known_others_clause,
            ));

            // Character system prompt
            if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                let sys = substitute_user_macro(sys, persona_name);
                if !sys.is_empty() {
                    system_parts.push(sys);
                }
            }

            // Character description
            if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                let desc = substitute_user_macro(desc, persona_name);
                if !desc.is_empty() {
                    system_parts.push(format!("Character Description:\n{}", desc));
                }
            }

            // Personality
            if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                let personality = substitute_user_macro(personality, persona_name);
                if !personality.is_empty() {
                    system_parts.push(format!("Personality:\n{}", personality));
                }
            }

            // Scenario
            if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                let scenario = substitute_user_macro(scenario, persona_name);
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

    // ── About the User ──
    // Injected once, right after the character card(s) — tells the model
    // who it's actually roleplaying with when the user has an active
    // persona selected. A complete no-op when no persona is selected.
    if let Some(ref p) = persona {
        let desc = p.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let personality = p.data.get("personality").and_then(|v| v.as_str()).unwrap_or("");
        let mut about = format!("[About the User]\nYou are speaking with {}.", p.name);
        if !desc.is_empty() {
            about.push_str(&format!(" {}", desc));
        }
        if !personality.is_empty() {
            about.push_str(&format!(" {}", personality));
        }
        prompt.push(ChatMessage {
            role: MessageRole::System,
            content: about,
        });
    }

    // Add lorebook entries — fetch entries for every active character in
    // this conversation (not just the primary), then filter in Rust. A
    // conversation with e.g. Aria as primary and Lena as a cast member
    // previously only ever considered Aria's lorebook — Lena's entries were
    // silently never eligible for injection no matter how well-triggered
    // their keywords were.
    if let Some(ref char_id) = character_id {
        let mut lorebook_char_ids: Vec<String> = vec![char_id.clone()];
        for c in &active_conv_chars {
            let id = c.character_id.id.to_raw();
            if !lorebook_char_ids.contains(&id) {
                lorebook_char_ids.push(id);
            }
        }
        let mut all_entries: Vec<crate::models::lorebook::LorebookEntry> = Vec::new();
        {
            let mut seen_ids = std::collections::HashSet::new();
            for cid in &lorebook_char_ids {
                if let Ok(entries) = LorebookRepo::list(db, cid).await {
                    for entry in entries {
                        // A global entry (character_id IS NONE) is returned
                        // for every character queried — dedupe by id so it
                        // isn't injected once per cast member.
                        let entry_id = entry.id.id.to_raw();
                        if seen_ids.insert(entry_id) {
                            all_entries.push(entry);
                        }
                    }
                }
            }
            // Each per-character fetch is individually ORDER BY priority
            // DESC, insertion_order ASC, but merging multiple characters'
            // results loses that global ordering — re-sort so the
            // token-budget cutoff below still fills in true priority order
            // across the whole cast, not "all of character A's entries,
            // then all of character B's" regardless of relative priority.
            all_entries.sort_by(|a, b| {
                b.priority.cmp(&a.priority).then(a.insertion_order.cmp(&b.insertion_order))
            });
        }
        {
            // Collected first, THEN pushed to `prompt` subject to
            // max_lorebook_tokens below — previously every matching entry
            // was pushed unconditionally, with no cap at all (unlike the
            // memories layer just below, which has always enforced
            // context_budget.max_memory_tokens), so enough always-active/
            // triggered entries could blow the whole context budget.
            // `all_entries` is sorted by priority DESC, insertion_order ASC
            // above, so filling in that order and stopping once the budget's
            // spent keeps the most important entries.
            let mut lorebook_messages: Vec<ChatMessage> = Vec::new();

            // Always-active entries
            for entry in all_entries.iter() {
                if entry.enabled && entry.always_active {
                    lorebook_messages.push(ChatMessage {
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
                    // keys is already Vec<String> — no JSON parsing needed.
                    // Word-boundary match, not a raw substring check — the
                    // latter let a short keyword like "cat" fire on
                    // "catastrophe", triggering unrelated lore constantly.
                    let triggered = entry.keys
                        .iter()
                        .map(|k| k.to_lowercase())
                        .filter(|k| !k.is_empty())
                        .any(|keyword| keyword_matches_at_word_boundary(&corpus, &keyword));

                    if triggered {
                        lorebook_messages.push(ChatMessage {
                            role: MessageRole::System,
                            content: entry.content.clone(),
                        });
                    }
                }
            }

            let mut lorebook_tokens_used = 0usize;
            for msg in lorebook_messages {
                let tokens = count_message_tokens(&msg);
                if lorebook_tokens_used + tokens > context_budget.max_lorebook_tokens {
                    break;
                }
                lorebook_tokens_used += tokens;
                prompt.push(msg);
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
                                // memory_scope = "conversation" means the user asked for this
                                // story's memories to stay isolated — pass the conversation_id
                                // through so retrieval respects that instead of always
                                // searching this character's memories across every
                                // conversation they've ever appeared in.
                                let memory_scope_conv_id = if memory_scope == "conversation" {
                                    Some(conversation_id)
                                } else {
                                    None
                                };
                                if let Ok(results) = query_relevant_memories(
                                    db, &provider, embed_model,
                                    char_id, &last_user_content,
                                    10,   // top 10 relevant memories
                                    0.4,  // lower threshold — facts are short
                                    memory_scope_conv_id,
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
                            MemoryRepo::list_with_canon(db, conversation_id, char_id).await.unwrap_or_default()
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
                // Scene extraction is told to use the literal "{{user}}"
                // token for the player character (see scene_extractor.rs) —
                // substitute it the same way character-card text is above,
                // so the model never sees a raw unresolved macro here either.
                let chars_list = if scene.characters_present.is_empty() {
                    "unspecified".to_string()
                } else {
                    scene.characters_present.iter()
                        .map(|c| substitute_user_macro(c, persona_name))
                        .collect::<Vec<_>>()
                        .join(", ")
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
pub(crate) async fn resolve_model_id(
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
        ProviderAdapter::AiHorde => return Err(MythicError::Config(
            "AI Horde is an image provider, not an LLM provider".to_string()
        )),
        ProviderAdapter::WanGp => return Err(MythicError::Config(
            "WanGP is an image/video provider, not an LLM provider".to_string()
        )),
    };

    let api_key = config.config.get("api_key").and_then(|v| v.as_str());
    let base_url = config.config.get("base_url").and_then(|v| v.as_str());

    RigProvider::from_config(adapter_str, api_key, base_url)
}

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
async fn resolve_embedding_provider(db: &Surreal<Db>) -> Result<(RigProvider, String), MythicError> {
    let enabled = ProviderRepo::list_enabled_models(db, None).await?;
    let embedding_entry = enabled.iter()
        .find(|m| m.model_type == "embedding")
        .ok_or_else(|| MythicError::Config(
            "No embedding model enabled. Go to AI Studio → Embedding Models and enable one.".to_string()
        ))?;
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
            debug!("[scene_flow] No LLM provider configured, skipping extraction (non-fatal): {}", e);
        }
        if let Ok(provider_config) = provider_config_result {
            let provider_result = create_rig_provider(&provider_config);
            if let Err(ref e) = provider_result {
                debug!("[scene_flow] Failed to build provider for extraction (non-fatal): {}", e);
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

                        match extract_scene_state(&provider, &model, &ai_response, current_json.as_deref()).await {
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
                                            let _ = app.emit("scene_state_changed",
                                                serde_json::to_value(&new_state).unwrap_or_default());
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
            &db, &app, &conversation_id, &message_id, &ai_response, forced,
        ).await {
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
    let result = provider.generate(&model_id, &messages, &[], &gen_params).await?;

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
