//! Stage 2 of the NPC detection pipeline — a rarer, pricier LLM call that
//! generates a full CharacterCardV2-shaped profile for a candidate that has
//! already crossed the two-pass significance debounce in `detector`. Fires
//! once per candidate, ever.

use tracing::warn;

use crate::error::{truncate_at_char_boundary, truncate_tail_at_char_boundary, MythicError};
use crate::models::character::CharacterData;
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::providers::unified::RigProvider;

const PROFILE_GENERATOR_SYSTEM_PROMPT: &str = r#"You are a character profile writer for an ongoing roleplay story. You'll be given a character's name, the recent narrative they appeared in, the current scene, and the existing cast. Write a backstory-aligned profile for them.

Return ONLY valid JSON with these fields:
{
  "name": "string - the character's name, exactly as given (or the alias/epithet the story uses for them, e.g. \"the Hooded Stranger\", if their real name hasn't been revealed)",
  "description": "string - detailed appearance, personality, and backstory, written to align with what's already happened in the narrative",
  "personality": "string - a brief personality summary",
  "scenario": "string - how this character fits into the current story/scene",
  "first_mes": "string - NOT used for a greeting message; leave as an empty string",
  "tags": ["string", "..." - short descriptive tags, e.g. \"villain\", \"ally\", \"mysterious\"],
  "identity_concealed": "boolean - true ONLY if the story has deliberately not revealed who this character really is (a cloaked figure, a masked stranger, a hidden identity); false otherwise"
}

Rules:
- Stay consistent with the EXISTING CAST provided — do not contradict established facts, and do not duplicate another character's role or backstory.
- Tie the backstory to concrete events already in the narrative — don't invent an unrelated history.
- If identity_concealed is true: write description/personality using ONLY vague, in-universe terms the story has actually shown (posture, voice, clothing, demeanor) — do NOT invent a real name, face, backstory detail, or motive the narrative hasn't actually disclosed. A wrong guess here is a spoiler to the player; when genuinely unsure, stay vague rather than specific.
- Output ONLY the JSON object — no markdown fences, no commentary."#;

/// Generates a CharacterData profile for `candidate_name`, informed by the
/// recent narrative, current scene state, and existing cast (to stay
/// consistent/non-duplicative). max_tokens=900, temperature=0.7 — creative
/// but grounded; distinctly pricier than Stage 1's mechanical detection.
pub async fn generate_profile(
    provider: &RigProvider,
    model_id: &str,
    candidate_name: &str,
    narrative_context: &str,
    scene_context: Option<&str>,
    existing_cast: &[(String, String)],
) -> Result<CharacterData, MythicError> {
    let cast_list = if existing_cast.is_empty() {
        "(no other characters yet)".to_string()
    } else {
        existing_cast
            .iter()
            .map(|(name, desc)| format!("- {}: {}", name, desc))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let truncated = truncate_at_char_boundary(narrative_context, 4000);

    let user_prompt = format!(
        "CHARACTER TO PROFILE: {}\n\nEXISTING CAST (stay consistent — don't contradict or duplicate):\n{}\n\nCURRENT SCENE:\n{}\n\nRECENT NARRATIVE:\n{}",
        candidate_name,
        cast_list,
        scene_context.unwrap_or("(unknown)"),
        truncated,
    );

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: PROFILE_GENERATOR_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        max_tokens: 900,
        temperature: 0.7,
        top_p: 0.95,
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

    let profile: CharacterData = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "[npc_profile_generator] Failed to parse JSON: {}. Raw: {}",
            e,
            truncate_at_char_boundary(&raw_output, 300)
        );
        MythicError::Provider(format!("NPC profile generation parse error: {}", e))
    })?;

    Ok(profile)
}

/// Result of [`refine_profile`] — deliberately narrower than the full
/// [`CharacterData`] Stage 1 generates: only the three fields a refresh
/// should ever touch. Name, tags, `identity_concealed`, etc. stay exactly
/// as the user set them regardless of what the story implies, so a refresh
/// can never silently rename someone or flip their concealment status.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RefinedProfile {
    pub description: String,
    pub personality: String,
    pub scenario: String,
}

const REFRESH_SYSTEM_PROMPT: &str = r#"You are refining an existing roleplay character's profile using how they've actually appeared in the story so far. You'll be given their CURRENT profile, known facts about them established in the story (both settled canon and things that happened in this conversation), recent story dialogue/narration to infer voice and mannerisms from, and the existing cast for consistency.

The CURRENT DESCRIPTION may just be a placeholder like "Just spoke for the first time — their role in the story isn't clear yet." Treat that exact kind of text as EMPTY — there is no real information there yet, so write a fresh profile from the story context instead of trying to preserve or paraphrase it. Otherwise, REFINE the existing profile: keep whatever is still accurate, correct or drop what the story has since contradicted, weave in newly established facts, and match the voice/tone/mannerisms actually shown in the recent dialogue — not a generic rewrite.

Return ONLY valid JSON with these three fields:
{
  "description": "string - updated appearance, personality, and backstory, aligned with the story so far",
  "personality": "string - a brief personality summary reflecting how they've actually behaved",
  "scenario": "string - how this character currently fits into the story"
}

Rules:
- Stay consistent with the EXISTING CAST provided — don't contradict established facts or duplicate another character's role.
- Ground every detail in the story shown — don't invent unrelated history.
- If known facts are given, treat them as accurate — don't contradict them without a clear reason shown in the recent dialogue.
- Output ONLY the JSON object — no markdown fences, no commentary, no <think> preamble."#;

/// Refines an EXISTING character's `description`/`personality`/`scenario`
/// against how they've actually shown up in the story — the "Refresh from
/// Story" action, both its manual (button) and automatic (still-placeholder
/// detection) triggers. Unlike `generate_profile` (which writes an entirely
/// new profile for a brand-new candidate), this always has a current
/// profile to refine against and may also have known facts to respect.
pub async fn refine_profile(
    provider: &RigProvider,
    model_id: &str,
    character_name: &str,
    current_description: &str,
    current_personality: &str,
    current_scenario: &str,
    known_facts: &[String],
    recent_dialogue: &str,
    existing_cast: &[(String, String)],
    system_prompt_override: Option<&str>,
) -> Result<RefinedProfile, MythicError> {
    // Settings > Prompts lets the user rewrite this instruction (mirrors the
    // global systemPrompt/postHistoryInstructions pattern). Callers with no
    // frontend context (the automatic still-placeholder trigger in
    // pipeline.rs) pass None and get the built-in default instead.
    let system_prompt = match system_prompt_override {
        Some(s) if !s.trim().is_empty() => s,
        _ => REFRESH_SYSTEM_PROMPT,
    };

    let cast_list = if existing_cast.is_empty() {
        "(no other characters)".to_string()
    } else {
        existing_cast
            .iter()
            .map(|(name, desc)| format!("- {}: {}", name, desc))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let known_facts_list = if known_facts.is_empty() {
        "(none confirmed yet)".to_string()
    } else {
        known_facts
            .iter()
            .map(|f| format!("- {}", f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let truncated = truncate_tail_at_char_boundary(recent_dialogue, 6000);

    let user_prompt = format!(
        "CHARACTER: {}\n\nCURRENT DESCRIPTION:\n{}\n\nCURRENT PERSONALITY:\n{}\n\nCURRENT SCENARIO:\n{}\n\nKNOWN STORY FACTS (confirmed — don't contradict):\n{}\n\nEXISTING CAST (stay consistent):\n{}\n\nRECENT STORY DIALOGUE (voice/tone anchor):\n{}",
        character_name,
        if current_description.trim().is_empty() { "(none)" } else { current_description },
        if current_personality.trim().is_empty() { "(none)" } else { current_personality },
        if current_scenario.trim().is_empty() { "(none)" } else { current_scenario },
        known_facts_list,
        cast_list,
        truncated,
    );

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: system_prompt.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        // Reasoning models can burn a few hundred tokens on a <think> block
        // before ever reaching the JSON — same lesson as scene extraction's
        // token budget (see the matching comment in scene_extractor.rs).
        max_tokens: 1500,
        temperature: 0.7,
        top_p: 0.95,
        ..Default::default()
    };

    let raw_output = provider
        .generate(model_id, &messages, &[], &gen_params)
        .await?;

    let fenced = raw_output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let cleaned = match (fenced.find('{'), fenced.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &fenced[start..=end],
        _ => fenced,
    };

    let profile: RefinedProfile = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "[npc_profile_generator] Failed to parse refine JSON: {}. Raw: {}",
            e,
            truncate_at_char_boundary(&raw_output, 300)
        );
        MythicError::Provider(format!("Profile refresh parse error: {}", e))
    })?;

    Ok(profile)
}

/// A single lorebook entry as generated by the LLM, before it's persisted —
/// mirrors the fields `LorebookRepo::create`/`update` accept.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GeneratedLorebookEntry {
    pub name: String,
    pub keys: Vec<String>,
    pub content: String,
    #[serde(default)]
    pub always_active: bool,
    #[serde(default = "default_lorebook_priority")]
    pub priority: i32,
}

fn default_lorebook_priority() -> i32 {
    10
}

const LOREBOOK_GENERATOR_SYSTEM_PROMPT: &str = r#"You are a worldbuilding assistant for an ongoing roleplay story. You'll be given a character's profile, known facts confirmed in the story, recent dialogue, and any lorebook entries that already exist for them. Write NEW lorebook entries — keyword-triggered snippets of world/character lore that get injected into the AI's context only when relevant, so the model doesn't need everything crammed into the character card at once.

Return ONLY a valid JSON array, each element shaped like:
{
  "name": "string - short entry title",
  "keys": ["string", "..." - trigger keywords/phrases; when any appears in recent chat, this entry gets injected],
  "content": "string - 2-4 sentences of lore, grounded in the character's profile and the story so far",
  "always_active": "boolean - true ONLY for something so central it should inject every message (usually just the character's own core identity, if no such entry already exists); false for everything else",
  "priority": "integer 1-20 - higher injects first when the token budget is tight; core identity/relationships higher, minor world color lower"
}

Rules:
- Cover distinct facets: heritage/identity, key locations, relationships with other named characters, objects/artifacts, ongoing goals or conflicts — whatever is actually present in the given material. Don't invent facets that aren't grounded in the profile/facts/dialogue.
- Do NOT duplicate or closely overlap any EXISTING ENTRY listed below — skip that facet entirely if it's already covered.
- Ground everything in the given profile/facts/dialogue — don't invent unrelated history.
- Write 4-8 entries. Fewer if there genuinely isn't enough material yet; never pad with filler.
- Output ONLY the JSON array — no markdown fences, no commentary."#;

/// Generates new lorebook entries for a character from their profile, known
/// story facts, and recent dialogue — the "Generate from Story" action.
/// `existing_entry_names` is passed so the model skips facets already
/// covered by real entries, rather than producing near-duplicates every time
/// this is re-run.
pub async fn generate_lorebook_entries(
    provider: &RigProvider,
    model_id: &str,
    character_name: &str,
    description: &str,
    personality: &str,
    scenario: &str,
    known_facts: &[String],
    recent_dialogue: &str,
    existing_entry_names: &[String],
) -> Result<Vec<GeneratedLorebookEntry>, MythicError> {
    let known_facts_list = if known_facts.is_empty() {
        "(none confirmed yet)".to_string()
    } else {
        known_facts
            .iter()
            .map(|f| format!("- {}", f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let existing_list = if existing_entry_names.is_empty() {
        "(none yet)".to_string()
    } else {
        existing_entry_names
            .iter()
            .map(|n| format!("- {}", n))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let truncated_dialogue = truncate_tail_at_char_boundary(recent_dialogue, 6000);

    let user_prompt = format!(
        "CHARACTER: {}\n\nDESCRIPTION:\n{}\n\nPERSONALITY:\n{}\n\nSCENARIO:\n{}\n\nKNOWN STORY FACTS:\n{}\n\nEXISTING LOREBOOK ENTRIES (don't duplicate):\n{}\n\nRECENT STORY DIALOGUE:\n{}",
        character_name,
        if description.trim().is_empty() { "(none)" } else { description },
        if personality.trim().is_empty() { "(none)" } else { personality },
        if scenario.trim().is_empty() { "(none)" } else { scenario },
        known_facts_list,
        existing_list,
        truncated_dialogue,
    );

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: LOREBOOK_GENERATOR_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        max_tokens: 2000,
        temperature: 0.7,
        top_p: 0.95,
        ..Default::default()
    };

    let raw_output = provider
        .generate(model_id, &messages, &[], &gen_params)
        .await?;

    let fenced = raw_output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let cleaned = match (fenced.find('['), fenced.rfind(']')) {
        (Some(start), Some(end)) if end >= start => &fenced[start..=end],
        _ => fenced,
    };

    let entries: Vec<GeneratedLorebookEntry> = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "[npc_profile_generator] Failed to parse lorebook JSON: {}. Raw: {}",
            e,
            truncate_at_char_boundary(&raw_output, 300)
        );
        MythicError::Provider(format!("Lorebook generation parse error: {}", e))
    })?;

    Ok(entries)
}
