//! Stage 1 of the NPC detection pipeline — a cheap, narrow LLM call that
//! only decides WHICH names (if any) deserve tracking. It never generates a
//! profile itself; that's Stage 2 (`profile_generator`), and only runs after
//! a name has been flagged here across two separate passes (see
//! `pipeline::run_npc_detection`).

use tracing::warn;

use crate::error::{truncate_at_char_boundary, MythicError};
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::unified::RigProvider;

const NPC_DETECTOR_SYSTEM_PROMPT: &str = r#"You are a narrative-significance filter for a roleplay story. Given the recent narrative, decide which named characters (if any) are becoming genuinely significant to the story — NOT every character mentioned.

Return ONLY valid JSON: {"candidates": [{"name": "...", "tag": "recurring"|"pivotal"}]}

INCLUDE a name only if it shows real narrative weight:
- "pivotal" — clearly set up as a villain, major ally, or holder of a story-critical secret; their actions are driving the plot right now.
- "recurring" — appears with their own dialogue/actions across multiple beats and seems likely to matter again, even if their exact role isn't clear yet.

NEVER include:
- Characters already in the KNOWN CAST list provided to you — do not re-report them.
- Transactional/background characters: a shopkeeper who sells one item, a guard who says one line, a crowd, a passerby, anyone who exists only to service a single beat and then vanishes. These get NO entry at all — not even tagged "recurring".
- The user's own character or any KNOWN CAST member.

Most narrative passages should produce an empty candidates array. Only real story development earns an entry.

Output ONLY the JSON object — no markdown fences, no commentary."#;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CandidateTag {
    pub name: String,
    /// "recurring" | "pivotal"
    pub tag: String,
}

#[derive(Debug, serde::Deserialize)]
struct DetectorResponse {
    #[serde(default)]
    candidates: Vec<CandidateTag>,
}

/// Detects narratively significant new character names in `narrative`,
/// excluding anyone already in `known_names`. Cheap call — max_tokens=300,
/// temperature=0.3 (higher than scene_extractor's 0.1 since this requires
/// narrative judgment, not mechanical field extraction, but still low
/// enough to stay conservative).
pub async fn detect_candidates(
    provider: &RigProvider,
    model_id: &str,
    narrative: &str,
    known_names: &[String],
) -> Result<Vec<CandidateTag>, MythicError> {
    let known_list = if known_names.is_empty() {
        "(none yet)".to_string()
    } else {
        known_names.join(", ")
    };

    let truncated = truncate_at_char_boundary(narrative, 3000);
    let user_prompt = format!(
        "KNOWN CAST (never re-report these):\n{}\n\nRECENT NARRATIVE:\n{}",
        known_list, truncated
    );

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: NPC_DETECTOR_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        max_tokens: 300,
        temperature: 0.3,
        top_p: 0.9,
        ..Default::default()
    };

    let raw_output = provider
        .generate(model_id, &messages, &[], &gen_params)
        .await?;

    let cleaned = raw_output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: DetectorResponse = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "[npc_detector] Failed to parse JSON: {}. Raw: {}",
            e,
            truncate_at_char_boundary(&raw_output, 300)
        );
        MythicError::Provider(format!("NPC detection parse error: {}", e))
    })?;

    Ok(parsed.candidates)
}
