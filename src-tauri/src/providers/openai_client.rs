//! Shared OpenAI-compatible HTTP client used by all LLM providers.
//!
//! OpenRouter, Ollama, and generic OpenAI-compatible APIs all share the same
//! `/v1/chat/completions` request format. This module provides a single
//! implementation that each provider configures with its own base URL,
//! auth headers, and model transformations.

use futures::StreamExt;
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::models::provider::ModelInfo;
use crate::providers::traits::StreamChunk;

/// Configuration for a specific OpenAI-compatible endpoint.
#[derive(Debug, Clone)]
pub struct OpenAiClientConfig {
    /// Full base URL (e.g., "https://openrouter.ai/api/v1", "http://localhost:11434/v1")
    pub base_url: String,

    /// Additional headers (auth, custom headers like HTTP-Referer for OpenRouter)
    pub headers: HeaderMap,

    /// Optional model name override (some providers require specific formatting)
    pub default_model: Option<String>,
}

/// Shared OpenAI-compatible API client.
pub struct OpenAiClient {
    http: reqwest::Client,
    config: OpenAiClientConfig,
}

impl OpenAiClient {
    pub fn new(http: reqwest::Client, config: OpenAiClientConfig) -> Self {
        Self { http, config }
    }

    /// Lists available models from the `/v1/models` endpoint.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, MythicError> {
        let url = format!("{}/models", self.config.base_url);

        let resp = self.http
            .get(&url)
            .headers(self.config.headers.clone())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MythicError::Provider(
                format!("Failed to list models ({}): {}", status, body)
            ));
        }

        let body: ModelsResponse = resp.json().await?;

        Ok(body.data.into_iter().map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id, // Use ID as name — providers don't always have a separate name
            context_length: m.context_length,
            metadata: None,
        }).collect())
    }

    /// Sends a non-streaming chat completion request.
    pub async fn generate(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let body = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages.iter().map(|m| ApiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content.clone(),
            }).collect(),
            max_tokens: Some(params.max_tokens),
            temperature: Some(params.temperature),
            top_p: Some(params.top_p),
            frequency_penalty: if params.frequency_penalty != 0.0 { Some(params.frequency_penalty) } else { None },
            presence_penalty: if params.presence_penalty != 0.0 { Some(params.presence_penalty) } else { None },
            stop: if params.stop.is_empty() { None } else { Some(params.stop.clone()) },
            stream: false,
        };

        debug!("Sending chat completion to {}", url);

        let resp = self.http
            .post(&url)
            .headers(self.config.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(MythicError::Provider(
                format!("Chat completion failed ({}): {}", status, err_body)
            ));
        }

        let result: ChatCompletionResponse = resp.json().await?;

        result.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| MythicError::Provider("No choices in response".to_string()))
    }

    /// Sends a streaming chat completion request, forwarding deltas through the channel.
    pub async fn generate_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let body = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages.iter().map(|m| ApiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content.clone(),
            }).collect(),
            max_tokens: Some(params.max_tokens),
            temperature: Some(params.temperature),
            top_p: Some(params.top_p),
            frequency_penalty: if params.frequency_penalty != 0.0 { Some(params.frequency_penalty) } else { None },
            presence_penalty: if params.presence_penalty != 0.0 { Some(params.presence_penalty) } else { None },
            stop: if params.stop.is_empty() { None } else { Some(params.stop.clone()) },
            stream: true,
        };

        debug!("Starting streaming chat completion to {}", url);

        let resp = self.http
            .post(&url)
            .headers(self.config.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            let _ = tx.send(StreamChunk::Error(
                format!("Chat completion failed ({}): {}", status, err_body)
            )).await;
            return Err(MythicError::Provider(
                format!("Chat completion failed ({})", status)
            ));
        }

        let mut full_content = String::new();
        let mut stream = resp.bytes_stream();

        // Buffer for incomplete SSE lines across chunk boundaries
        let mut line_buffer = String::new();
        // Set when finish_reason is received; we drain remaining buffered
        // lines before sending Done to avoid truncating the last token.
        let mut finished = false;

        'outer: while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    error!("Stream error: {}", e);
                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                    return Err(MythicError::Http(e));
                }
            };

            let text = String::from_utf8_lossy(&chunk);
            line_buffer.push_str(&text);

            // Process complete SSE lines
            while let Some(newline_pos) = line_buffer.find('\n') {
                let line = line_buffer[..newline_pos].trim().to_string();
                line_buffer = line_buffer[newline_pos + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        let _ = tx.send(StreamChunk::Done(full_content.clone())).await;
                        return Ok(());
                    }

                    match serde_json::from_str::<StreamChatCompletionChunk>(data) {
                        Ok(chunk) => {
                            if let Some(choice) = chunk.choices.first() {
                                if let Some(ref delta) = choice.delta.content {
                                    if !delta.is_empty() {
                                        full_content.push_str(delta);
                                        if tx.send(StreamChunk::Delta(delta.clone())).await.is_err() {
                                            // Receiver dropped — generation was cancelled
                                            return Ok(());
                                        }
                                    }
                                }

                                // Some providers send finish_reason on the same chunk as the last
                                // token. Use a flag so we finish after draining — not immediately.
                                if choice.finish_reason.is_some() {
                                    finished = true;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse SSE chunk: {} — data: {}", e, data);
                        }
                    }
                }

                if finished {
                    break 'outer;
                }
            }
        }

        // Drain complete: send Done with everything accumulated
        let _ = tx.send(StreamChunk::Done(full_content)).await;
        Ok(())

    }

    /// Simple health check — tries to reach the models endpoint.
    pub async fn health_check(&self) -> Result<bool, MythicError> {
        let url = format!("{}/models", self.config.base_url);

        let resp = self.http
            .get(&url)
            .headers(self.config.headers.clone())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }
}

// --- API Types ---

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: ApiMessage,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    context_length: Option<u32>,
}

#[derive(Deserialize)]
struct StreamChatCompletionChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}
