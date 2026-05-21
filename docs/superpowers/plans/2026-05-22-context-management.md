# Context Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Mythic's naive "send everything" chat pipeline with a token-budgeted context manager that keeps conversations fast and affordable at any length.

**Architecture:** A 3-phase approach — (1) token-budgeted sliding window that immediately caps context size, (2) rolling summaries that preserve narrative from evicted messages, (3) schema preparation for future vector RAG. The context manager is a standalone Rust module (`context/`) that `build_prompt()` delegates to, keeping chat.rs thin.

**Tech Stack:** Rust, SurrealDB 2.x (embedded), `tiktoken-rs` (cl100k_base tokenizer), Tauri v2 IPC, Svelte 5 (runes) frontend.

---

## File Map

### New Files
| File | Responsibility |
|------|---------------|
| `src-tauri/src/context/mod.rs` | Module root — re-exports |
| `src-tauri/src/context/tokenizer.rs` | Token counting via tiktoken-rs (cl100k_base) |
| `src-tauri/src/context/budget.rs` | Token budget calculator — allocates budget across prompt layers |
| `src-tauri/src/context/window.rs` | Sliding window — trims conversation history to fit budget |
| `src-tauri/src/context/summary.rs` | Rolling summary — generates, stores, retrieves conversation summaries |
| `src-tauri/src/db/summaries.rs` | SummaryRepo — CRUD for conversation_summaries table |
| `src-tauri/src/models/summary.rs` | ConversationSummary model struct |

### Modified Files
| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Add `tiktoken-rs` dependency |
| `src-tauri/src/main.rs` or `lib.rs` | Register `context` module, register new Tauri commands |
| `src-tauri/src/db/mod.rs` | Register `summaries` module |
| `src-tauri/src/db/schema.rs` | Add `conversation_summaries` table definition |
| `src-tauri/src/models/mod.rs` | Register `summary` module |
| `src-tauri/src/commands/chat.rs` | Refactor `build_prompt()` to use context manager |
| `src/lib/stores/chat.ts` | Trigger summary generation after response, pass context settings |
| `src/lib/services/ipc.ts` | Add `get_context_stats` IPC command |
| `src/routes/settings/+page.svelte` | Add Context Management settings section |
| `src/lib/types/index.ts` | Add ContextStats type |

---

## Task 1: Add tiktoken-rs dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add tiktoken-rs to Cargo.toml**

In `src-tauri/Cargo.toml` under `[dependencies]`, add:

```toml
tiktoken-rs = "0.6"
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors related to tiktoken-rs.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(context): add tiktoken-rs dependency for token counting"
```

---

## Task 2: Token counting module

**Files:**
- Create: `src-tauri/src/context/mod.rs`
- Create: `src-tauri/src/context/tokenizer.rs`
- Modify: `src-tauri/src/lib.rs` (register module)

- [ ] **Step 1: Create the context module root**

```rust
// src-tauri/src/context/mod.rs
pub mod tokenizer;
pub mod budget;
pub mod window;
pub mod summary;
```

- [ ] **Step 2: Implement the tokenizer wrapper**

```rust
// src-tauri/src/context/tokenizer.rs
use tiktoken_rs::cl100k_base;
use std::sync::OnceLock;

use crate::providers::traits::ChatMessage;

/// Global tokenizer instance — cl100k_base is a reasonable approximation
/// across providers (GPT-4, Claude, Gemini all tokenize similarly enough
/// for budget purposes). The exact count doesn't need to be perfect;
/// we leave a safety margin in the budget calculator.
static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

fn bpe() -> &'static tiktoken_rs::CoreBPE {
    BPE.get_or_init(|| cl100k_base().expect("Failed to load cl100k_base tokenizer"))
}

/// Count tokens in a single string.
pub fn count_tokens(text: &str) -> usize {
    bpe().encode_ordinary(text).len()
}

/// Count tokens for a single ChatMessage.
/// Includes the overhead of role markers (~4 tokens per message for
/// the <|im_start|>role\n...content...<|im_end|> framing).
pub fn count_message_tokens(message: &ChatMessage) -> usize {
    // Per OpenAI: every message has ~4 tokens of overhead
    // (role name, delimiters). This is a reasonable cross-provider estimate.
    const MESSAGE_OVERHEAD: usize = 4;
    count_tokens(&message.content) + MESSAGE_OVERHEAD
}

/// Count tokens for a slice of messages.
pub fn count_messages_tokens(messages: &[ChatMessage]) -> usize {
    // Base overhead: 3 tokens for the <|im_start|>assistant priming
    const REPLY_PRIMING: usize = 3;
    messages.iter().map(count_message_tokens).sum::<usize>() + REPLY_PRIMING
}
```

- [ ] **Step 3: Register the context module in lib.rs**

In `src-tauri/src/lib.rs`, add `mod context;` alongside the existing module declarations.

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly. The `context` module is registered but `budget.rs`, `window.rs`, `summary.rs` don't exist yet — create them as empty files to satisfy the `mod` declarations:

```rust
// src-tauri/src/context/budget.rs  (placeholder)
// Token budget allocation — implemented in Task 3

// src-tauri/src/context/window.rs  (placeholder)
// Sliding window — implemented in Task 4

// src-tauri/src/context/summary.rs (placeholder)
// Rolling summaries — implemented in Task 7
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/context/ src-tauri/src/lib.rs
git commit -m "feat(context): add token counting module with tiktoken-rs cl100k_base"
```

---

## Task 3: Token budget calculator

**Files:**
- Create: `src-tauri/src/context/budget.rs`

The budget calculator determines how many tokens are available for conversation history after accounting for all fixed prompt layers.

- [ ] **Step 1: Implement the budget calculator**

```rust
// src-tauri/src/context/budget.rs
use crate::context::tokenizer::{count_message_tokens, count_messages_tokens};
use crate::providers::traits::ChatMessage;

/// Configuration for the context window budget.
/// All values are in tokens.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Total context window size in tokens (e.g., 8192, 32768, 131072).
    /// This should match the model's context length.
    pub max_context_tokens: usize,
    /// Tokens reserved for the model's response (max_tokens generation param).
    pub reserved_for_response: usize,
    /// Safety margin as a fraction (0.0–1.0). We use 90% of available space
    /// to account for tokenizer approximation differences across providers.
    pub safety_margin: f64,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_context_tokens: 16384,
            reserved_for_response: 2048,
            safety_margin: 0.90,
        }
    }
}

/// Result of a budget calculation — tells the window manager how many
/// tokens are available for conversation history and summary.
#[derive(Debug, Clone)]
pub struct BudgetAllocation {
    /// Total usable tokens after safety margin and response reservation.
    pub total_usable: usize,
    /// Tokens consumed by fixed layers (system prompt, character, lorebook, memories, emotion, PHI).
    pub fixed_layers_tokens: usize,
    /// Tokens available for conversation history (the sliding window).
    pub history_budget: usize,
    /// Tokens available for the rolling summary (subset of history_budget).
    /// Summaries get up to 20% of history budget; rest goes to verbatim messages.
    pub summary_budget: usize,
    /// Tokens available for verbatim recent messages.
    pub messages_budget: usize,
}

impl ContextBudget {
    /// Calculate how many tokens are available for conversation history,
    /// given the fixed prompt layers that have already been assembled.
    ///
    /// `fixed_layers` — all system messages EXCEPT conversation history
    /// (system prompt, character card, lorebook, memories, emotional state, PHI).
    pub fn allocate(&self, fixed_layers: &[ChatMessage]) -> BudgetAllocation {
        let total_usable = ((self.max_context_tokens - self.reserved_for_response) as f64
            * self.safety_margin) as usize;

        let fixed_layers_tokens = count_messages_tokens(fixed_layers);

        let history_budget = total_usable.saturating_sub(fixed_layers_tokens);

        // Summary gets up to 20% of the history budget.
        // This keeps summaries concise while leaving room for verbatim messages.
        let summary_budget = history_budget / 5;
        let messages_budget = history_budget.saturating_sub(summary_budget);

        BudgetAllocation {
            total_usable,
            fixed_layers_tokens,
            history_budget,
            summary_budget,
            messages_budget,
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/context/budget.rs
git commit -m "feat(context): add token budget calculator with layer-aware allocation"
```

---

## Task 4: Sliding window

**Files:**
- Create: `src-tauri/src/context/window.rs`

The sliding window takes the full message branch and trims it to fit within the token budget, keeping the most recent messages.

- [ ] **Step 1: Implement the sliding window**

```rust
// src-tauri/src/context/window.rs
use crate::context::tokenizer::count_message_tokens;
use crate::providers::traits::ChatMessage;

/// Result of applying the sliding window to a conversation history.
#[derive(Debug)]
pub struct WindowResult {
    /// Messages that fit within the token budget (most recent first in the original order).
    pub included: Vec<ChatMessage>,
    /// Number of messages that were evicted (didn't fit in the window).
    pub evicted_count: usize,
    /// Total tokens consumed by the included messages.
    pub included_tokens: usize,
}

/// Applies a token-budgeted sliding window to the conversation history.
///
/// Walks backwards from the most recent message, including messages until
/// the token budget is exhausted. Returns the messages in chronological
/// order (oldest included first) to maintain conversation flow.
///
/// # Arguments
/// * `chain` — Full conversation history in chronological order (root → leaf).
/// * `token_budget` — Maximum tokens available for conversation messages.
pub fn apply_sliding_window(
    chain: &[ChatMessage],
    token_budget: usize,
) -> WindowResult {
    if chain.is_empty() || token_budget == 0 {
        return WindowResult {
            included: Vec::new(),
            evicted_count: chain.len(),
            included_tokens: 0,
        };
    }

    let mut included_tokens: usize = 0;
    let mut include_from: usize = chain.len(); // index where we start including

    // Walk backwards from the most recent message
    for (i, msg) in chain.iter().enumerate().rev() {
        let msg_tokens = count_message_tokens(msg);

        if included_tokens + msg_tokens > token_budget {
            // This message doesn't fit — stop here.
            // But always include at least the last message (the user's current input).
            if include_from == chain.len() {
                // Haven't included anything yet — force-include the last message
                include_from = chain.len() - 1;
                included_tokens = msg_tokens;
            }
            break;
        }

        included_tokens += msg_tokens;
        include_from = i;
    }

    let evicted_count = include_from;
    let included = chain[include_from..].to_vec();

    WindowResult {
        included,
        evicted_count,
        included_tokens,
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/context/window.rs
git commit -m "feat(context): add token-budgeted sliding window for conversation history"
```

---

## Task 5: Integrate sliding window into build_prompt()

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`

This is the critical integration task. We refactor `build_prompt()` to:
1. Build fixed layers first (system prompt, character, lorebook, memories, emotion, PHI)
2. Calculate the token budget
3. Apply the sliding window to the message chain
4. Return the context stats for observability

- [ ] **Step 1: Add context config parameter to build_prompt and integrate the window**

The key change is in `build_prompt()`. Currently at line 621, it does `prompt.extend(chain)` — sending ALL messages. We replace this with a budgeted window.

Add imports at the top of `chat.rs`:

```rust
use crate::context::budget::ContextBudget;
use crate::context::window::apply_sliding_window;
use crate::context::tokenizer::count_messages_tokens;
```

Modify `build_prompt()` signature to accept context budget:

```rust
async fn build_prompt(
    db: &Surreal<Db>,
    conversation_id: &str,
    up_to_message_id: &str,
    user_system_prompt: Option<&str>,
    post_history_instructions: Option<&str>,
    context_budget: &ContextBudget,
) -> Result<(Vec<ChatMessage>, ContextStats), MythicError> {
```

Add a `ContextStats` struct to `chat.rs` (above `build_prompt`):

```rust
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
```

In `build_prompt()`, replace every `prompt.extend(chain)` occurrence with this pattern:

```rust
// Instead of: prompt.extend(chain);
// Do this:

// Collect all fixed layers (everything in `prompt` so far + PHI)
// PHI needs to be counted but added AFTER history, so we count it separately.
let phi_message = post_history_instructions
    .filter(|s| !s.trim().is_empty())
    .map(|phi| ChatMessage {
        role: MessageRole::System,
        content: phi.trim().to_string(),
    });

let mut fixed_for_budget = prompt.clone();
if let Some(ref phi) = phi_message {
    fixed_for_budget.push(phi.clone());
}

// Calculate the budget
let allocation = context_budget.allocate(&fixed_for_budget);

// Apply sliding window to the conversation chain
let window = apply_sliding_window(&chain, allocation.messages_budget);

info!(
    "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
    allocation.fixed_layers_tokens + window.included_tokens,
    context_budget.max_context_tokens,
    window.included.len(),
    chain.len(),
    window.evicted_count,
);

let stats = ContextStats {
    total_budget: context_budget.max_context_tokens,
    fixed_tokens: allocation.fixed_layers_tokens,
    history_tokens: window.included_tokens,
    summary_tokens: 0,
    total_messages: chain.len(),
    included_messages: window.included.len(),
    evicted_messages: window.evicted_count,
};

// Extend with windowed history (not the full chain)
prompt.extend(window.included);

// Add PHI as the final message
if let Some(phi) = phi_message {
    prompt.push(phi);
}
```

This pattern replaces the 3 separate `prompt.extend(chain)` blocks at lines 621, 654, and 688. The PHI injection at lines 696-704 is moved into this block (remove the original PHI code at the end of `build_prompt()`).

- [ ] **Step 2: Update all callers of build_prompt**

`build_prompt` is called in 3 places: `send_message` (L81), `retry_failed_message` (L265), and `regenerate_message` (via `send_message`).

In `send_message()`, construct the budget from the provider's context_length:

```rust
// After getting the provider config (line 85-86):
let provider_config = get_default_llm_provider(&db).await?;
let model_id = resolve_model_id(model, &provider_config, &db).await?;

// Build context budget from provider/model config
let max_context = provider_config.config
    .get("context_length")
    .and_then(|v| v.as_u64())
    .unwrap_or(16384) as usize;

let context_budget = ContextBudget {
    max_context_tokens: max_context,
    reserved_for_response: gen_params.max_tokens as usize,
    ..Default::default()
};

// Update build_prompt call:
let (messages, context_stats) = build_prompt(
    &db, &conversation_id, &user_msg_id,
    system_prompt.as_deref(),
    post_history_instructions.as_deref(),
    &context_budget,
).await?;
debug!("[send_message] context stats: {:?}", context_stats);
```

Apply the same pattern to `retry_failed_message`.

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly. Warnings about unused `context_stats` fields are OK for now.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/chat.rs
git commit -m "feat(context): integrate sliding window into build_prompt — context now token-budgeted"
```

---

## Task 6: Summary model and database schema

**Files:**
- Create: `src-tauri/src/models/summary.rs`
- Create: `src-tauri/src/db/summaries.rs`
- Modify: `src-tauri/src/db/schema.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/models/mod.rs`

- [ ] **Step 1: Create the ConversationSummary model**

```rust
// src-tauri/src/models/summary.rs
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use super::deserialize_thing_to_string;

/// A rolling summary of evicted conversation messages.
/// Each summary covers a contiguous range of messages that have
/// fallen outside the sliding window. Summaries are cumulative —
/// new summaries incorporate the previous summary + newly evicted messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// SurrealDB record ID (conversation_summaries:uuid).
    #[serde(deserialize_with = "deserialize_thing_to_string")]
    pub id: String,
    /// The conversation this summary belongs to.
    #[serde(deserialize_with = "deserialize_thing_to_string")]
    pub conversation_id: String,
    /// The compressed summary text.
    pub summary_text: String,
    /// Number of messages this summary covers.
    pub covered_message_count: u32,
    /// Token count of the summary text itself.
    pub token_count: u32,
    /// ID of the oldest message NOT covered by this summary.
    /// This is the boundary — messages older than this are summarized,
    /// messages from this ID onward are in the sliding window.
    #[serde(default, deserialize_with = "super::deserialize_option_thing_to_string")]
    pub window_start_message_id: Option<String>,
    /// ISO timestamp of when this summary was generated.
    pub created_at: String,
    /// ISO timestamp of last update.
    pub updated_at: String,
}
```

- [ ] **Step 2: Add the table to the SurrealDB schema**

In `src-tauri/src/db/schema.rs`, add after the existing table definitions:

```rust
// ── Conversation Summaries ──
// Rolling summaries of evicted conversation history for context management.
DEFINE TABLE IF NOT EXISTS conversation_summaries SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS conversation_id ON conversation_summaries TYPE record<conversations>;
DEFINE FIELD IF NOT EXISTS summary_text ON conversation_summaries TYPE string;
DEFINE FIELD IF NOT EXISTS covered_message_count ON conversation_summaries TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS token_count ON conversation_summaries TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS window_start_message_id ON conversation_summaries TYPE option<record<messages>>;
DEFINE FIELD IF NOT EXISTS created_at ON conversation_summaries TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON conversation_summaries TYPE datetime DEFAULT time::now();

// One summary per conversation (upsert pattern)
DEFINE INDEX IF NOT EXISTS idx_summary_conversation ON conversation_summaries FIELDS conversation_id UNIQUE;

// Cascade: delete summary when conversation is deleted
DEFINE EVENT IF NOT EXISTS delete_conversation_summaries ON TABLE conversations WHEN $event = 'DELETE' THEN {
    DELETE FROM conversation_summaries WHERE conversation_id = $before.id;
};
```

- [ ] **Step 3: Create SummaryRepo**

```rust
// src-tauri/src/db/summaries.rs
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::summary::ConversationSummary;

pub struct SummaryRepo;

impl SummaryRepo {
    /// Get the current rolling summary for a conversation.
    /// Returns None if no summary has been generated yet.
    pub async fn get(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Option<ConversationSummary>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM conversation_summaries \
                 WHERE conversation_id = type::thing('conversations', $conv_id) \
                 LIMIT 1"
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;

        let summaries: Vec<ConversationSummary> = result.take(0)?;
        Ok(summaries.into_iter().next())
    }

    /// Create or update (upsert) the rolling summary for a conversation.
    /// Uses the unique index on conversation_id for conflict resolution.
    pub async fn upsert(
        db: &Surreal<Db>,
        conversation_id: &str,
        summary_text: &str,
        covered_message_count: u32,
        token_count: u32,
        window_start_message_id: Option<&str>,
    ) -> Result<(), MythicError> {
        let window_start = window_start_message_id
            .map(|id| format!("type::thing('messages', '{}')", id))
            .unwrap_or_else(|| "NONE".to_string());

        let query = format!(
            "UPSERT conversation_summaries SET \
                conversation_id = type::thing('conversations', $conv_id), \
                summary_text = $text, \
                covered_message_count = $count, \
                token_count = $tokens, \
                window_start_message_id = {window_start}, \
                updated_at = time::now() \
             WHERE conversation_id = type::thing('conversations', $conv_id)"
        );

        db.query(&query)
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("text", summary_text.to_string()))
            .bind(("count", covered_message_count as i64))
            .bind(("tokens", token_count as i64))
            .await?;

        Ok(())
    }

    /// Delete the summary for a conversation (e.g., when conversation is cleared).
    pub async fn delete(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM conversation_summaries \
             WHERE conversation_id = type::thing('conversations', $conv_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .await?;

        Ok(())
    }
}
```

- [ ] **Step 4: Register new modules**

In `src-tauri/src/db/mod.rs`, add: `pub mod summaries;`
In `src-tauri/src/models/mod.rs`, add: `pub mod summary;`

- [ ] **Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/models/summary.rs src-tauri/src/db/summaries.rs src-tauri/src/db/schema.rs src-tauri/src/db/mod.rs src-tauri/src/models/mod.rs
git commit -m "feat(context): add conversation_summaries schema, model, and repo"
```

---

## Task 7: Rolling summary generation

**Files:**
- Modify: `src-tauri/src/context/summary.rs`
- Modify: `src-tauri/src/commands/chat.rs` (add `generate_summary` command)

The rolling summary uses the existing `generate_raw` pattern (stateless LLM call) to compress evicted messages into a narrative summary.

- [ ] **Step 1: Implement the summary generator**

```rust
// src-tauri/src/context/summary.rs
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{info, warn};

use crate::context::tokenizer::count_tokens;
use crate::db::summaries::SummaryRepo;
use crate::error::MythicError;
use crate::providers::traits::{ChatMessage, MessageRole, GenerationParams};
use crate::providers::unified::RigProvider;

/// The system prompt used to generate rolling summaries.
/// Optimized for roleplay context — preserves character dynamics and plot.
const SUMMARY_SYSTEM_PROMPT: &str = r#"You are a narrative summarizer for a roleplay conversation. Your job is to compress conversation history into a concise summary that preserves:

1. **Key plot events** — what happened, in order
2. **Character dynamics** — how relationships evolved, promises made, emotional shifts
3. **Important details** — names, places, objects, gifts that may be referenced later
4. **Tone and atmosphere** — the current mood of the scene

Rules:
- Write in third person past tense
- Be concise but complete — every detail you drop is forgotten forever
- If a previous summary is provided, incorporate it naturally — don't just append
- Focus on WHAT HAPPENED, not on quoting dialogue verbatim
- Keep the summary under 500 words
- Do NOT add commentary or analysis — just summarize the events"#;

/// Generate or update the rolling summary for a conversation.
///
/// This is called as a background task after a chat response completes,
/// when messages have been evicted from the sliding window.
///
/// # Arguments
/// * `db` — Database handle
/// * `provider` — LLM provider for summary generation
/// * `model_id` — Model to use for summarization
/// * `conversation_id` — The conversation to summarize
/// * `evicted_messages` — Messages that fell outside the sliding window
/// * `existing_summary` — The previous summary text (if any)
pub async fn generate_rolling_summary(
    db: &Surreal<Db>,
    provider: &RigProvider,
    model_id: &str,
    conversation_id: &str,
    evicted_messages: &[ChatMessage],
    existing_summary: Option<&str>,
    window_start_message_id: Option<&str>,
) -> Result<(), MythicError> {
    if evicted_messages.is_empty() {
        return Ok(());
    }

    info!(
        "[summary] Generating summary for conversation {} — {} evicted messages",
        conversation_id,
        evicted_messages.len()
    );

    // Build the user prompt for the summarizer
    let mut user_prompt = String::new();

    if let Some(prev) = existing_summary {
        user_prompt.push_str("## Previous Summary\n\n");
        user_prompt.push_str(prev);
        user_prompt.push_str("\n\n---\n\n");
    }

    user_prompt.push_str("## New Messages to Incorporate\n\n");
    for msg in evicted_messages {
        let role_label = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Character",
            MessageRole::System => continue, // Skip system messages from summary
        };
        user_prompt.push_str(&format!("**{}:** {}\n\n", role_label, msg.content));
    }

    user_prompt.push_str("\n---\n\nWrite the updated summary incorporating both the previous summary and the new messages:");

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: SUMMARY_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        max_tokens: 1024,
        temperature: 0.3, // Low temperature for factual summarization
        ..Default::default()
    };

    match provider.generate(model_id, &messages, &gen_params).await {
        Ok(summary_text) => {
            let token_count = count_tokens(&summary_text) as u32;
            let covered_count = evicted_messages.len() as u32
                + existing_summary.map(|_| 0u32).unwrap_or(0); // previous count handled by upsert

            SummaryRepo::upsert(
                db,
                conversation_id,
                &summary_text,
                covered_count,
                token_count,
                window_start_message_id,
            ).await?;

            info!(
                "[summary] Summary generated: {} tokens, {} messages covered",
                token_count, covered_count
            );

            Ok(())
        }
        Err(e) => {
            warn!("[summary] Failed to generate summary: {}. Context will work without it.", e);
            // Non-fatal — the sliding window still works without a summary.
            // The summary will be attempted again on the next eviction.
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/context/summary.rs
git commit -m "feat(context): add rolling summary generator with narrative-preserving prompt"
```

---

## Task 8: Integrate summaries into build_prompt and trigger generation

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`

- [ ] **Step 1: Inject existing summary into the prompt**

In `build_prompt()`, after the fixed layers are assembled and before the sliding window is applied, retrieve and inject any existing summary:

```rust
// After memories and emotional state are added, before applying the window:

// ── Rolling Summary ──
// If a rolling summary exists for this conversation, inject it before
// the conversation history. This gives the LLM compressed context about
// events that happened before the sliding window.
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
```

Update the `ContextStats` to include `summary_tokens`:
```rust
let stats = ContextStats {
    // ... existing fields ...
    summary_tokens,
    // ...
};
```

- [ ] **Step 2: Trigger summary generation after response**

In `send_message()`, after the streaming consumer task completes (inside the `StreamChunk::Done` handler), trigger summary generation if messages were evicted:

```rust
// Inside StreamChunk::Done handler, after saving the response:
// Trigger rolling summary generation if messages were evicted
if context_stats.evicted_messages > 0 {
    let db_for_summary = db_for_save.clone();
    let conv_id_summary = conv_id.clone();
    let evicted_count = context_stats.evicted_messages;

    tokio::spawn(async move {
        // Re-fetch evicted messages for summarization
        // We need the full branch, then take the first `evicted_count` messages
        if let Ok(branch) = MessageRepo::get_branch(&db_for_summary, &assist_id).await {
            if branch.len() > evicted_count {
                let evicted: Vec<ChatMessage> = branch[..evicted_count]
                    .iter()
                    .map(|m| ChatMessage {
                        role: m.role.clone(),
                        content: m.content.clone(),
                    })
                    .collect();

                let existing_summary = SummaryRepo::get(&db_for_summary, &conv_id_summary)
                    .await.ok().flatten();

                // Get provider for summary generation
                if let Ok(provider_config) = get_default_llm_provider(&db_for_summary).await {
                    if let Ok(provider) = create_rig_provider(&provider_config) {
                        let model = provider_config.config
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("default")
                            .to_string();

                        let window_start_id = branch.get(evicted_count)
                            .map(|m| m.id.id.to_raw());

                        let _ = generate_rolling_summary(
                            &db_for_summary,
                            &provider,
                            &model,
                            &conv_id_summary,
                            &evicted,
                            existing_summary.as_ref().map(|s| s.summary_text.as_str()),
                            window_start_id.as_deref(),
                        ).await;
                    }
                }
            }
        }
    });
}
```

Add the needed imports at the top of `chat.rs`:
```rust
use crate::context::summary::generate_rolling_summary;
use crate::context::tokenizer::count_message_tokens;
use crate::db::summaries::SummaryRepo;
```

- [ ] **Step 3: Pass context_stats to the streaming task**

The `context_stats` needs to be available inside the streaming consumer `tokio::spawn`. Clone it before the spawn:

```rust
let context_stats_clone = context_stats.clone();
// ... inside the spawn, use context_stats_clone
```

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/chat.rs
git commit -m "feat(context): integrate rolling summaries — inject existing + trigger generation on eviction"
```

---

## Task 9: Context stats IPC command

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`
- Modify: `src-tauri/src/lib.rs` (register command)
- Modify: `src/lib/services/ipc.ts`
- Modify: `src/lib/types/index.ts`

- [ ] **Step 1: Add get_context_stats Tauri command**

```rust
// In chat.rs, add a new command:

/// Returns context window statistics for a conversation.
/// Used by the frontend to display token usage and context health.
#[tauri::command]
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
```

Register in `lib.rs`:
```rust
// Add to the .invoke_handler(tauri::generate_handler![...]) list:
commands::chat::get_context_stats,
```

- [ ] **Step 2: Add frontend IPC binding**

In `src/lib/services/ipc.ts`, add:

```typescript
export interface ContextStats {
  total_budget: number;
  fixed_tokens: number;
  history_tokens: number;
  summary_tokens: number;
  total_messages: number;
  included_messages: number;
  evicted_messages: number;
}

export async function getContextStats(
  conversationId: string,
  messageId: string,
  systemPrompt?: string,
  postHistoryInstructions?: string,
): Promise<ContextStats> {
  return invoke('get_context_stats', {
    conversationId,
    messageId,
    systemPrompt,
    postHistoryInstructions,
  });
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/chat.rs src-tauri/src/lib.rs src/lib/services/ipc.ts src/lib/types/index.ts
git commit -m "feat(context): add get_context_stats IPC command for frontend observability"
```

---

## Task 10: Frontend — Context settings and token display

**Files:**
- Modify: `src/routes/settings/+page.svelte`
- Modify: `src/routes/+page.svelte` (main chat page)
- Modify: `src/lib/stores/chat.ts`

- [ ] **Step 1: Add Context Management section to Settings**

In `src/routes/settings/+page.svelte`, add a new section after the existing settings:

```svelte
<!-- Context Management Section -->
<div class="settings-group">
  <h3 class="settings-group-title">
    <span class="icon">🧠</span>
    Context Management
  </h3>
  <p class="settings-group-description">
    Control how conversation history is managed to prevent context overflow.
  </p>

  <div class="setting-item">
    <div class="setting-label">
      <span>Context Window Size</span>
      <span class="setting-hint">Maximum tokens for the context window. Match to your model's limit.</span>
    </div>
    <select
      bind:value={$settings.maxContextTokens}
      class="setting-select"
    >
      <option value={4096}>4K tokens (small/fast)</option>
      <option value={8192}>8K tokens</option>
      <option value={16384}>16K tokens (default)</option>
      <option value={32768}>32K tokens</option>
      <option value={65536}>64K tokens</option>
      <option value={131072}>128K tokens (large)</option>
    </select>
  </div>

  <div class="setting-item">
    <div class="setting-label">
      <span>Auto-Summarize</span>
      <span class="setting-hint">Generate rolling summaries of older messages that fall outside the context window.</span>
    </div>
    <label class="toggle">
      <input type="checkbox" bind:checked={$settings.autoSummarize} />
      <span class="toggle-slider"></span>
    </label>
  </div>
</div>
```

Add the new settings to the settings store defaults:
```typescript
// In the settings store initialization:
maxContextTokens: 16384,
autoSummarize: true,
```

- [ ] **Step 2: Update token display in chat page**

In `src/routes/+page.svelte`, replace the crude `content.length / 4` approximation with actual context stats. After `sendMessage` completes, fetch stats:

```svelte
<script>
  import { getContextStats } from '$lib/services/ipc';

  let contextStats = $state(null);

  // Update context stats after sending a message
  async function updateContextStats() {
    if (!$activeConversation?.id || !$activeConversation?.active_message_id) return;
    try {
      contextStats = await getContextStats(
        $activeConversation.id,
        $activeConversation.active_message_id,
        $settings.systemPrompt,
        $settings.postHistoryInstructions,
      );
    } catch (e) {
      // Non-critical — don't break the UI
    }
  }
</script>
```

Update the token count display to show real stats when available:

```svelte
{#if contextStats}
  <span class="token-count" title="Context usage">
    {contextStats.fixed_tokens + contextStats.history_tokens + contextStats.summary_tokens}
    / {contextStats.total_budget} tokens
    ({contextStats.included_messages}/{contextStats.total_messages} msgs)
  </span>
{/if}
```

- [ ] **Step 3: Pass maxContextTokens through IPC**

In `src/lib/stores/chat.ts`, update `sendMessage()` to pass `maxContextTokens` to the backend. Add it to the `send_message` IPC call. The backend's `send_message` command needs a new optional parameter:

```rust
// In send_message command signature, add:
max_context_tokens: Option<usize>,
```

And use it when constructing the `ContextBudget`:
```rust
let max_context = max_context_tokens.unwrap_or_else(|| {
    provider_config.config
        .get("context_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(16384) as usize
});
```

- [ ] **Step 4: Verify frontend compiles**

Run: `npm run check` (from the project root)
Expected: No TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add src/routes/settings/+page.svelte src/routes/+page.svelte src/lib/stores/chat.ts src/lib/services/ipc.ts src-tauri/src/commands/chat.rs
git commit -m "feat(context): add context management settings and real token display"
```

---

## Task 11: Cleanup and production hardening

**Files:**
- Modify: `src-tauri/src/db/mod.rs` (remove debug logging)
- Modify: `src-tauri/src/commands/chat.rs` (add summary debouncing)

- [ ] **Step 1: Add summary generation debouncing**

Summaries should NOT be generated on every turn. Add a check in the summary trigger:

```rust
// Only generate a summary if enough new messages have been evicted
// since the last summary. Prevents excessive LLM calls.
const SUMMARY_BATCH_SIZE: usize = 10;

if context_stats.evicted_messages > 0 {
    let existing = SummaryRepo::get(&db, &conversation_id).await.ok().flatten();
    let previously_covered = existing.as_ref().map(|s| s.covered_message_count).unwrap_or(0);
    let new_evictions = context_stats.evicted_messages as u32 - previously_covered;

    if new_evictions >= SUMMARY_BATCH_SIZE as u32 || existing.is_none() {
        // Trigger summary generation...
    }
}
```

- [ ] **Step 2: Remove debug logging from db/mod.rs**

Remove the debug `info!("Debugging conversation list...")` block around lines 35-50.

- [ ] **Step 3: Delete stale SQLite files**

```bash
rm src-tauri/mythic.db
rm src-tauri/seed_memories.sql
```

- [ ] **Step 4: Full build verification**

Run:
```bash
cd src-tauri && cargo check
cd .. && npm run check
```
Expected: Both compile cleanly.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore(context): cleanup debug logging, add summary debouncing, remove stale SQLite files"
```

---

## Verification Plan

### Automated Checks
- `cargo check` — no compilation errors
- `npm run check` — no TypeScript errors

### Manual Verification
1. **Short conversation (< window):** Send 5 messages. All should appear in context. No summary generated. Token count should show all messages included.
2. **Long conversation (> window):** Send 30+ messages with a small context window (4K). Verify:
   - Response time stays constant (doesn't grow with conversation length)
   - Token count shows evicted messages
   - After ~10 evicted messages, a summary is generated
   - The LLM still references events from the beginning (via summary)
3. **Settings:** Change context window size in Settings. Verify it takes effect on next message.
4. **Edge cases:** New conversation (no history), branched conversation, retry failed message.
