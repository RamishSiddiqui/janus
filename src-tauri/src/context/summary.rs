use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{info, warn};

use crate::context::tokenizer::count_tokens;
use crate::db::summaries::SummaryRepo;
use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, MessageRole, GenerationParams};
use crate::providers::unified::RigProvider;

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

/// Generates (or updates) a rolling summary of evicted conversation messages.
///
/// This is called asynchronously after a streaming response completes,
/// when messages have been evicted from the sliding context window.
/// The summary is stored in the database and injected into future prompts
/// as a "[Story So Far]" system message, giving the LLM memory of earlier events.
///
/// The function is intentionally resilient — failures are logged but don't
/// propagate, since the conversation can function without a summary.
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
            MessageRole::System => continue,
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
        temperature: 0.3,
        ..Default::default()
    };

    match provider.generate(model_id, &messages, &[], &gen_params).await {
        Ok(summary_text) => {
            let token_count = count_tokens(&summary_text) as u32;
            let covered_count = evicted_messages.len() as u32;

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
            Ok(())
        }
    }
}
