//! Chat command handler — orchestrates message sending, prompt building,
//! and streaming responses from LLM providers via Tauri events.

use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::context::budget::ContextBudget;
use crate::context::window::apply_sliding_window;

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::db::characters::CharacterRepo;
use crate::db::character_state::CharacterStateRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::lorebook::LorebookRepo;
use crate::db::memories::MemoryRepo;
use crate::db::messages::MessageRepo;
use crate::db::providers::ProviderRepo;
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
pub async fn send_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    content: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
) -> Result<serde_json::Value, MythicError> {
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

    // 2. Build the prompt
    debug!("[send_message] building prompt...");

    // Get the active LLM provider early — we need context_length for the budget
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
                tx,
            ).await {
                error!("Stream generation error: {}", e);
            }
        });

    // Forward stream chunks as Tauri events
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
                    // Save the complete response to the database
                    if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                        error!("Failed to save response: {}", e);
                    }

                    let _ = app.emit("chat-stream", StreamEvent {
                        event_type: "done".to_string(),
                        content: full_text,
                        message_id: assist_id.clone(),
                    });

                    info!("Chat response completed for conversation {}", conv_id);
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

    Ok(serde_json::json!({
        "user_message_id": user_msg_id,
        "assistant_message_id": assistant_msg_id,
    }))
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

        Ok(serde_json::json!({
            "user_message_id": user_msg_id,
            "assistant_message_id": assistant_msg_id,
        }))
    }
}

/// Retries a failed message by reusing the existing user message already in the DB.
/// Cleans up the empty/failed assistant placeholder from the previous attempt,
/// creates a fresh one, and re-triggers LLM generation. This avoids duplicating
/// the user message in both the UI and database.
#[tauri::command]
pub async fn retry_failed_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    user_message_id: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
) -> Result<serde_json::Value, MythicError> {
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
                &model_id, &stream_messages, &gen_params, tx,
            ).await {
                error!("Retry stream generation error: {}", e);
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

    Ok(serde_json::json!({
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_msg_id,
    }))
}

/// Regenerates the AI response for a given message by re-running generation
/// from the same parent point in the conversation tree.
#[tauri::command]
pub async fn regenerate_message(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
    model: Option<String>,
    system_prompt: Option<String>,
    streaming: Option<bool>,
    post_history_instructions: Option<String>,
) -> Result<serde_json::Value, MythicError> {
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

/// Statistics about the context window for observability.
/// Returned alongside the prompt so callers can log/surface token usage.
#[derive(Clone, Debug, serde::Serialize)]
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

    // Build system prompt from character data
    if let Some(ref char_id) = character_id {
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
            // Memories are auto-extracted facts from past conversations (events, relationships,
            // character reveals, etc.). Injecting them here gives the AI long-term recall.
            //
            // Placement: after lorebook, before emotional state — the "knowledge layer".
            // Limited to 20 most recent to avoid context overflow.
            if memory_scope != "none" {
                let memory_list = if memory_scope == "character" {
                    MemoryRepo::list(db, Some(char_id), None).await.unwrap_or_default()
                } else {
                    MemoryRepo::list(db, None, Some(conversation_id)).await.unwrap_or_default()
                };

                // Take only the 20 most recent (list is already ordered DESC)
                let memory_rows: Vec<_> = memory_list.into_iter().take(20).collect();

                if !memory_rows.is_empty() {
                    // Format memories as a bullet list, reversed to chronological order
                    let mut facts: Vec<String> = memory_rows.into_iter().map(|m| m.content).collect();
                    facts.reverse(); // oldest first for natural reading order

                    let memory_block = format!(
                        "[Remembered Facts — things you know from past interactions]\n{}",
                        facts.iter()
                            .map(|f| format!("• {}", f))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );

                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: memory_block,
                    });
                }
            }

            // Inject character emotional state as a dynamic context layer.
            // Placed last in the system prompt so it's closest to the conversation history
            // and carries the most weight in the attention window.
            if let Ok(Some(state)) = CharacterStateRepo::get(db, char_id, conversation_id).await {
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

            // PHI goes last — after history, maximum attention weight
            if let Some(phi) = phi_message {
                prompt.push(phi);
            }

            return Ok((prompt, ContextStats {
                total_budget: context_budget.max_context_tokens,
                fixed_tokens: allocation.fixed_layers_tokens,
                history_tokens,
                summary_tokens: 0,
                total_messages,
                included_messages,
                evicted_messages,
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
                let memory_list = MemoryRepo::list(db, None, Some(conversation_id)).await.unwrap_or_default();
                let memory_rows: Vec<_> = memory_list.into_iter().take(20).collect();
                if !memory_rows.is_empty() {
                    let mut facts: Vec<String> = memory_rows.into_iter().map(|m| m.content).collect();
                    facts.reverse();
                    let memory_block = format!(
                        "[Remembered Facts — things you know from past interactions]\n{}",
                        facts.iter().map(|f| format!("• {}", f)).collect::<Vec<_>>().join("\n")
                    );
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: memory_block,
                    });
                }
            }

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

            if let Some(phi) = phi_message {
                prompt.push(phi);
            }

            return Ok((prompt, ContextStats {
                total_budget: context_budget.max_context_tokens,
                fixed_tokens: allocation.fixed_layers_tokens,
                history_tokens,
                summary_tokens: 0,
                total_messages,
                included_messages,
                evicted_messages,
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
                let memory_block = format!(
                    "[Remembered Facts — things you know from past interactions]\n{}",
                    facts.iter().map(|f| format!("• {}", f)).collect::<Vec<_>>().join("\n")
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: memory_block,
                });
            }
        }

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

        if let Some(phi) = phi_message {
            prompt.push(phi);
        }

        Ok((prompt, ContextStats {
            total_budget: context_budget.max_context_tokens,
            fixed_tokens: allocation.fixed_layers_tokens,
            history_tokens,
            summary_tokens: 0,
            total_messages,
            included_messages,
            evicted_messages,
        }))
    }
}

/// Finds the default LLM provider configuration from the database.
async fn get_default_llm_provider(
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
                // Fall back to first enabled model for this provider
                let provider_id_str = provider_config.id.id.to_raw();
                let enabled = ProviderRepo::list_enabled_models(db, Some(&provider_id_str)).await?;
                match enabled.into_iter().next() {
                    Some(m) => Ok(m.model_id),
                    None => Err(MythicError::Config(
                        "No model selected. Go to AI Studio \u{2192} Models, enable at least one model.".to_string()
                    )),
                }
            }
        }
    }
}

/// Creates a unified rig-backed LLM provider from DB config.
fn create_rig_provider(config: &ProviderConfig) -> Result<RigProvider, MythicError> {
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

/// Stateless LLM generation — calls the configured provider without saving
/// anything to the database. Used by internal pipelines (memory extraction,
/// summarization) that need LLM inference without polluting conversations.
#[tauri::command]
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
