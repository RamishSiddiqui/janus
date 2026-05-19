//! Chat command handler — orchestrates message sending, prompt building,
//! and streaming responses from LLM providers via Tauri events.

use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::models::provider::{ModelInfo, ProviderAdapter, ProviderConfig, ProviderType};
use crate::providers::ollama::OllamaProvider;
use crate::providers::openai_client::{OpenAiClient, OpenAiClientConfig};
use crate::providers::openrouter::OpenRouterProvider;
use crate::providers::traits::{LlmProvider, StreamChunk};
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
    let http = state_guard.http_client.clone();
    drop(state_guard);

    // 1. Save the user message
    let user_msg_id = Uuid::new_v4().to_string();

    // Get current active message as parent for branching
    let parent_id: Option<String> = sqlx::query_scalar(
        "SELECT active_message_id FROM conversations WHERE id = ?"
    )
    .bind(&conversation_id)
    .fetch_optional(&db)
    .await?
    .flatten();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, parent_id)
         VALUES (?, ?, 'user', ?, ?)"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .bind(&content)
    .bind(&parent_id)
    .execute(&db)
    .await?;

    // Update active message pointer
    sqlx::query(
        "UPDATE conversations SET active_message_id = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .execute(&db)
    .await?;

    // 2. Build the prompt
    let messages = build_prompt(&db, &conversation_id, &user_msg_id, system_prompt.as_deref(), post_history_instructions.as_deref()).await?;

    // 3. Get the active LLM provider
    let provider_config = get_default_llm_provider(&db).await?;
    let model_id = match model {
        Some(m) if !m.is_empty() && m != "unknown" => m,
        _ => {
            let stored = provider_config.config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !stored.is_empty() && stored != "unknown" {
                stored.to_string()
            } else {
                // Fall back to first enabled model for this provider
                let first_enabled: Option<(String,)> = sqlx::query_as(
                    "SELECT model_id FROM enabled_models WHERE provider_id = ? AND enabled = 1 LIMIT 1"
                )
                .bind(&provider_config.id)
                .fetch_optional(&db)
                .await?;
                match first_enabled {
                    Some((m,)) => m,
                    None => return Err(MythicError::Config(
                        "No model selected. Go to AI Studio \u{2192} Models, enable at least one model.".to_string()
                    )),
                }
            }
        }
    };



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

    // 4. Create the assistant message placeholder
    let assistant_msg_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, parent_id)
         VALUES (?, ?, 'assistant', '', ?)"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id)
    .bind(&user_msg_id)
    .execute(&db)
    .await?;

    // Update active message pointer to the assistant response
    sqlx::query(
        "UPDATE conversations SET active_message_id = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id)
    .execute(&db)
    .await?;

    // 5. Stream or generate the response
    let use_streaming = streaming.unwrap_or(true);

    if use_streaming {
        // --- Streaming path ---
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let provider = create_llm_provider(&provider_config, http)?;

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
                    if let Err(e) = sqlx::query(
                        "UPDATE messages SET content = ? WHERE id = ?"
                    )
                    .bind(&full_text)
                    .bind(&assist_id)
                    .execute(&db_for_save)
                    .await {
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
        let provider = create_llm_provider(&provider_config, http)?;

        let db_for_save = db.clone();
        let conv_id = conversation_id.clone();
        let assist_id = assistant_msg_id.clone();

        match provider.generate(&model_id, &messages, &gen_params).await {
            Ok(full_text) => {
                // Save to database
                sqlx::query("UPDATE messages SET content = ? WHERE id = ?")
                    .bind(&full_text)
                    .bind(&assist_id)
                    .execute(&db_for_save)
                    .await?;

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
    let parent_id: Option<String> = sqlx::query_scalar(
        "SELECT parent_id FROM messages WHERE id = ?"
    )
    .bind(&message_id)
    .fetch_optional(&db)
    .await?
    .flatten();

    drop(state_guard);

    // Delete the old response
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&message_id)
        .execute(&db)
        .await?;

    // If the message had a parent (it was an assistant response to a user message),
    // use the parent as the last user message
    if let Some(ref pid) = parent_id {
        // Update active to the parent so send_message builds from there
        sqlx::query(
            "UPDATE conversations SET active_message_id = ?, updated_at = datetime('now') WHERE id = ?"
        )
        .bind(pid)
        .bind(&conversation_id)
        .execute(&db)
        .await?;
    }

    // Get the parent message content to re-send
    let parent_content: String = if let Some(ref pid) = parent_id {
        sqlx::query_scalar("SELECT content FROM messages WHERE id = ?")
            .bind(pid)
            .fetch_optional(&db)
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Re-trigger send_message (which will create a new assistant response)
    // For regeneration, we just need to stream a new response from the same history
    send_message(app, state, conversation_id, parent_content, model, system_prompt, streaming, post_history_instructions).await
}

// --- Internal helpers ---

/// Builds the full prompt by combining system prompt, character data,
/// and conversation history.
async fn build_prompt(
    db: &sqlx::Pool<sqlx::Sqlite>,
    conversation_id: &str,
    up_to_message_id: &str,
    user_system_prompt: Option<&str>,
    post_history_instructions: Option<&str>,
) -> Result<Vec<ChatMessage>, MythicError> {
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
    let conv_meta: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT character_id, memory_scope FROM conversations WHERE id = ?"
    )
    .bind(conversation_id)
    .fetch_optional(db)
    .await?;

    let (character_id, memory_scope) = match conv_meta {
        Some((char_id, scope)) => (char_id, scope),
        None => (None, "character".to_string()),
    };

    // Build system prompt from character data
    if let Some(ref char_id) = character_id {
        let char_data: Option<String> = sqlx::query_scalar(
            "SELECT data FROM characters WHERE id = ?"
        )
        .bind(char_id)
        .fetch_optional(db)
        .await?
        .flatten();

        if let Some(data_json) = char_data {
            if let Ok(card) = serde_json::from_str::<serde_json::Value>(&data_json) {
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
    }

    // Add lorebook entries that are always active
    if let Some(ref char_id) = character_id {
        let lorebook_entries: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM lorebook_entries
             WHERE (character_id = ? OR character_id IS NULL)
             AND enabled = 1 AND always_active = 1
             ORDER BY priority DESC"
        )
        .bind(char_id)
        .fetch_all(db)
        .await?;

        for entry in lorebook_entries {
            prompt.push(ChatMessage {
                role: MessageRole::System,
                content: entry,
            });
        }
    }

    // Walk the message tree from root to the current message
    let mut chain = Vec::new();
    let mut current_id = Some(up_to_message_id.to_string());

    while let Some(ref id) = current_id {
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT role, content, parent_id FROM messages WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db)
        .await?;

        match row {
            Some((role, content, parent)) => {
                chain.push(ChatMessage {
                    role: match role.as_str() {
                        "user" => MessageRole::User,
                        "assistant" => MessageRole::Assistant,
                        "system" => MessageRole::System,
                        _ => MessageRole::User,
                    },
                    content,
                });
                current_id = parent;
            }
            None => break,
        }
    }

    chain.reverse();

    // Keyword-triggered lorebook entries: scan recent messages for matching keywords
    if let Some(ref char_id) = character_id {
        let keyword_entries: Vec<(String, String)> = sqlx::query_as(
            "SELECT keys, content FROM lorebook_entries
             WHERE (character_id = ? OR character_id IS NULL)
             AND enabled = 1 AND always_active = 0
             AND keys IS NOT NULL AND keys != '' AND keys != '[]'
             ORDER BY priority DESC"
        )
        .bind(char_id)
        .fetch_all(db)
        .await?;

        if !keyword_entries.is_empty() {
            // Build a search corpus from the last N messages (for performance)
            let scan_depth = chain.len().min(20);
            let corpus: String = chain[chain.len().saturating_sub(scan_depth)..]
                .iter()
                .map(|m| m.content.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");

            for (keys_raw, content) in keyword_entries {
                // keys column is a JSON array e.g. ["wolf","storm"] — fall back to CSV
                let keywords: Vec<String> = serde_json::from_str::<Vec<String>>(&keys_raw)
                    .unwrap_or_else(|_| {
                        keys_raw.split(',').map(|k| k.trim().to_string()).collect()
                    });

                let triggered = keywords
                    .iter()
                    .map(|k| k.to_lowercase())
                    .filter(|k| !k.is_empty())
                    .any(|keyword| corpus.contains(&keyword));

                if triggered {
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content,
                    });
                }
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
        let memory_rows: Vec<(String,)> = if memory_scope == "character" {
            // Character-scoped: all memories for this character (shared across conversations)
            if let Some(ref char_id) = character_id {
                sqlx::query_as(
                    "SELECT content FROM memories
                     WHERE character_id = ?
                     ORDER BY created_at DESC LIMIT 20"
                )
                .bind(char_id)
                .fetch_all(db)
                .await?
            } else {
                Vec::new()
            }
        } else {
            // Conversation-scoped: only this conversation's memories
            sqlx::query_as(
                "SELECT content FROM memories
                 WHERE conversation_id = ?
                 ORDER BY created_at DESC LIMIT 20"
            )
            .bind(conversation_id)
            .fetch_all(db)
            .await?
        };

        if !memory_rows.is_empty() {
            // Format memories as a bullet list, reversed to chronological order
            let mut facts: Vec<String> = memory_rows.into_iter().map(|(c,)| c).collect();
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
    if let Some(ref char_id) = character_id {
        let state_row: Option<(i32, i32, i32, String, String)> = sqlx::query_as(
            "SELECT mood, trust, arousal, dominant_emotion, state_summary
             FROM character_states
             WHERE character_id = ? AND conversation_id = ?
             LIMIT 1",
        )
        .bind(char_id)
        .bind(conversation_id)
        .fetch_optional(db)
        .await?;

        if let Some((mood, trust, arousal, emotion, summary)) = state_row {
            let state_block = format!(
                "[Current Emotional State]\n\
                 Dominant emotion: {emotion}\n\
                 Mood: {mood}/100  Trust: {trust}/100  Intensity: {arousal}/100\n\
                 Internal state: {summary}\n\
                 (Let this emotional state colour your response naturally — do not announce or describe it explicitly.)"
            );
            prompt.push(ChatMessage {
                role: MessageRole::System,
                content: state_block,
            });
        }
    }

    prompt.extend(chain);

    // ── Post-History Instructions (PHI) ──
    // Injected AFTER the conversation history as the very last system message.
    // This carries maximum attention weight and shapes how the AI structures
    // its response — narrative hooks, scene transitions, pacing directives.
    // Equivalent to SillyTavern's "Post-History Instructions" feature.
    if let Some(phi) = post_history_instructions {
        let trimmed = phi.trim();
        if !trimmed.is_empty() {
            prompt.push(ChatMessage {
                role: MessageRole::System,
                content: trimmed.to_string(),
            });
        }
    }

    Ok(prompt)
}

/// Finds the default LLM provider configuration from the database.
async fn get_default_llm_provider(
    db: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<ProviderConfig, MythicError> {
    let row: Option<(String, String, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, provider_type, adapter, config, is_default
         FROM provider_configs
         WHERE provider_type = 'llm'
         ORDER BY is_default DESC
         LIMIT 1"
    )
    .fetch_optional(db)
    .await?;

    match row {
        Some((id, name, _provider_type, adapter, config, is_default)) => {
            Ok(ProviderConfig {
                id,
                name,
                provider_type: ProviderType::Llm,
                adapter: match adapter.as_str() {
                    "ollama" => ProviderAdapter::Ollama,
                    "open_router" => ProviderAdapter::OpenRouter,
                    _ => ProviderAdapter::OpenAiCompatible,
                },
                config: serde_json::from_str(&config)?,
                is_default,
            })
        }
        None => Err(MythicError::Config(
            "No LLM provider configured. Add one in Settings → Models.".to_string()
        )),
    }
}

/// Creates a concrete LLM provider instance from config.
fn create_llm_provider(
    config: &ProviderConfig,
    http: reqwest::Client,
) -> Result<Box<dyn LlmProvider>, MythicError> {
    match config.adapter {
        ProviderAdapter::OpenRouter => {
            let api_key = config.config
                .get("api_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MythicError::Config("OpenRouter API key missing".to_string()))?;

            Ok(Box::new(OpenRouterProvider::new(http, api_key)?))
        }
        ProviderAdapter::Ollama => {
            let base_url = config.config
                .get("base_url")
                .and_then(|v| v.as_str());

            Ok(Box::new(OllamaProvider::new(http, base_url)))
        }
        ProviderAdapter::OpenAiCompatible => {
            let base_url = config.config
                .get("base_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MythicError::Config("OpenAI-compatible base URL missing".to_string()))?;

            let api_key = config.config
                .get("api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut headers = reqwest::header::HeaderMap::new();
            if !api_key.is_empty() {
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", api_key))
                        .map_err(|_| MythicError::Config("Invalid API key format".to_string()))?,
                );
            }

            let client = OpenAiClient::new(http, OpenAiClientConfig {
                base_url: format!("{}/v1", base_url.trim_end_matches("/v1").trim_end_matches('/')),
                headers,
                default_model: config.config.get("model").and_then(|v| v.as_str()).map(String::from),
            });

            // Wrap in a generic adapter struct
            Ok(Box::new(GenericOpenAiProvider { client }))
        }
        _ => Err(MythicError::Config(format!(
            "Unsupported LLM adapter: {:?}", config.adapter
        ))),
    }
}

/// Generic OpenAI-compatible provider for LM Studio, KoboldCPP, vLLM, etc.
struct GenericOpenAiProvider {
    client: OpenAiClient,
}

#[async_trait::async_trait]
impl LlmProvider for GenericOpenAiProvider {
    fn name(&self) -> &str { "OpenAI Compatible" }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, MythicError> {
        self.client.list_models().await
    }

    async fn generate(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        self.client.generate(model, messages, params).await
    }

    async fn generate_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        self.client.generate_stream(model, messages, params, tx).await
    }

    async fn health_check(&self) -> Result<bool, MythicError> {
        self.client.health_check().await
    }
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
    let http = state_guard.http_client.clone();
    drop(state_guard);

    let provider_config = get_default_llm_provider(&db).await?;
    let model_id = match model {
        Some(m) if !m.is_empty() && m != "unknown" => m,
        _ => {
            let stored = provider_config.config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !stored.is_empty() && stored != "unknown" {
                stored.to_string()
            } else {
                // Fall back to first enabled model for this provider
                let first_enabled: Option<(String,)> = sqlx::query_as(
                    "SELECT model_id FROM enabled_models WHERE provider_id = ? AND enabled = 1 LIMIT 1"
                )
                .bind(&provider_config.id)
                .fetch_optional(&db)
                .await?;
                match first_enabled {
                    Some((m,)) => m,
                    None => return Err(MythicError::Config(
                        "No model selected. Go to AI Studio \u{2192} Models, enable at least one model.".to_string()
                    )),
                }
            }
        }
    };



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

    let provider = create_llm_provider(&provider_config, http)?;
    let result = provider.generate(&model_id, &messages, &gen_params).await?;

    Ok(result)
}
