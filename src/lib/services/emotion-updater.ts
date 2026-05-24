// ============================================================
//   Mythic — Emotion Updater Pipeline
//
//   After each assistant response completes, infers the character's
//   new emotional state via a secondary generate_raw LLM call.
//   Uses the previous state as a delta baseline for continuity.
//   Runs fire-and-forget — never blocks the conversation flow.
// ============================================================

export interface EmotionState {
  mood:             number; // 0–100: 0=devastated, 50=neutral, 100=elated
  trust:            number; // 0–100: 0=hostile, 50=wary, 100=devoted
  arousal:          number; // 0–100: 0=withdrawn, 50=engaged, 100=intense
  dominant_emotion: string; // single lowercase word
  state_summary:    string; // 1–2 sentences, third person, max 150 chars
}

const SYSTEM_PROMPT = `You are an emotional state analyzer for a roleplay character.
Given the last exchange, infer how the SPECIFIED CHARACTER is feeling RIGHT NOW.

Output ONLY a valid JSON object — no markdown, no explanation, nothing else:
{"mood":<0-100>,"trust":<0-100>,"arousal":<0-100>,"dominant_emotion":"<one_lowercase_word>","state_summary":"<max 120 chars, third person present tense>"}

Rules:
- Base all values on the SPECIFIED CHARACTER's words, tone, body language, and actions — not the user's or other characters'.
- If the response contains dialogue from multiple characters, focus ONLY on the specified character.
- dominant_emotion is a single lowercase English word (e.g. curious, guarded, elated, anxious, tender, wary, melancholy, playful).
- state_summary is 1–2 sentences in third person present tense (e.g. "She feels cautiously hopeful, drawn in but not yet trusting.").
- If a previous state is provided, treat it as the baseline and apply realistic incremental changes — emotions don't flip instantly.
- Keep state_summary under 120 characters.`;

function buildPrompt(
  userMessage:       string,
  assistantResponse: string,
  prev:              EmotionState | null,
  characterName?:    string,
): string {
  const baseline = prev
    ? `Previous state: mood=${prev.mood} trust=${prev.trust} arousal=${prev.arousal} emotion=${prev.dominant_emotion}`
    : 'Previous state: unknown (first exchange — infer from this response alone)';

  const charLine = characterName
    ? `\n[TARGET CHARACTER: ${characterName}]`
    : '';

  return `${baseline}${charLine}

[USER MESSAGE]
${userMessage.slice(0, 800)}

[CHARACTER RESPONSE]
${assistantResponse.slice(0, 1500)}

Infer ${characterName ? characterName + "'s" : "the character's"} new emotional state:`;
}

function parseEmotionResponse(raw: string): EmotionState | null {
  try {
    // Strip markdown fences if the model adds them despite instructions
    let cleaned = raw.trim().replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '');
    const match = cleaned.match(/\{[\s\S]*\}/);
    if (!match) return null;

    const p = JSON.parse(match[0]);

    const clamp = (v: unknown): number =>
      Math.max(0, Math.min(100, Math.round(Number(v) || 50)));
    const str = (v: unknown, fallback: string): string =>
      typeof v === 'string' && v.trim().length > 0 ? v.trim() : fallback;

    return {
      mood:             clamp(p.mood),
      trust:            clamp(p.trust),
      arousal:          clamp(p.arousal),
      dominant_emotion: str(p.dominant_emotion, 'neutral').toLowerCase().split(/\s+/)[0],
      state_summary:    str(p.state_summary, '').slice(0, 150),
    };
  } catch {
    return null;
  }
}

/**
 * Infers and persists the character's new emotional state after an exchange.
 *
 * Designed to run fire-and-forget after stream completion:
 *   updateEmotionalState(...).catch(() => {});
 *
 * Pipeline:
 * 1. Fetch previous state (for delta baseline)
 * 2. Call generate_raw with low temperature for consistent structured output
 * 3. Parse and validate the JSON response
 * 4. Persist via upsert_character_state (upserts on conflict)
 */
export async function updateEmotionalState(
  characterId:       string,
  conversationId:    string,
  userMessage:       string,
  assistantResponse: string,
  characterName?:    string,
): Promise<void> {
  // Skip very short responses — not enough signal to infer emotion
  if (assistantResponse.length < 80) return;

  const ipc = await import('$lib/services/ipc');

  // Fetch previous state for continuity
  let prev: EmotionState | null = null;
  try {
    prev = await ipc.getCharacterState(characterId, conversationId);
  } catch {
    // First turn or fetch failed — fine, we'll infer from scratch
  }

  try {
    const raw = await ipc.generateRaw(
      SYSTEM_PROMPT,
      buildPrompt(userMessage, assistantResponse, prev, characterName),
      undefined, // use default model
      256,       // emotion response is short
      0.3,       // low temperature — we want stable, consistent output
    );

    const state = parseEmotionResponse(raw);
    if (!state) {
      console.warn('[Mythic] Emotion parser returned null for raw:', raw.slice(0, 100));
      return;
    }

    await ipc.upsertCharacterState(
      characterId,
      conversationId,
      state.mood,
      state.trust,
      state.arousal,
      state.dominant_emotion,
      state.state_summary,
    );

    console.debug(
      `[Mythic] Emotion updated → ${state.dominant_emotion} (mood=${state.mood} trust=${state.trust} arousal=${state.arousal})`
    );
  } catch (err) {
    // Never surface emotion errors to the user — it's a background enhancement
    console.warn('[Mythic] Emotion update failed silently:', err);
  }
}
