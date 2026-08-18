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
use rig_core::client::Nothing;
use rig_core::completion::Message;
use rig_core::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface, hyperbolic, moonshot, ollama, openai,
    openrouter, perplexity, together, xai,
};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tracing::{debug, error};

/// How long to wait for the provider to produce its NEXT stream event
/// (connection + first token, or the gap between subsequent chunks) before
/// giving up. Without this, a provider that accepts the request but never
/// responds (seen in practice with overloaded free-tier OpenRouter models)
/// leaves the whole send hanging forever with no error and no way to
/// recover short of restarting the app.
const STREAM_EVENT_TIMEOUT: Duration = Duration::from_secs(90);

use rig_core::message::{ImageMediaType, UserContent};
use rig_core::OneOrMany;

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
        images: &[(Vec<u8>, String)],
        params: &GenerationParams,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        let rig_messages = convert_messages(messages, images);
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
                // Chain-of-thought/thinking content, kept separate from the
                // visible reply — reasoning models (Nemotron, DeepSeek R1,
                // etc.) narrate their thought process ("The user is greeting
                // Aria, I should...") before the actual in-character reply,
                // and that narration must never be forwarded live or stored
                // as if the character said it. Only used as a last-resort
                // fallback below, for providers that route their entire
                // answer through Reasoning blocks with no Text at all.
                let mut reasoning_text = String::new();
                let mut sent_final = false;
                loop {
                    let item = match timeout(STREAM_EVENT_TIMEOUT, stream.next()).await {
                        Ok(Some(item)) => item,
                        Ok(None) => break, // stream ended normally
                        Err(_) => {
                            error!(
                                "[RigProvider] stream timed out — no event from provider for {}s",
                                STREAM_EVENT_TIMEOUT.as_secs()
                            );
                            let _ = tx.send(StreamChunk::Error(format!(
                                "The provider stopped responding (no reply for {}s). It may be overloaded — try again or switch models.",
                                STREAM_EVENT_TIMEOUT.as_secs()
                            ))).await;
                            return Ok(());
                        }
                    };
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
                            let text = reasoning.display_text();
                            if !text.is_empty() {
                                reasoning_text.push_str(&text);
                                if tx.send(StreamChunk::ReasoningDelta(text)).await.is_err() {
                                    debug!("[RigProvider] channel closed, stopping stream");
                                    sent_final = true;
                                    break;
                                }
                            }
                        }
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ReasoningDelta { ref reasoning, .. }
                        )) => {
                            if !reasoning.is_empty() {
                                reasoning_text.push_str(reasoning);
                                if tx.send(StreamChunk::ReasoningDelta(reasoning.clone())).await.is_err() {
                                    debug!("[RigProvider] channel closed, stopping stream");
                                    sent_final = true;
                                    break;
                                }
                            }
                        }
                        Ok(MultiTurnStreamItem::FinalResponse(fin)) => {
                            let final_text = if !full_text.is_empty() {
                                full_text.clone()
                            } else if !reasoning_text.trim().is_empty() {
                                reasoning_text.clone()
                            } else {
                                fin.response().to_string()
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
                    let salvaged = if !full_text.trim().is_empty() { &full_text } else { &reasoning_text };
                    if salvaged.trim().is_empty() {
                        error!("[RigProvider] stream ended without FinalResponse and no text received");
                        let _ = tx.send(StreamChunk::Error(
                            "Provider stream closed without a response. Please verify your API key and model settings.".to_string()
                        )).await;
                    } else {
                        debug!("[RigProvider] stream ended without FinalResponse, but text was received — treating as done");
                        let _ = tx.send(StreamChunk::Done(salvaged.clone())).await;
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
        images: &[(Vec<u8>, String)],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        let rig_messages = convert_messages(messages, images);
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
            self.name(),
            model_id,
            texts.len()
        );

        macro_rules! embed_with {
            ($client:expr) => {{
                use rig_core::client::EmbeddingsClient;
                use rig_core::embeddings::EmbeddingModel;
                let model = $client.embedding_model(model_id);
                let embeddings = model
                    .embed_texts(texts)
                    .await
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
///
/// `images` (raw bytes + MIME type, resolved from a message's stored
/// attachments — see `commands::chat::attachments::load_message_images`) are attached
/// to the LAST user-role message only, i.e. the current turn's prompt.
/// This works generically across every rig-backed adapter (OpenAI,
/// Anthropic, OpenRouter, Gemini, Ollama, etc.) with no per-provider code —
/// rig itself serializes `UserContent::Image` into each provider's own
/// wire format.
fn convert_messages(messages: &[ChatMessage], images: &[(Vec<u8>, String)]) -> Vec<Message> {
    let last_user_idx = if images.is_empty() {
        None
    } else {
        messages.iter().rposition(|m| m.role == MessageRole::User)
    };

    messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role != MessageRole::System)
        .map(|(i, m)| match m.role {
            MessageRole::User if Some(i) == last_user_idx => {
                let content =
                    OneOrMany::many(std::iter::once(UserContent::text(&m.content)).chain(
                        images.iter().map(|(bytes, mime)| {
                            UserContent::image_raw(bytes.clone(), image_media_type(mime), None)
                        }),
                    ))
                    .expect("non-empty: UserContent::text is always present");
                Message::User { content }
            }
            MessageRole::User => Message::user(&m.content),
            MessageRole::Assistant => Message::assistant(&m.content),
            MessageRole::System => unreachable!(), // filtered above
        })
        .collect()
}

/// Maps a stored MIME type (from `MessageAttachment`/`upload_message_attachment`)
/// to rig's `ImageMediaType`. `None` for anything rig doesn't recognize —
/// rig/the provider can still often infer the type from the image bytes
/// themselves, so this is a best-effort hint, not a hard requirement.
fn image_media_type(mime: &str) -> Option<ImageMediaType> {
    match mime {
        "image/png" => Some(ImageMediaType::PNG),
        "image/jpeg" => Some(ImageMediaType::JPEG),
        "image/webp" => Some(ImageMediaType::WEBP),
        "image/gif" => Some(ImageMediaType::GIF),
        _ => None,
    }
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
        Message::User { content } => match content.first() {
            rig_core::message::UserContent::Text(t) => t.text.clone(),
            _ => String::new(),
        },
        Message::Assistant { content, .. } => match content.first() {
            rig_core::completion::AssistantContent::Text(t) => t.text.clone(),
            _ => String::new(),
        },
        Message::System { content } => content.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_msg(role: MessageRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
        }
    }

    // ── convert_messages ───────────────────────────────────────────

    #[test]
    fn convert_messages_filters_out_system_messages() {
        let messages = vec![
            chat_msg(MessageRole::System, "You are a helpful assistant."),
            chat_msg(MessageRole::User, "Hi"),
            chat_msg(MessageRole::Assistant, "Hello!"),
        ];
        let rig_messages = convert_messages(&messages, &[]);
        assert_eq!(rig_messages.len(), 2);
        assert!(matches!(rig_messages[0], Message::User { .. }));
        assert!(matches!(rig_messages[1], Message::Assistant { .. }));
    }

    #[test]
    fn convert_messages_preserves_user_and_assistant_text() {
        let messages = vec![
            chat_msg(MessageRole::User, "What's the weather?"),
            chat_msg(MessageRole::Assistant, "It's sunny."),
        ];
        let rig_messages = convert_messages(&messages, &[]);
        assert_eq!(
            extract_text_from_message(&rig_messages[0]),
            "What's the weather?"
        );
        assert_eq!(extract_text_from_message(&rig_messages[1]), "It's sunny.");
    }

    #[test]
    fn convert_messages_without_images_produces_plain_text_user_message() {
        let messages = vec![chat_msg(MessageRole::User, "hello")];
        let rig_messages = convert_messages(&messages, &[]);
        match &rig_messages[0] {
            Message::User { content } => assert_eq!(content.len(), 1),
            other => panic!("expected User message, got {other:?}"),
        }
    }

    #[test]
    fn convert_messages_attaches_images_only_to_last_user_message() {
        let messages = vec![
            chat_msg(MessageRole::User, "first turn"),
            chat_msg(MessageRole::Assistant, "reply"),
            chat_msg(MessageRole::User, "second turn, with an image"),
        ];
        let images = vec![(vec![0u8, 1, 2, 3], "image/png".to_string())];
        let rig_messages = convert_messages(&messages, &images);

        // First user message: text-only (1 content item).
        match &rig_messages[0] {
            Message::User { content } => assert_eq!(content.len(), 1),
            other => panic!("expected User message, got {other:?}"),
        }
        // Last user message: text + 1 image = 2 content items.
        match &rig_messages[2] {
            Message::User { content } => assert_eq!(content.len(), 2),
            other => panic!("expected User message, got {other:?}"),
        }
    }

    // ── extract_system_preamble ────────────────────────────────────

    #[test]
    fn extract_system_preamble_returns_none_when_no_system_messages() {
        let messages = vec![chat_msg(MessageRole::User, "hi")];
        assert_eq!(extract_system_preamble(&messages), None);
    }

    #[test]
    fn extract_system_preamble_joins_multiple_system_messages() {
        let messages = vec![
            chat_msg(MessageRole::System, "You are Aria."),
            chat_msg(MessageRole::User, "hi"),
            chat_msg(MessageRole::System, "Stay in character."),
        ];
        assert_eq!(
            extract_system_preamble(&messages),
            Some("You are Aria.\n\nStay in character.".to_string())
        );
    }

    // ── split_prompt_and_history ───────────────────────────────────

    #[test]
    fn split_prompt_and_history_on_empty_input() {
        let (prompt, history) = split_prompt_and_history(&[]);
        assert_eq!(prompt, "");
        assert!(history.is_empty());
    }

    #[test]
    fn split_prompt_and_history_separates_last_message_as_prompt() {
        let messages = vec![
            Message::user("first"),
            Message::assistant("second"),
            Message::user("third"),
        ];
        let (prompt, history) = split_prompt_and_history(&messages);
        assert_eq!(prompt, "third");
        assert_eq!(history.len(), 2);
    }

    // ── extract_text_from_message ──────────────────────────────────

    #[test]
    fn extract_text_from_message_handles_all_roles() {
        assert_eq!(extract_text_from_message(&Message::user("u")), "u");
        assert_eq!(extract_text_from_message(&Message::assistant("a")), "a");
        assert_eq!(extract_text_from_message(&Message::system("s")), "s");
    }

    // ── image_media_type ────────────────────────────────────────────

    #[test]
    fn image_media_type_maps_known_mime_types() {
        assert!(matches!(
            image_media_type("image/png"),
            Some(ImageMediaType::PNG)
        ));
        assert!(matches!(
            image_media_type("image/jpeg"),
            Some(ImageMediaType::JPEG)
        ));
        assert!(matches!(
            image_media_type("image/webp"),
            Some(ImageMediaType::WEBP)
        ));
        assert!(matches!(
            image_media_type("image/gif"),
            Some(ImageMediaType::GIF)
        ));
    }

    #[test]
    fn image_media_type_returns_none_for_unknown_mime_type() {
        assert_eq!(image_media_type("image/bmp"), None);
        assert_eq!(image_media_type("application/octet-stream"), None);
    }
}
