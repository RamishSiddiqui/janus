# Rig Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Mythic's hand-rolled LLM provider layer with rig-core, getting 17+ providers with streaming support through a single unified interface.

**Architecture:** A `RigProvider` enum wraps rig-core's native provider clients. A `create_rig_client()` factory resolves DB config → rig client at runtime. Streaming uses rig's native `stream_chat()` → our existing mpsc channel → Tauri events. We do NOT use `rig-dyn` because it doesn't support streaming.

**Tech Stack:** `rig-core 0.37` (with `derive` + `image` features), Rust, Tauri 2, SQLite

---

### Task 1: Add rig-core dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add rig-core to dependencies**

In `src-tauri/Cargo.toml`, add rig-core and remove eventsource-stream:

```toml
# Under [dependencies], ADD:
# LLM Framework (unified provider abstraction)
rig-core = { version = "0.37", features = ["derive", "image"] }

# REMOVE this line:
# eventsource-stream = "0.2"
```

- [ ] **Step 2: Run cargo check to verify dependency resolves**

Run: `cd src-tauri && cargo check`
Expected: Compiles with warnings but no errors. rig-core pulls in its own reqwest and eventsource-stream internally.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add rig-core dependency, remove eventsource-stream"
```

---

### Task 2: Create the unified provider module

**Files:**
- Create: `src-tauri/src/providers/unified.rs`
- Modify: `src-tauri/src/providers/mod.rs`

- [ ] **Step 1: Create `providers/unified.rs` with RigProvider enum and factory**

This is the core of the integration. We build our own dynamic dispatch enum (instead of rig-dyn) because we need streaming support.

```rust
//! Unified LLM provider backed by rig-core.
//!
//! Resolves any supported provider from DB config at runtime.
//! Supports streaming via rig's native `stream_chat()`.

use futures::StreamExt;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface,
    hyperbolic, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};
use rig::client::{CompletionClient, Nothing};
use rig::completion::{CompletionRequest, CompletionModel as RigCompletionModel};
use rig::message::Message;
use rig::streaming::{StreamingChat, StreamedAssistantContent};
use rig::OneOrMany;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::traits::StreamChunk;

/// All rig-supported provider clients wrapped in a single enum.
/// Each variant holds the native rig client for that provider.
#[derive(Clone)]
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
            "openai" | "openai_compatible" | "open_ai_compatible" => {
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
                let builder = anthropic::Client::builder().api_key(key);
                let client = if let Some(url) = base_url {
                    builder.base_url(url).build()
                } else {
                    builder.build()
                }.map_err(|e| MythicError::Config(format!("Anthropic client error: {e}")))?;
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
            "deepseek" => {
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
                 gemini, ollama, cohere, deepseek, groq, perplexity, xai, huggingface, hyperbolic, \
                 moonshot, together",
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
    ///
    /// This is a macro-generated dispatch that calls the correct rig provider's
    /// streaming method based on the enum variant.
    pub async fn generate_stream(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        let rig_messages = convert_messages(messages);
        let preamble = extract_system_preamble(messages);

        // Use macro to dispatch to the correct provider
        macro_rules! stream_with {
            ($client:expr) => {{
                let mut agent_builder = $client.agent(model_id);
                if let Some(ref pre) = preamble {
                    agent_builder = agent_builder.preamble(pre);
                }
                if let Some(temp) = params.temperature {
                    agent_builder = agent_builder.temperature(temp);
                }
                let agent = agent_builder.build();

                // Get the user's last message as the prompt
                let (prompt, history) = split_prompt_and_history(&rig_messages);

                match agent.stream_chat(&prompt, history).await {
                    Ok(mut stream) => {
                        let mut full_text = String::new();
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(StreamedAssistantContent::Text(text)) => {
                                    full_text.push_str(&text);
                                    if tx.send(StreamChunk::Delta(text)).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(_) => {} // ToolCallDelta, FinalUsage — skip
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(format!("{e}"))).await;
                                    return Ok(());
                                }
                            }
                        }
                        let _ = tx.send(StreamChunk::Done(full_text)).await;
                    }
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(format!("{e}"))).await;
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

    /// Non-streaming chat completion.
    pub async fn generate(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        let rig_messages = convert_messages(messages);
        let preamble = extract_system_preamble(messages);

        macro_rules! complete_with {
            ($client:expr) => {{
                let mut agent_builder = $client.agent(model_id);
                if let Some(ref pre) = preamble {
                    agent_builder = agent_builder.preamble(pre);
                }
                if let Some(temp) = params.temperature {
                    agent_builder = agent_builder.temperature(temp);
                }
                let agent = agent_builder.build();

                let (prompt, history) = split_prompt_and_history(&rig_messages);

                use rig::completion::Chat;
                let response = agent.chat(&prompt, history).await
                    .map_err(|e| MythicError::Generation(format!("{e}")))?;

                // Extract text from response
                let text = response.text();
                Ok(text)
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
}

// ── Helper functions ───────────────────────────────────────────────

/// Converts Mythic's `ChatMessage` array to rig's `Message` array.
/// Filters out system messages (those become the preamble).
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

/// Extracts the combined system prompt from all system-role messages.
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

/// Splits a message list into (last_user_prompt, history).
/// rig's `stream_chat(prompt, history)` expects the current turn's
/// prompt separately from the conversation history.
fn split_prompt_and_history(messages: &[Message]) -> (String, Vec<Message>) {
    if messages.is_empty() {
        return (String::new(), vec![]);
    }

    // The last message should be the user's current prompt
    let last = messages.last().unwrap();
    let prompt = match last {
        Message::Human { content } => {
            content.first().map(|c| match c {
                rig::message::UserContent::Text(t) => t.text.clone(),
                _ => String::new(),
            }).unwrap_or_default()
        }
        _ => String::new(),
    };

    let history = messages[..messages.len() - 1].to_vec();
    (prompt, history)
}
```

- [ ] **Step 2: Update `providers/mod.rs`**

```rust
pub mod traits;
pub mod unified;

// Legacy — will be removed after full migration verification
pub mod openai_client;
pub mod openrouter;
pub mod ollama;
```

- [ ] **Step 3: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: Compiles (unified.rs may have warnings about unused imports until chat.rs wires it up)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/providers/
git commit -m "feat: add unified rig provider with streaming support for 14 providers"
```

---

### Task 3: Wire chat.rs to use RigProvider

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`

- [ ] **Step 1: Update imports in chat.rs**

Replace the old provider imports at the top of `chat.rs`:

```rust
// REMOVE these lines:
// use crate::providers::ollama::OllamaProvider;
// use crate::providers::openai_client::{OpenAiClient, OpenAiClientConfig};
// use crate::providers::openrouter::OpenRouterProvider;
// use crate::providers::traits::LlmProvider;

// ADD this line:
use crate::providers::unified::RigProvider;

// KEEP this line:
use crate::providers::traits::StreamChunk;
```

- [ ] **Step 2: Replace `create_llm_provider()` function**

Delete the entire `create_llm_provider()` function (lines ~662-714) and the `GenericOpenAiProvider` struct (lines ~717-754). Replace with:

```rust
/// Creates a unified LLM provider from DB config.
fn create_rig_provider(config: &ProviderConfig) -> Result<RigProvider, MythicError> {
    let adapter = serde_json::to_value(&config.adapter)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{:?}", config.adapter).to_lowercase());

    let api_key = config.config.get("api_key").and_then(|v| v.as_str());
    let base_url = config.config.get("base_url").and_then(|v| v.as_str());

    RigProvider::from_config(&adapter, api_key, base_url)
}
```

- [ ] **Step 3: Update all call sites of `create_llm_provider`**

There are 3 call sites in chat.rs. For each one:

```rust
// BEFORE:
let provider = create_llm_provider(&provider_config, http)?;

// AFTER (http param no longer needed — rig manages its own HTTP client):
let provider = create_rig_provider(&provider_config)?;
```

The streaming call site changes from:
```rust
// BEFORE:
provider.generate_stream(&model_id, &stream_messages, &gen_params, tx).await

// AFTER (same signature!):
provider.generate_stream(&model_id, &stream_messages, &gen_params, tx).await
```

The non-streaming call site changes from:
```rust
// BEFORE:
provider.generate(&model_id, &messages, &gen_params).await

// AFTER (same signature!):
provider.generate(&model_id, &messages, &gen_params).await
```

- [ ] **Step 4: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: Compiles. Old provider files may show dead code warnings — that's expected since we haven't deleted them yet.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/chat.rs
git commit -m "feat: wire chat commands to RigProvider, drop reqwest dependency for LLM"
```

---

### Task 4: Update provider management commands

**Files:**
- Modify: `src-tauri/src/commands/providers.rs`
- Modify: `src-tauri/src/models/provider.rs`

- [ ] **Step 1: Expand ProviderAdapter enum**

In `src-tauri/src/models/provider.rs`, add new variants for all rig-supported providers:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdapter {
    // Existing (backward compatible with DB)
    Ollama,
    OpenRouter,
    OpenAiCompatible,
    SiliconFlow,
    HuggingFace,
    ComfyUi,
    // New rig-native providers
    OpenAi,
    Anthropic,
    Gemini,
    Cohere,
    DeepSeek,
    Groq,
    Perplexity,
    Xai,
    Hyperbolic,
    Moonshot,
    Together,
}
```

- [ ] **Step 2: Update `parse_adapter()` in providers.rs**

Add the new adapter strings to the match:

```rust
fn parse_adapter(s: &str) -> Result<ProviderAdapter, MythicError> {
    match s {
        "ollama" => Ok(ProviderAdapter::Ollama),
        "open_router" | "openrouter" => Ok(ProviderAdapter::OpenRouter),
        "openai_compatible" | "open_ai_compatible" => Ok(ProviderAdapter::OpenAiCompatible),
        "silicon_flow" => Ok(ProviderAdapter::SiliconFlow),
        "hugging_face" | "huggingface" => Ok(ProviderAdapter::HuggingFace),
        "comfy_ui" => Ok(ProviderAdapter::ComfyUi),
        "openai" | "open_ai" => Ok(ProviderAdapter::OpenAi),
        "anthropic" => Ok(ProviderAdapter::Anthropic),
        "gemini" => Ok(ProviderAdapter::Gemini),
        "cohere" => Ok(ProviderAdapter::Cohere),
        "deepseek" | "deep_seek" => Ok(ProviderAdapter::DeepSeek),
        "groq" => Ok(ProviderAdapter::Groq),
        "perplexity" => Ok(ProviderAdapter::Perplexity),
        "xai" => Ok(ProviderAdapter::Xai),
        "hyperbolic" => Ok(ProviderAdapter::Hyperbolic),
        "moonshot" => Ok(ProviderAdapter::Moonshot),
        "together" => Ok(ProviderAdapter::Together),
        _ => Err(MythicError::Config(format!("Unknown adapter: {}", s))),
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: Compiles with possible warnings about unused enum variants.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/models/provider.rs src-tauri/src/commands/providers.rs
git commit -m "feat: expand ProviderAdapter enum for all rig-supported providers"
```

---

### Task 5: Clean up legacy provider files

**Files:**
- Delete: `src-tauri/src/providers/openai_client.rs`
- Delete: `src-tauri/src/providers/openrouter.rs`
- Delete: `src-tauri/src/providers/ollama.rs`
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/providers/traits.rs`

- [ ] **Step 1: Remove `LlmProvider` trait from traits.rs**

In `src-tauri/src/providers/traits.rs`, keep only `StreamChunk` and `ImageProvider`/`ImageResult`. Remove the `LlmProvider` trait (lines 23-63).

- [ ] **Step 2: Update mod.rs to remove old modules**

```rust
pub mod traits;
pub mod unified;
```

- [ ] **Step 3: Delete the legacy provider files**

```bash
cd src-tauri
Remove-Item src/providers/openai_client.rs
Remove-Item src/providers/openrouter.rs
Remove-Item src/providers/ollama.rs
```

- [ ] **Step 4: Fix any remaining references**

Search for any remaining imports of the deleted modules and remove them:

Run: `cd src-tauri && grep -rn "openai_client\|openrouter\|ollama" src/ --include="*.rs"`

Fix all hits.

- [ ] **Step 5: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: Clean compile, no errors.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: delete 518 lines of legacy provider code, replaced by rig unified layer"
```

---

### Task 6: End-to-end verification

**Files:** None (testing only)

- [ ] **Step 1: Run full build**

Run: `cd src-tauri && cargo build`
Expected: Full build succeeds.

- [ ] **Step 2: Run the app**

Run: `npx tauri dev` from project root
Expected: App launches, no errors in terminal.

- [ ] **Step 3: Test streaming chat**

1. Open a conversation with Aria
2. Send a message
3. Verify tokens stream in real-time
4. Verify the full response saves to DB

Expected: Same behavior as before migration.

- [ ] **Step 4: Test follow-up message**

Send a second message in the same conversation.
Expected: Response streams correctly without hanging.

- [ ] **Step 5: Test error handling**

Set an invalid API key in settings, send a message.
Expected: Error state appears, retry button shows.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "test: verify rig integration end-to-end streaming"
```
