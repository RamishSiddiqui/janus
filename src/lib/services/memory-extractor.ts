// ============================================================
//   Mythic — Memory Auto-Extraction Pipeline
//   LLM-powered structured fact extraction from RP conversations
//
//   Architecture informed by:
//   • ChatGPT Memory — implicit fact detection, atomic structured facts,
//     injected as context layer (not RAG), contradiction resolution
//   • Letta/MemGPT — tiered memory (working/recall/archival), agent
//     self-editing via tool calls, autonomous fact curation
//   • Replika — DB-stored facts, selective per-message retrieval,
//     contextual injection into LLM window
//   • SillyTavern — recursive summaries (Summaryception), VectorDB RAG,
//     hybrid approach with multiple compression layers
//   • Character.AI — manual pinning + fixed memory fields (400 chars),
//     user-driven persistence via Personas
//
//   Our approach: Two-tier extraction
//   1. Primary: LLM-powered structured extraction via `generate_raw`
//      (stateless endpoint that doesn't pollute conversations)
//   2. Fallback: Pattern-based heuristic extraction when LLM is
//      unavailable, rate-limited, or fails
// ============================================================

/**
 * Extraction result — a structured, atomic fact suitable for memory storage.
 * Each fact is self-contained and can be stored/updated/deleted independently.
 * Follows ChatGPT's pattern of atomic, categorized user facts.
 */
export interface ExtractedFact {
  /** The condensed, atomic fact (one sentence, third-person past tense). */
  summary: string;
  /** Category for future filtering/retrieval. */
  category: 'character' | 'location' | 'event' | 'relationship' | 'item' | 'decision';
}

/**
 * System prompt for LLM-powered memory extraction.
 *
 * Design principles (informed by industry research):
 * - Atomic facts: one sentence each, independently manageable (ChatGPT pattern)
 * - Third-person past tense: consistent format for memory injection
 * - Category tagging: enables Replika-style selective retrieval
 * - Strict JSON output: reliable parsing without markdown fencing
 * - Deduplication hint: skip information that's common knowledge in the world
 * - Hard cap of 5: keeps memory store lean (Letta pattern of curated memory)
 */
const EXTRACTION_SYSTEM_PROMPT = `You are a memory extraction system for a roleplay application.
Analyze the conversation exchange and identify notable facts worth remembering long-term.

EXTRACT:
- Character names, titles, and identity reveals
- Locations visited or mentioned (cities, realms, buildings)
- Significant events (battles, discoveries, transformations, deaths)
- Relationship changes (alliances, betrayals, confessions, bonds)
- Important items (weapons, artifacts, gifts, documents)
- Key decisions and commitments (vows, promises, plans)

RULES:
- Each fact must be a single, self-contained sentence.
- Write in third person, past tense (e.g., "Aria revealed she is the last of the Sky Elves").
- Only extract genuinely significant information — skip greetings, small talk, routine actions.
- Do NOT extract information that would be obvious from the character's description.
- Maximum 5 facts per extraction.
- If nothing notable happened, return an empty array.

OUTPUT FORMAT (strict JSON array, no markdown fencing, no explanation):
[{"summary":"...","category":"character|location|event|relationship|item|decision"},...]

If nothing notable: []`;

/**
 * Build the user prompt for the extraction LLM call.
 */
function buildExtractionPrompt(userMessage: string, assistantResponse: string): string {
  return `Extract notable facts from this roleplay exchange:

[USER]
${userMessage.slice(0, 1000)}

[ASSISTANT]
${assistantResponse.slice(0, 2000)}`;
}

/**
 * Parse the LLM's extraction response into structured facts.
 * Handles common LLM output quirks: markdown fencing, trailing commas, etc.
 */
function parseExtractionResponse(raw: string): ExtractedFact[] {
  try {
    let cleaned = raw.trim();
    // Strip markdown code fences if the model adds them despite instructions
    if (cleaned.startsWith('```')) {
      cleaned = cleaned.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '');
    }
    // Handle models that wrap in extra text
    const arrayMatch = cleaned.match(/\[[\s\S]*\]/);
    if (arrayMatch) {
      cleaned = arrayMatch[0];
    }

    const parsed = JSON.parse(cleaned);
    if (!Array.isArray(parsed)) return [];

    const validCategories = new Set(['character', 'location', 'event', 'relationship', 'item', 'decision']);

    return parsed
      .filter((item: any) =>
        typeof item.summary === 'string' &&
        item.summary.length > 10 &&
        item.summary.length < 300 &&
        validCategories.has(item.category)
      )
      .slice(0, 5)
      .map((item: any) => ({
        summary: item.summary.trim(),
        category: item.category as ExtractedFact['category'],
      }));
  } catch {
    console.warn('[Mythic] Failed to parse extraction response:', raw.slice(0, 200));
    return [];
  }
}

// ============================================================
//   Throttle — Extract every Nth message (cost management)
// ============================================================

const EXTRACT_EVERY_N = 3;
const MIN_RESPONSE_LENGTH = 100;
let messageCounter = 0;

/** Check if we should extract from this response. */
export function shouldExtract(): boolean {
  messageCounter++;
  return messageCounter % EXTRACT_EVERY_N === 0;
}

/** Reset counter (on conversation switch). */
export function resetCounter(): void {
  messageCounter = 0;
}

// ============================================================
//   Main Pipeline — LLM extraction with heuristic fallback
// ============================================================

/**
 * Extract and save memories from a completed AI response.
 *
 * Pipeline:
 * 1. Try LLM-powered extraction via `generate_raw` (structured JSON facts)
 * 2. If LLM fails, fall back to heuristic pattern matching
 * 3. Save extracted facts via `createMemory` with source='auto'
 *
 * Runs asynchronously — never blocks the main conversation flow.
 */
export async function extractAndSaveMemories(
  conversationId: string,
  characterId: string | null | undefined,
  userMessage: string,
  assistantResponse: string,
): Promise<number> {
  if (assistantResponse.length < MIN_RESPONSE_LENGTH) return 0;

  const ipc = await import('$lib/services/ipc');
  let facts: ExtractedFact[] = [];

  // --- Tier 1: LLM-powered extraction ---
  try {
    const raw = await ipc.generateRaw(
      EXTRACTION_SYSTEM_PROMPT,
      buildExtractionPrompt(userMessage, assistantResponse),
      undefined, // use default model
      512,       // max tokens — extraction responses are short
      0.2,       // low temperature for consistent structured output
    );
    facts = parseExtractionResponse(raw);
    if (facts.length > 0) {
      console.debug(`[Mythic] LLM extracted ${facts.length} fact(s)`);
    }
  } catch (err) {
    console.warn('[Mythic] LLM extraction failed, falling back to heuristics:', err);
  }

  // --- Tier 2: Heuristic fallback ---
  if (facts.length === 0) {
    facts = heuristicExtract(assistantResponse);
    if (facts.length > 0) {
      console.debug(`[Mythic] Heuristic extracted ${facts.length} fact(s)`);
    }
  }

  if (facts.length === 0) return 0;

  // --- Save to backend ---
  let saved = 0;
  for (const fact of facts) {
    try {
      await ipc.createMemory(
        `[${fact.category}] ${fact.summary}`,
        characterId ?? undefined,
        conversationId,
        'auto',
      );
      saved++;
    } catch (err) {
      console.warn('[Mythic] Failed to save auto-memory:', err);
    }
  }

  return saved;
}

// ============================================================
//   Heuristic Fallback — Pattern-based extraction
//   Used when LLM extraction is unavailable or fails.
// ============================================================

const EXTRACTION_RULES: Array<{ pattern: RegExp; category: ExtractedFact['category'] }> = [
  { pattern: /(?:my name is|i am called|they call me|i'm known as|introduces? (?:him|her|them)self as)\s+["']?(\w[\w\s]{1,30})/i, category: 'character' },
  { pattern: /(?:we (?:are|arrived|have reached) (?:at|in)|welcome to|this (?:is|place is called)|the (?:city|town|village|kingdom|realm) of)\s+["']?([A-Z][\w\s]{2,30})/i, category: 'location' },
  { pattern: /(?:betray(?:s|ed)?|allies? with|joins? forces|forms? (?:a |an )?alliance|becomes? (?:friends?|enemies?|allies?|rivals?)|confesses? (?:love|feelings)|pledg(?:es?|ed) (?:loyalty|allegiance))/i, category: 'relationship' },
  { pattern: /(?:i (?:promise|swear|vow|pledge|decide)|we must|it is decided|the pact is sealed|chooses? to|resolves? to)/i, category: 'decision' },
  { pattern: /(?:(?:wields?|carries?|possesses?|reveals?|discovers?|bestows?|forges?|crafts?)\s+(?:a |an |the )?\w[\w\s]{2,30})/i, category: 'item' },
  { pattern: /(?:falls? (?:in battle|dead|unconscious)|is (?:slain|defeated|captured|transformed)|the battle (?:ends?|is won|is lost)|(?:war|siege|invasion) (?:begins?|ends?))/i, category: 'event' },
  { pattern: /(?:the (?:truth|secret|prophecy) (?:is|was)|reveals? (?:that|the truth)|confess(?:es)? (?:that|to))/i, category: 'event' },
];

function heuristicExtract(response: string): ExtractedFact[] {
  const facts: ExtractedFact[] = [];
  const seenKeys = new Set<string>();

  for (const rule of EXTRACTION_RULES) {
    const match = response.match(rule.pattern);
    if (match) {
      const sentence = extractSentence(response, match.index ?? 0);
      const summary = cleanForMemory(sentence);
      const key = summary.toLowerCase();

      if (summary.length > 15 && summary.length < 200 && !seenKeys.has(key)) {
        seenKeys.add(key);
        facts.push({ summary, category: rule.category });
      }
    }
    if (facts.length >= 3) break;
  }

  if (facts.length === 0) {
    const best = findNarrativePeak(response);
    if (best) facts.push({ summary: best, category: 'event' });
  }

  return facts;
}

function extractSentence(text: string, matchIndex: number): string {
  const before = text.lastIndexOf('.', matchIndex);
  const after = text.indexOf('.', matchIndex);
  return text.slice(before >= 0 ? before + 1 : 0, after >= 0 ? after + 1 : text.length).trim();
}

function cleanForMemory(text: string): string {
  return text
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/[#>]+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function findNarrativePeak(text: string): string | null {
  const sentences = text
    .replace(/\*([^*]+)\*/g, '$1')
    .split(/(?<=[.!?])\s+/)
    .filter(s => s.length > 20 && s.length < 200);

  if (sentences.length === 0) return null;

  let bestScore = 0;
  let bestSentence = '';

  for (const sentence of sentences) {
    let score = 0;
    score += (sentence.match(/(?<=\s)[A-Z][a-z]{2,}/g)?.length ?? 0) * 2;
    if (/[""\u201C\u201D]/.test(sentence)) score += 3;
    if (/\b(?:attack|discover|reveal|betray|transform|escape|arrive|defeat|summon|destroy|create|forge)\b/i.test(sentence)) score += 4;
    if (/\b(?:love|fear|rage|joy|sorrow|betrayal|hope|despair|grief|triumph)\b/i.test(sentence)) score += 3;

    if (score > bestScore) {
      bestScore = score;
      bestSentence = sentence;
    }
  }

  return bestScore >= 4 ? cleanForMemory(bestSentence) : null;
}
