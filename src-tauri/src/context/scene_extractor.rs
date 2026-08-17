//! Scene state extraction engine.
//!
//! After each AI response, this module makes a lightweight LLM call to extract
//! scene state changes (location, time, weather, characters present, mood)
//! from the narrative text. Runs asynchronously — never blocks streaming.

use tracing::{debug, warn};

use crate::error::{truncate_at_char_boundary, MythicError};
use crate::models::conversation::{ChatMessage, GenerationParams, MessageRole};
use crate::models::scene_state::SceneStateUpdate;
use crate::providers::unified::RigProvider;

const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are a scene state extractor for a roleplay narrative. Given the AI character's latest response, extract the current scene state.

Return ONLY valid JSON with these fields. Keep unchanged fields as-is from the current state. Set scene_changed to true only if location, time, or characters present actually changed.

{
  "location_name": "string - name of the current location",
  "location_description": "string - 1-2 sentence description of the location",
  "time_period": "morning|midday|afternoon|evening|night|late_night|dawn|unspecified",
  "weather": "clear|cloudy|raining|storming|snowing|foggy|windy",
  "characters_present": ["Character1", "{{user}}"],
  "ambient_details": "string - sensory details: sounds, smells, lighting",
  "scene_mood": "tense|calm|romantic|mysterious|dangerous|joyful|melancholic|neutral",
  "scene_changed": false,
  "notable_character_event": false
}

Rules:
- Output ONLY the JSON object, no markdown fences, no commentary
- {{user}} always refers to the player/user character
- If the response doesn't mention a field, keep it unchanged from current state
- scene_changed = true only if location changes, time shifts, or characters enter/exit
- notable_character_event = true only if a new character was just introduced by name with dialogue/action, OR an existing character's story role just escalated (revealed as villain/ally/betrayer, etc.) — false for ordinary background mentions or continuing dialogue from an already-established character"#;

/// Extracts scene state changes from the AI's narrative response.
///
/// This is a cheap, fast LLM call (temperature=0.1) that runs in the
/// background after streaming completes. If it fails for any reason, the
/// scene state simply remains unchanged.
pub async fn extract_scene_state(
    provider: &RigProvider,
    model_id: &str,
    ai_response: &str,
    current_state_json: Option<&str>,
) -> Result<SceneStateUpdate, MythicError> {
    let current_context = current_state_json.unwrap_or(
        r#"{"location_name":"Unknown","location_description":"","time_period":"unspecified","weather":"clear","characters_present":[],"ambient_details":"","scene_mood":"neutral"}"#
    );

    let user_prompt = format!(
        "Current scene state:\n{}\n\nLatest narrative response:\n{}",
        current_context,
        // Truncate very long responses to save tokens
        truncate_at_char_boundary(ai_response, 2000)
    );

    let messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: EXTRACTION_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: MessageRole::User,
            content: user_prompt,
        },
    ];

    let gen_params = GenerationParams {
        // Reasoning models (Nemotron, Gemini "thinking" variants, DeepSeek-R1,
        // QwQ, etc.) burn a big chunk of their output budget on a `<think>`
        // preamble before ever reaching the actual JSON answer — and that
        // reasoning counts against max_tokens same as visible output. 300
        // was tuned for plain non-reasoning models and left thinking models
        // truncated mid-thought with no JSON in the response at all.
        max_tokens: 1500,
        temperature: 0.1,
        top_p: 0.9,
        ..Default::default()
    };

    let raw_output = provider
        .generate(model_id, &messages, &[], &gen_params)
        .await?;

    debug!(
        "[scene_extractor] Raw output: {}",
        truncate_at_char_boundary(&raw_output, 200)
    );

    // Parse the JSON — be lenient with markdown fences AND with reasoning
    // models (Nemotron, DeepSeek-R1, QwQ, etc.) that prepend a `<think>...`
    // block or other chatter before the actual object. Rather than special-
    // casing every reasoning-model preamble format, just slice from the
    // first `{` to the last `}` — the object is always the outermost thing
    // in this response, so this survives any wrapper text on either side.
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

    let update: SceneStateUpdate = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "[scene_extractor] Failed to parse JSON: {}. Raw: {}",
            e,
            truncate_at_char_boundary(&raw_output, 300)
        );
        MythicError::Provider(format!("Scene extraction parse error: {}", e))
    })?;

    debug!(
        "[scene_extractor] Extracted: location={:?}, mood={:?}, changed={}",
        update.location_name, update.scene_mood, update.scene_changed
    );

    Ok(update)
}
