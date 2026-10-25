//! Unified LLM provider backed by rig-core.
//!
//! Resolves any supported provider from DB config at runtime.
//! Streams through rig's model API: one request type for every provider, and
//! one event stream that carries answer text and reasoning separately.
//!
//! ## Supported providers (zero per-provider code needed):
//! OpenAI, Anthropic, OpenRouter, Gemini, Ollama, Cohere, DeepSeek,
//! Groq, Perplexity, xAI, HuggingFace, Hyperbolic, Moonshot, Together
//!
//! OpenAI-compatible endpoints (LM Studio, KoboldCPP, vLLM) use the
//! OpenAI provider with a custom `base_url`, over Chat Completions. OpenAI
//! itself (no custom `base_url`) uses the Responses API.

use futures::StreamExt;
use rig_core::completion::{CompletionRequest, Message};
use rig_core::message::{ImageMediaType, UserContent};
use rig_core::operation::{Completion, Embedding};
use rig_core::providers::openai::wire::{
    DEEPSEEK, GROQ, HUGGINGFACE, HYPERBOLIC, MOONSHOT, OPENAI, OPENROUTER, PERPLEXITY, TOGETHER,
};
use rig_core::providers::openai::{OpenAI, OpenAIConfig};
use rig_core::providers::{
    anthropic::{Anthropic, AnthropicConfig},
    cohere::{Cohere, CohereConfig},
    gemini::{Gemini, GeminiConfig},
    ollama::{Ollama, OllamaConfig},
    xai,
};
use rig_core::streaming::{Item, StreamEvent};
use rig_core::DynModel;
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

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::traits::StreamChunk;

/// Which OpenAI wire a client talks.
#[derive(Clone, Copy)]
enum OpenAIApi {
    /// The Responses API: OpenAI's current API, used for OpenAI itself.
    Responses,
    /// Chat Completions: what LM Studio, KoboldCPP, vLLM and the like speak.
    Chat,
    /// The dialect's own flagship endpoint (OpenRouter, Groq, DeepSeek, ...).
    Flagship,
}

/// The rig client for one provider family. Every OpenAI-shaped vendor shares
/// the `OpenAI` client with a different dialect.
enum Backend {
    // Boxed: the OpenAI config is several times the size of the others.
    OpenAI(Box<OpenAI>, OpenAIApi),
    Anthropic(Anthropic),
    Gemini(Gemini),
    Ollama(Ollama),
    Cohere(Cohere),
}

/// A provider resolved from the DB config: its display name plus the rig
/// client that talks to it.
pub struct RigProvider {
    name: &'static str,
    backend: Backend,
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

        // OpenAI-shaped vendor on its dialect's default endpoint, or on
        // `base_url` for the ones where users can point it elsewhere.
        let shaped = |name: &'static str, config: OpenAIConfig, api: OpenAIApi| Self {
            name,
            backend: Backend::OpenAI(Box::new(config.client()), api),
        };
        let with_url = |config: OpenAIConfig| match base_url {
            Some(url) => config.with_base_url(url),
            None => config,
        };

        match adapter {
            "openai" | "openai_compatible" | "open_ai_compatible" | "lm_studio" | "lmstudio" => {
                // A custom base_url means an OpenAI-compatible server, which
                // speaks Chat Completions; the real OpenAI gets Responses.
                let api = if base_url.is_some() {
                    OpenAIApi::Chat
                } else {
                    OpenAIApi::Responses
                };
                Ok(shaped(
                    "OpenAI",
                    with_url(OpenAIConfig::with_key(&OPENAI, key)),
                    api,
                ))
            }
            "anthropic" => {
                let mut config = AnthropicConfig::new(key);
                if let Some(url) = base_url {
                    config = config.with_base_url(url);
                }
                Ok(Self {
                    name: "Anthropic",
                    backend: Backend::Anthropic(config.client()),
                })
            }
            "open_router" | "openrouter" => Ok(shaped(
                "OpenRouter",
                with_url(OpenAIConfig::with_key(&OPENROUTER, key)),
                OpenAIApi::Flagship,
            )),
            "gemini" => {
                let mut config = GeminiConfig::new(key);
                if let Some(url) = base_url {
                    config = config.with_base_url(url);
                }
                Ok(Self {
                    name: "Gemini",
                    backend: Backend::Gemini(config.client()),
                })
            }
            "ollama" => {
                let mut config = OllamaConfig::new();
                if let Some(url) = base_url {
                    config = config.with_base_url(url);
                }
                Ok(Self {
                    name: "Ollama",
                    backend: Backend::Ollama(config.client()),
                })
            }
            "cohere" => Ok(Self {
                name: "Cohere",
                backend: Backend::Cohere(CohereConfig::new(key).client()),
            }),
            "deepseek" | "deep_seek" => Ok(shaped(
                "DeepSeek",
                OpenAIConfig::with_key(&DEEPSEEK, key),
                OpenAIApi::Flagship,
            )),
            "groq" => Ok(shaped(
                "Groq",
                OpenAIConfig::with_key(&GROQ, key),
                OpenAIApi::Flagship,
            )),
            "perplexity" => Ok(shaped(
                "Perplexity",
                OpenAIConfig::with_key(&PERPLEXITY, key),
                OpenAIApi::Flagship,
            )),
            "xai" => Ok(shaped(
                "xAI",
                OpenAIConfig::with_key(&xai::DIALECT, key),
                OpenAIApi::Flagship,
            )),
            "hugging_face" | "huggingface" => Ok(shaped(
                "HuggingFace",
                OpenAIConfig::with_key(&HUGGINGFACE, key),
                OpenAIApi::Flagship,
            )),
            "hyperbolic" => Ok(shaped(
                "Hyperbolic",
                OpenAIConfig::with_key(&HYPERBOLIC, key),
                OpenAIApi::Flagship,
            )),
            "moonshot" => Ok(shaped(
                "Moonshot",
                OpenAIConfig::with_key(&MOONSHOT, key),
                OpenAIApi::Flagship,
            )),
            "together" => Ok(shaped(
                "Together",
                OpenAIConfig::with_key(&TOGETHER, key),
                OpenAIApi::Flagship,
            )),
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
        self.name
    }

    /// The chat model `model_id` on this provider.
    fn completion_model(&self, model_id: &str) -> DynModel<Completion> {
        match &self.backend {
            Backend::OpenAI(client, OpenAIApi::Responses) => client.responses(model_id).erase(),
            Backend::OpenAI(client, OpenAIApi::Chat) => client.chat(model_id).erase(),
            Backend::OpenAI(client, OpenAIApi::Flagship) => client.completion(model_id).erase(),
            Backend::Anthropic(client) => client.completion(model_id).erase(),
            Backend::Gemini(client) => client.completion(model_id).erase(),
            Backend::Ollama(client) => client.native_completion(model_id).erase(),
            Backend::Cohere(client) => client.completion(model_id).erase(),
        }
    }

    /// The embedding model `model_id`, for the providers that serve one.
    fn embedding_model(&self, model_id: &str) -> Option<DynModel<Embedding>> {
        match (&self.backend, self.name) {
            (Backend::OpenAI(client, _), "OpenAI" | "OpenRouter" | "Together") => {
                Some(client.embedding(model_id, None).erase())
            }
            (Backend::Ollama(client), _) => Some(client.embedding(model_id, None).erase()),
            (Backend::Gemini(client), _) => Some(client.embedding(model_id, None).erase()),
            (Backend::Cohere(client), _) => Some(client.embedding(model_id, None).erase()),
            _ => None,
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
        let request = build_request(messages, images, params)?;

        debug!(
            "[RigProvider::generate_stream] provider={}, model={}, msg_count={}",
            self.name(),
            model_id,
            request.chat_history.len()
        );

        let mut stream = match self.completion_model(model_id).stream(request) {
            Ok(stream) => stream,
            Err(e) => {
                error!("[RigProvider] could not start the stream: {e}");
                let _ = tx.send(StreamChunk::Error(format!("{e}"))).await;
                return Ok(());
            }
        };

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

        loop {
            let item = match timeout(STREAM_EVENT_TIMEOUT, stream.next()).await {
                Ok(Some(item)) => item,
                Ok(None) => break, // stream ended normally
                Err(_) => {
                    error!(
                        "[RigProvider] stream timed out — no event from provider for {}s",
                        STREAM_EVENT_TIMEOUT.as_secs()
                    );
                    let _ = tx
                        .send(StreamChunk::Error(format!(
                            "The provider stopped responding (no reply for {}s). It may be overloaded — try again or switch models.",
                            STREAM_EVENT_TIMEOUT.as_secs()
                        )))
                        .await;
                    return Ok(());
                }
            };
            match item {
                Ok(Item::Event(StreamEvent::Text { text, .. })) => {
                    full_text.push_str(&text);
                    if tx.send(StreamChunk::Delta(text)).await.is_err() {
                        debug!("[RigProvider] channel closed, stopping stream");
                        return Ok(());
                    }
                }
                Ok(Item::Event(StreamEvent::Reasoning { text, .. })) => {
                    if !text.is_empty() {
                        reasoning_text.push_str(&text);
                        if tx.send(StreamChunk::ReasoningDelta(text)).await.is_err() {
                            debug!("[RigProvider] channel closed, stopping stream");
                            return Ok(());
                        }
                    }
                }
                Ok(_) => {
                    // Part starts/ends, tool-call arguments, unmodelled payloads — skip.
                }
                Err(e) => {
                    error!("[RigProvider] stream error: {e}");
                    let _ = tx.send(StreamChunk::Error(format!("{e}"))).await;
                    return Ok(());
                }
            }
        }

        // The stream is over: report what arrived. The reasoning narration is
        // only a fallback for providers that put their whole answer there.
        let final_text = if !full_text.is_empty() {
            full_text
        } else if !reasoning_text.trim().is_empty() {
            reasoning_text
        } else {
            error!("[RigProvider] stream ended with no text received");
            let _ = tx.send(StreamChunk::Error(
                "Received empty response from the provider. Please verify your API key and model settings.".to_string(),
            ))
            .await;
            return Ok(());
        };
        let _ = tx.send(StreamChunk::Done(final_text)).await;
        Ok(())
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
        let request = build_request(messages, images, params)?;

        debug!(
            "[RigProvider::generate] provider={}, model={}, msg_count={}",
            self.name(),
            model_id,
            request.chat_history.len()
        );

        let response = self
            .completion_model(model_id)
            .call(request)
            .await
            .map_err(|e| MythicError::Provider(format!("{e}")))?
            .text();

        if response.trim().is_empty() {
            return Err(MythicError::Provider("Received empty response from the provider. Please verify your API key and model settings.".to_string()));
        }

        Ok(response)
    }

    /// Generate embeddings for a batch of texts.
    /// Works across the providers that serve an embedding model.
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

        let model = self.embedding_model(model_id).ok_or_else(|| {
            MythicError::Config(format!(
                "Provider '{}' does not support embeddings. Use OpenAI, OpenRouter, Ollama, Gemini, Cohere, or Together.",
                self.name()
            ))
        })?;

        let response = model
            .call(texts)
            .await
            .map_err(|e| MythicError::Provider(format!("Embedding error: {e}")))?;
        Ok(response.embeddings.into_iter().map(|e| e.vec).collect())
    }
}

// ── Helper functions ───────────────────────────────────────────────

/// Builds the rig request for a conversation: system messages become the
/// preamble, the last message is the prompt, everything before it is history.
fn build_request(
    messages: &[ChatMessage],
    images: &[(Vec<u8>, String)],
    params: &GenerationParams,
) -> Result<CompletionRequest, MythicError> {
    let mut history = convert_messages(messages, images);
    let prompt = history
        .pop()
        .ok_or_else(|| MythicError::Provider("There is nothing to send to the model.".into()))?;

    let mut request = CompletionRequest::new(prompt)
        .messages(history)
        .temperature(f64::from(params.temperature));
    if let Some(preamble) = extract_system_preamble(messages) {
        request = request.preamble(preamble);
    }
    Ok(request)
}

/// Converts Mythic's `ChatMessage` array to rig's `Message` array.
/// Filters out system messages (those become the request's preamble).
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
                let content = std::iter::once(UserContent::text(&m.content))
                    .chain(images.iter().map(|(bytes, mime)| {
                        UserContent::image_raw(bytes.clone(), image_media_type(mime), None)
                    }))
                    .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_msg(role: MessageRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
        }
    }

    /// The text of a rig message's first text block.
    fn text_of(msg: &Message) -> String {
        match msg {
            Message::User { content } => match content.first() {
                Some(UserContent::Text(t)) => t.text.clone(),
                _ => String::new(),
            },
            Message::Assistant(turn) => turn.text(),
            Message::System { content } => content.clone(),
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
        assert_eq!(text_of(&rig_messages[0]), "What's the weather?");
        assert_eq!(text_of(&rig_messages[1]), "It's sunny.");
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

    // ── build_request ───────────────────────────────────────────────

    fn params() -> GenerationParams {
        GenerationParams {
            temperature: 0.7,
            ..Default::default()
        }
    }

    #[test]
    fn build_request_puts_preamble_first_and_last_message_as_prompt() {
        let messages = vec![
            chat_msg(MessageRole::System, "You are Aria."),
            chat_msg(MessageRole::User, "first"),
            chat_msg(MessageRole::Assistant, "second"),
            chat_msg(MessageRole::User, "third"),
        ];
        let request = build_request(&messages, &[], &params()).unwrap();

        assert_eq!(request.system_instructions(), Some("You are Aria."));
        // preamble + user + assistant + the prompt
        assert_eq!(request.chat_history.len(), 4);
        assert_eq!(text_of(request.chat_history.last().unwrap()), "third");
        assert_eq!(request.temperature, Some(f64::from(0.7_f32)));
    }

    #[test]
    fn build_request_keeps_images_on_the_prompt() {
        let messages = vec![chat_msg(MessageRole::User, "look at this")];
        let images = vec![(vec![1u8, 2, 3], "image/png".to_string())];
        let request = build_request(&messages, &images, &params()).unwrap();

        match request.chat_history.last().unwrap().as_ref() {
            Message::User { content } => assert_eq!(content.len(), 2),
            other => panic!("expected User message, got {other:?}"),
        }
    }

    #[test]
    fn build_request_rejects_a_conversation_with_no_messages() {
        let messages = vec![chat_msg(MessageRole::System, "You are Aria.")];
        assert!(build_request(&messages, &[], &params()).is_err());
    }

    // ── against a local OpenAI-compatible server ────────────────────

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves one canned HTTP response on a local port and hands back the
    /// request it received (request line + headers + body) when done.
    async fn serve_once(
        content_type: &'static str,
        body: String,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                received.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&received).to_string();
                if let Some(split) = text.find("\r\n\r\n") {
                    let length = text[..split]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if received.len() >= split + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
            socket.shutdown().await.ok();
            String::from_utf8_lossy(&received).to_string()
        });
        (base_url, handle)
    }

    fn sse_chunk(delta: &str, finish: Option<&str>) -> String {
        let finish = finish.map_or("null".to_string(), |f| format!("\"{f}\""));
        format!(
            "data: {{\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"m\",\"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish}}}]}}

"
        )
    }

    #[tokio::test]
    async fn openai_compatible_stream_delivers_deltas_then_done() {
        let body = format!(
            "{}{}{}data: [DONE]

",
            sse_chunk(r#"{"role":"assistant","content":"Hel"}"#, None),
            sse_chunk(r#"{"content":"lo"}"#, None),
            sse_chunk("{}", Some("stop")),
        );
        let (base_url, server) = serve_once("text/event-stream", body).await;
        let provider =
            RigProvider::from_config("openai_compatible", Some("test-key"), Some(&base_url))
                .unwrap();

        let messages = vec![
            chat_msg(MessageRole::System, "You are Aria."),
            chat_msg(MessageRole::User, "Say hello"),
        ];
        let (tx, mut rx) = mpsc::channel(16);
        provider
            .generate_stream("some-model", &messages, &[], &params(), tx)
            .await
            .unwrap();

        let mut deltas = String::new();
        let mut done = None;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(d) => deltas.push_str(&d),
                StreamChunk::Done(full) => done = Some(full),
                StreamChunk::Error(e) => panic!("unexpected error chunk: {e}"),
                _ => {}
            }
        }
        assert_eq!(deltas, "Hello");
        assert_eq!(done.as_deref(), Some("Hello"));

        let request = server.await.unwrap();
        assert!(
            request.starts_with("POST /v1/chat/completions"),
            "{request}"
        );
        assert!(
            request.to_ascii_lowercase().contains("bearer test-key"),
            "{request}"
        );
        let request_body = request.split("\r\n\r\n").nth(1).unwrap();
        let json: serde_json::Value = serde_json::from_str(request_body).unwrap();
        assert_eq!(json["model"], "some-model");
        assert_eq!(json["stream"], true);
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["content"], "Say hello");
    }

    #[tokio::test]
    async fn openrouter_reasoning_is_kept_apart_from_the_reply() {
        let body = format!(
            "{}{}{}{}data: [DONE]

",
            sse_chunk(
                r#"{"role":"assistant","reasoning":"The user greets me. "}"#,
                None
            ),
            sse_chunk(r#"{"reasoning":"I should answer."}"#, None),
            sse_chunk(r#"{"content":"Hi there!"}"#, None),
            sse_chunk("{}", Some("stop")),
        );
        let (base_url, _server) = serve_once("text/event-stream", body).await;
        let provider = RigProvider {
            name: "OpenRouter",
            backend: Backend::OpenAI(
                Box::new(
                    OpenAIConfig::with_key(&OPENROUTER, "k")
                        .with_base_url(&base_url)
                        .client(),
                ),
                OpenAIApi::Flagship,
            ),
        };

        let messages = vec![chat_msg(MessageRole::User, "hello")];
        let (tx, mut rx) = mpsc::channel(16);
        provider
            .generate_stream("some/model", &messages, &[], &params(), tx)
            .await
            .unwrap();

        let (mut reply, mut reasoning, mut done) = (String::new(), String::new(), None);
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(d) => reply.push_str(&d),
                StreamChunk::ReasoningDelta(r) => reasoning.push_str(&r),
                StreamChunk::Done(full) => done = Some(full),
                StreamChunk::Error(e) => panic!("unexpected error chunk: {e}"),
            }
        }
        assert_eq!(reply, "Hi there!");
        assert_eq!(reasoning, "The user greets me. I should answer.");
        // The stored reply never contains the narration.
        assert_eq!(done.as_deref(), Some("Hi there!"));
    }

    #[tokio::test]
    async fn ollama_native_stream_delivers_deltas_then_done() {
        let line = |content: &str, done: bool| {
            let reason = if done { r#","done_reason":"stop""# } else { "" };
            format!(
                r#"{{"model":"llama","created_at":"2026-01-01T00:00:00Z","message":{{"role":"assistant","content":"{content}"}},"done":{done}{reason}}}"#
            )
        };
        let body = [line("Hel", false), line("lo", false), line("", true)].join(
            "
",
        ) + "
";
        let (base_url, server) = serve_once("application/x-ndjson", body).await;
        // Ollama's address has no /v1; the server URL above does, so strip it.
        let provider =
            RigProvider::from_config("ollama", None, Some(base_url.trim_end_matches("/v1")))
                .unwrap();

        let messages = vec![chat_msg(MessageRole::User, "hello")];
        let (tx, mut rx) = mpsc::channel(16);
        provider
            .generate_stream("llama", &messages, &[], &params(), tx)
            .await
            .unwrap();

        let (mut reply, mut done) = (String::new(), None);
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(d) => reply.push_str(&d),
                StreamChunk::Done(full) => done = Some(full),
                StreamChunk::Error(e) => panic!("unexpected error chunk: {e}"),
                _ => {}
            }
        }
        assert_eq!(reply, "Hello");
        assert_eq!(done.as_deref(), Some("Hello"));
        assert!(server.await.unwrap().starts_with("POST /api/chat"));
    }

    /// One Responses API server-sent event.
    fn responses_event(kind: &str, fields: &str) -> String {
        format!(
            "event: {kind}
data: {{\"type\":\"{kind}\",\"sequence_number\":0,{fields}}}

"
        )
    }

    #[tokio::test]
    async fn openai_responses_stream_delivers_deltas_then_done() {
        let message = r#"{"id":"msg_1","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hello","annotations":[]}]}"#;
        let response = |status: &str, output: &str| {
            format!(
                r#""response":{{"id":"resp_1","object":"response","created_at":0,"status":"{status}","model":"gpt-test","output":{output},"usage":{{"input_tokens":3,"output_tokens":2,"total_tokens":5}}}}"#
            )
        };
        let body = [
            responses_event("response.created", &response("in_progress", "[]")),
            responses_event(
                "response.output_item.added",
                r#""output_index":0,"item":{"id":"msg_1","type":"message","status":"in_progress","role":"assistant","content":[]}"#,
            ),
            responses_event(
                "response.content_part.added",
                r#""item_id":"msg_1","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}"#,
            ),
            responses_event(
                "response.output_text.delta",
                r#""item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel""#,
            ),
            responses_event(
                "response.output_text.delta",
                r#""item_id":"msg_1","output_index":0,"content_index":0,"delta":"lo""#,
            ),
            responses_event(
                "response.output_text.done",
                r#""item_id":"msg_1","output_index":0,"content_index":0,"text":"Hello""#,
            ),
            responses_event(
                "response.content_part.done",
                r#""item_id":"msg_1","output_index":0,"content_index":0,"part":{"type":"output_text","text":"Hello","annotations":[]}"#,
            ),
            responses_event(
                "response.output_item.done",
                &format!(r#""output_index":0,"item":{message}"#),
            ),
            responses_event(
                "response.completed",
                &response("completed", &format!("[{message}]")),
            ),
        ]
        .concat();
        let (base_url, server) = serve_once("text/event-stream", body).await;
        let provider = RigProvider {
            name: "OpenAI",
            backend: Backend::OpenAI(
                Box::new(
                    OpenAIConfig::with_key(&OPENAI, "test-key")
                        .with_base_url(&base_url)
                        .client(),
                ),
                OpenAIApi::Responses,
            ),
        };

        let messages = vec![
            chat_msg(MessageRole::System, "You are Aria."),
            chat_msg(MessageRole::User, "Say hello"),
        ];
        let (tx, mut rx) = mpsc::channel(16);
        provider
            .generate_stream("gpt-test", &messages, &[], &params(), tx)
            .await
            .unwrap();

        let mut deltas = String::new();
        let mut done = None;
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(d) => deltas.push_str(&d),
                StreamChunk::Done(full) => done = Some(full),
                StreamChunk::Error(e) => panic!("unexpected error chunk: {e}"),
                _ => {}
            }
        }
        assert_eq!(deltas, "Hello");
        assert_eq!(done.as_deref(), Some("Hello"));

        let request = server.await.unwrap();
        assert!(request.starts_with("POST /v1/responses"), "{request}");
        let request_body = request
            .split(
                "\r
\r
",
            )
            .nth(1)
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(request_body).unwrap();
        assert_eq!(json["model"], "gpt-test");
        assert_eq!(json["stream"], true);
    }

    #[tokio::test]
    async fn openai_compatible_generate_returns_the_reply_text() {
        let body = r#"{"id":"c1","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"All good."},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#.to_string();
        let (base_url, server) = serve_once("application/json", body).await;
        let provider = RigProvider::from_config("lm_studio", Some("k"), Some(&base_url)).unwrap();

        let messages = vec![chat_msg(MessageRole::User, "How are you?")];
        let reply = provider
            .generate("some-model", &messages, &[], &params())
            .await
            .unwrap();
        assert_eq!(reply, "All good.");
        assert!(server
            .await
            .unwrap()
            .starts_with("POST /v1/chat/completions"));
    }

    #[tokio::test]
    async fn openai_compatible_embeddings_return_one_vector_per_text() {
        let body = r#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.5,0.25]},{"object":"embedding","index":1,"embedding":[1.0,2.0]}],"model":"m","usage":{"prompt_tokens":2,"total_tokens":2}}"#.to_string();
        let (base_url, server) = serve_once("application/json", body).await;
        let provider = RigProvider::from_config("openai", Some("k"), Some(&base_url)).unwrap();

        let vectors = provider
            .generate_embedding("embed-model", vec!["a".into(), "b".into()])
            .await
            .unwrap();
        assert_eq!(vectors, vec![vec![0.5, 0.25], vec![1.0, 2.0]]);
        assert!(server.await.unwrap().starts_with("POST /v1/embeddings"));
    }

    #[tokio::test]
    async fn providers_without_embeddings_say_so() {
        let provider = RigProvider::from_config("groq", Some("k"), None).unwrap();
        let err = provider
            .generate_embedding("m", vec!["a".into()])
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("does not support embeddings"));
    }

    #[test]
    fn from_config_rejects_unknown_adapters() {
        assert!(RigProvider::from_config("not-a-provider", None, None).is_err());
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
