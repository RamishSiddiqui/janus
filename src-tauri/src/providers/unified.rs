//! Unified LLM provider backed by rig-core.
//!
//! Resolves any supported provider from DB config at runtime.
//! Supports streaming via rig's native streaming API.
//!
//! ## Supported providers (zero per-provider code needed):
//! OpenAI, Anthropic, OpenRouter, Gemini, Ollama, Cohere, DeepSeek,
//! Groq, Perplexity, xAI, HuggingFace, Hyperbolic, Moonshot, Together
//!
//! OpenAI-compatible endpoints (LM Studio, KoboldCPP, vLLM) use the
//! OpenAI provider with a custom `base_url`.

use futures::StreamExt;
use rig_core::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface,
    hyperbolic, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};
use rig_core::client::Nothing;
use rig_core::completion::Message;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::traits::StreamChunk;

/// All rig-supported provider clients wrapped in a single enum.
/// Each variant holds the native rig client for that provider.
pub enum RigProvider {
    OpenAI(openai::Client),
    Anthropic(anthropic::Client),
    OpenRouter(openrouter::Client),
    Gemini(gemini::Client),
    Ollama(ollama::Client),
    Cohere(cohere::Client),
    DeepSeek(deepseek::Client),
    Groq(groq::Client),
    Perplexity(perplexity::Client),
    Xai(xai::Client),
    HuggingFace(huggingface::Client),
    Hyperbolic(hyperbolic::Client),
    Moonshot(moonshot::Client),
    Together(together::Client),
}

impl RigProvider {
    /// Creates a provider from DB config fields.
    ///
    /// `adapter` is the string stored in the DB (e.g., "openrouter", "anthropic").
    /// `api_key` and `base_url` are extracted from the provider's JSON config.
    pub fn from_config(
        adapter: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<Self, MythicError> {
        let key = api_key.unwrap_or("");

        match adapter {
            "openai" | "openai_compatible" | "open_ai_compatible" | "lm_studio" | "lmstudio" => {
                let client = if let Some(url) = base_url {
                    openai::Client::builder()
                        .api_key(key)
                        .base_url(url)
                        .build()
                        .map_err(|e| MythicError::Config(format!("OpenAI client error: {e}")))?
                } else {
                    openai::Client::new(key)
                        .map_err(|e| MythicError::Config(format!("OpenAI client error: {e}")))?
                };
                Ok(Self::OpenAI(client))
            }
            "anthropic" => {
                let client = if let Some(url) = base_url {
                    anthropic::Client::builder()
                        .api_key(key)
                        .base_url(url)
                        .build()
                        .map_err(|e| MythicError::Config(format!("Anthropic client error: {e}")))?
                } else {
                    anthropic::Client::builder()
                        .api_key(key)
                        .build()
                        .map_err(|e| MythicError::Config(format!("Anthropic client error: {e}")))?
                };
                Ok(Self::Anthropic(client))
            }
            "open_router" | "openrouter" => {
                let client = if let Some(url) = base_url {
                    openrouter::Client::builder()
                        .api_key(key)
                        .base_url(url)
                        .build()
                        .map_err(|e| MythicError::Config(format!("OpenRouter client error: {e}")))?
                } else {
                    openrouter::Client::new(key)
                        .map_err(|e| MythicError::Config(format!("OpenRouter client error: {e}")))?
                };
                Ok(Self::OpenRouter(client))
            }
            "gemini" => {
                let client = if let Some(url) = base_url {
                    gemini::Client::builder()
                        .api_key(key)
                        .base_url(url)
                        .build()
                        .map_err(|e| MythicError::Config(format!("Gemini client error: {e}")))?
                } else {
                    gemini::Client::new(key)
                        .map_err(|e| MythicError::Config(format!("Gemini client error: {e}")))?
                };
                Ok(Self::Gemini(client))
            }
            "ollama" => {
                let client = if let Some(url) = base_url {
                    ollama::Client::builder()
                        .api_key(Nothing)
                        .base_url(url)
                        .build()
                        .map_err(|e| MythicError::Config(format!("Ollama client error: {e}")))?
                } else {
                    ollama::Client::new(Nothing)
                        .map_err(|e| MythicError::Config(format!("Ollama client error: {e}")))?
                };
                Ok(Self::Ollama(client))
            }
            "cohere" => {
                let client = cohere::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Cohere client error: {e}")))?;
                Ok(Self::Cohere(client))
            }
            "deepseek" | "deep_seek" => {
                let client = deepseek::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("DeepSeek client error: {e}")))?;
                Ok(Self::DeepSeek(client))
            }
            "groq" => {
                let client = groq::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Groq client error: {e}")))?;
                Ok(Self::Groq(client))
            }
            "perplexity" => {
                let client = perplexity::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Perplexity client error: {e}")))?;
                Ok(Self::Perplexity(client))
            }
            "xai" => {
                let client = xai::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("xAI client error: {e}")))?;
                Ok(Self::Xai(client))
            }
            "hugging_face" | "huggingface" => {
                let client = huggingface::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("HuggingFace client error: {e}")))?;
                Ok(Self::HuggingFace(client))
            }
            "hyperbolic" => {
                let client = hyperbolic::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Hyperbolic client error: {e}")))?;
                Ok(Self::Hyperbolic(client))
            }
            "moonshot" => {
                let client = moonshot::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Moonshot client error: {e}")))?;
                Ok(Self::Moonshot(client))
            }
            "together" => {
                let client = together::Client::new(key)
                    .map_err(|e| MythicError::Config(format!("Together client error: {e}")))?;
                Ok(Self::Together(client))
            }
            other => Err(MythicError::Config(format!(
                "Unsupported LLM provider adapter: '{}'. Supported: openai, anthropic, openrouter, \
                 gemini, ollama, lm_studio, cohere, deepseek, groq, perplexity, xai, huggingface, \
                 hyperbolic, moonshot, together",
                other
            ))),
        }
    }

    /// Returns the provider name for logging.
    pub fn name(&self) -> &str {
        match self {
            Self::OpenAI(_) => "OpenAI",
            Self::Anthropic(_) => "Anthropic",
            Self::OpenRouter(_) => "OpenRouter",
            Self::Gemini(_) => "Gemini",
            Self::Ollama(_) => "Ollama",
            Self::Cohere(_) => "Cohere",
            Self::DeepSeek(_) => "DeepSeek",
            Self::Groq(_) => "Groq",
            Self::Perplexity(_) => "Perplexity",
            Self::Xai(_) => "xAI",
            Self::HuggingFace(_) => "HuggingFace",
            Self::Hyperbolic(_) => "Hyperbolic",
            Self::Moonshot(_) => "Moonshot",
            Self::Together(_) => "Together",
        }
    }

    /// Streams a chat completion, sending chunks through the mpsc channel.
    pub async fn generate_stream(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        let rig_messages = convert_messages(messages);
        let preamble = extract_system_preamble(messages);

        debug!(
            "[RigProvider::generate_stream] provider={}, model={}, msg_count={}",
            self.name(),
            model_id,
            rig_messages.len()
        );

        /// Macro that builds a rig agent, starts streaming, and forwards
        /// MultiTurnStreamItem chunks to our mpsc channel as StreamChunks.
        macro_rules! stream_with {
            ($client:expr) => {{
                use rig_core::agent::MultiTurnStreamItem;
                use rig_core::client::CompletionClient;
                use rig_core::streaming::{StreamingChat, StreamedAssistantContent};

                let mut agent_builder = $client.agent(model_id);
                if let Some(ref pre) = preamble {
                    agent_builder = agent_builder.preamble(pre);
                }
                agent_builder = agent_builder.temperature(params.temperature as f64);
                let agent = agent_builder.build();

                let (prompt, history) = split_prompt_and_history(&rig_messages);
                debug!("[RigProvider] prompt_len={}, history_len={}", prompt.len(), history.len());

                // stream_chat returns a StreamingPromptRequest builder;
                // .await calls IntoFuture which produces the stream directly
                // (Pin<Box<dyn Stream<Item = Result<MultiTurnStreamItem<R>, StreamingError>>>>)
                let mut stream = agent.stream_chat(&prompt, history).await;

                let mut full_text = String::new();
                let mut sent_final = false;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Text(text)
                        )) => {
                            full_text.push_str(&text.text);
                            if tx.send(StreamChunk::Delta(text.text.clone())).await.is_err() {
                                debug!("[RigProvider] channel closed, stopping stream");
                                sent_final = true;
                                break;
                            }
                        }
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Reasoning(ref reasoning)
                        )) => {
                            // Some models (e.g. reasoning models) send their content
                            // through Reasoning blocks instead of Text blocks.
                            let text = reasoning.display_text();
                            if !text.is_empty() {
                                full_text.push_str(&text);
                                if tx.send(StreamChunk::Delta(text)).await.is_err() {
                                    debug!("[RigProvider] channel closed, stopping stream");
                                    sent_final = true;
                                    break;
                                }
                            }
                        }
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ReasoningDelta { ref reasoning, .. }
                        )) => {
                            // Incremental reasoning deltas — forward text to frontend
                            if !reasoning.is_empty() {
                                full_text.push_str(reasoning);
                                if tx.send(StreamChunk::Delta(reasoning.clone())).await.is_err() {
                                    debug!("[RigProvider] channel closed, stopping stream");
                                    sent_final = true;
                                    break;
                                }
                            }
                        }
                        Ok(MultiTurnStreamItem::FinalResponse(fin)) => {
                            let final_text = if full_text.is_empty() {
                                fin.response().to_string()
                            } else {
                                full_text.clone()
                            };
                            
                            if final_text.trim().is_empty() {
                                let _ = tx.send(StreamChunk::Error("Received empty response from the provider. Please verify your API key and model settings.".to_string())).await;
                            } else {
                                let _ = tx.send(StreamChunk::Done(final_text)).await;
                            }
                            sent_final = true;
                            break;
                        }
                        Ok(_) => {
                            // ToolCallDelta, Final, etc. — skip silently
                        }
                        Err(e) => {
                            error!("[RigProvider] stream error: {e}");
                            let _ = tx.send(StreamChunk::Error(format!("{e}"))).await;
                            return Ok(());
                        }
                    }
                }

                // If the stream closed without a FinalResponse (provider returned
                // None / dropped the connection), we still need to notify the frontend.
                if !sent_final {
                    if full_text.trim().is_empty() {
                        error!("[RigProvider] stream ended without FinalResponse and no text received");
                        let _ = tx.send(StreamChunk::Error(
                            "Provider stream closed without a response. Please verify your API key and model settings.".to_string()
                        )).await;
                    } else {
                        debug!("[RigProvider] stream ended without FinalResponse, but text was received — treating as done");
                        let _ = tx.send(StreamChunk::Done(full_text)).await;
                    }
                }
                Ok(())
            }};
        }

        match self {
            Self::OpenAI(c) => stream_with!(c),
            Self::Anthropic(c) => stream_with!(c),
            Self::OpenRouter(c) => stream_with!(c),
            Self::Gemini(c) => stream_with!(c),
            Self::Ollama(c) => stream_with!(c),
            Self::Cohere(c) => stream_with!(c),
            Self::DeepSeek(c) => stream_with!(c),
            Self::Groq(c) => stream_with!(c),
            Self::Perplexity(c) => stream_with!(c),
            Self::Xai(c) => stream_with!(c),
            Self::HuggingFace(c) => stream_with!(c),
            Self::Hyperbolic(c) => stream_with!(c),
            Self::Moonshot(c) => stream_with!(c),
            Self::Together(c) => stream_with!(c),
        }
    }

    /// Non-streaming chat completion. Used for internal tasks like
    /// memory extraction and summarization.
    pub async fn generate(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        let rig_messages = convert_messages(messages);
        let preamble = extract_system_preamble(messages);

        debug!(
            "[RigProvider::generate] provider={}, model={}, msg_count={}",
            self.name(),
            model_id,
            rig_messages.len()
        );

        macro_rules! complete_with {
            ($client:expr) => {{
                use rig_core::client::CompletionClient;
                use rig_core::completion::Chat;

                let mut agent_builder = $client.agent(model_id);
                if let Some(ref pre) = preamble {
                    agent_builder = agent_builder.preamble(pre);
                }
                agent_builder = agent_builder.temperature(params.temperature as f64);
                let agent = agent_builder.build();

                let (prompt, mut history) = split_prompt_and_history(&rig_messages);

                let response = agent.chat(&prompt, &mut history).await
                    .map_err(|e| MythicError::Provider(format!("{e}")))?;

                if response.trim().is_empty() {
                    return Err(MythicError::Provider("Received empty response from the provider. Please verify your API key and model settings.".to_string()));
                }

                Ok(response)
            }};
        }

        match self {
            Self::OpenAI(c) => complete_with!(c),
            Self::Anthropic(c) => complete_with!(c),
            Self::OpenRouter(c) => complete_with!(c),
            Self::Gemini(c) => complete_with!(c),
            Self::Ollama(c) => complete_with!(c),
            Self::Cohere(c) => complete_with!(c),
            Self::DeepSeek(c) => complete_with!(c),
            Self::Groq(c) => complete_with!(c),
            Self::Perplexity(c) => complete_with!(c),
            Self::Xai(c) => complete_with!(c),
            Self::HuggingFace(c) => complete_with!(c),
            Self::Hyperbolic(c) => complete_with!(c),
            Self::Moonshot(c) => complete_with!(c),
            Self::Together(c) => complete_with!(c),
        }
    }

    /// Generate embeddings for a batch of texts.
    /// Uses rig-core's EmbeddingsClient trait — works across providers
    /// that support embeddings.
    pub async fn generate_embedding(
        &self,
        model_id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, MythicError> {
        debug!(
            "[RigProvider::generate_embedding] provider={}, model={}, text_count={}",
            self.name(), model_id, texts.len()
        );

        macro_rules! embed_with {
            ($client:expr) => {{
                use rig_core::client::EmbeddingsClient;
                use rig_core::embeddings::EmbeddingModel;
                let model = $client.embedding_model(model_id);
                let embeddings = model.embed_texts(texts).await
                    .map_err(|e| MythicError::Provider(format!("Embedding error: {e}")))?;
                Ok(embeddings.iter().map(|e| e.vec.clone()).collect())
            }};
        }

        match self {
            Self::OpenAI(c) => embed_with!(c),
            Self::OpenRouter(c) => embed_with!(c),
            Self::Ollama(c) => embed_with!(c),
            Self::Cohere(c) => {
                use rig_core::embeddings::EmbeddingModel;
                let model = c.embedding_model(model_id, "search_document");
                let embeddings = model.embed_texts(texts).await
                    .map_err(|e| MythicError::Provider(format!("Embedding error: {e}")))?;
                Ok(embeddings.iter().map(|e| e.vec.clone()).collect())
            }
            Self::Gemini(c) => embed_with!(c),
            Self::Together(c) => embed_with!(c),
            _ => Err(MythicError::Config(format!(
                "Provider '{}' does not support embeddings. Use OpenAI, OpenRouter, Ollama, Gemini, Cohere, or Together.",
                self.name()
            ))),
        }
    }
}

// ── Helper functions ───────────────────────────────────────────────

/// Converts Mythic's `ChatMessage` array to rig's `Message` array.
/// Filters out system messages (those become the agent's preamble).
fn convert_messages(messages: &[ChatMessage]) -> Vec<Message> {
    messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| match m.role {
            MessageRole::User => Message::user(&m.content),
            MessageRole::Assistant => Message::assistant(&m.content),
            MessageRole::System => unreachable!(), // filtered above
        })
        .collect()
}

/// Extracts and concatenates all system-role messages into a single preamble.
fn extract_system_preamble(messages: &[ChatMessage]) -> Option<String> {
    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();

    if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    }
}

/// Splits a message list into (last_user_prompt, remaining_history).
///
/// rig's `stream_chat(prompt, history)` expects the current turn's
/// prompt separately from the conversation history.
fn split_prompt_and_history(messages: &[Message]) -> (String, Vec<Message>) {
    if messages.is_empty() {
        return (String::new(), vec![]);
    }

    let last = &messages[messages.len() - 1];
    let prompt = extract_text_from_message(last);
    let history = messages[..messages.len() - 1].to_vec();

    (prompt, history)
}

/// Extracts the plain text content from a rig `Message`.
fn extract_text_from_message(msg: &Message) -> String {
    match msg {
        Message::User { content } => {
            match content.first() {
                rig_core::message::UserContent::Text(t) => t.text.clone(),
                _ => String::new(),
            }
        }
        Message::Assistant { content, .. } => {
            match content.first() {
                rig_core::completion::AssistantContent::Text(t) => t.text.clone(),
                _ => String::new(),
            }
        }
        Message::System { content } => content.clone(),
    }
}
