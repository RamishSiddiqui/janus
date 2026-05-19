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
  category: 'character' | 'location' | 'event' | 'relationship' | 'item' | 'decision' | 'emotion' | 'trait' | 'preference' | 'atmosphere';
}

/**
 * System prompt for LLM-powered memory extraction.
 *
 * Design philosophy: Extract the "connective tissue" of a story —
 * not just plot beats, but the emotional texture, interpersonal dynamics,
 * and recurring motifs that make a narrative feel alive and continuous
 * across sessions. A good memory should make the reader think
 * "oh right, THAT happened" and feel the emotional weight of it.
 *
 * Architecture:
 * - Atomic facts: one sentence each, independently manageable (ChatGPT pattern)
 * - Third-person past tense: consistent format for memory injection
 * - Category tagging: enables Replika-style selective retrieval
 * - Strict JSON output: reliable parsing without markdown fencing
 * - Deduplication hint: skip information already in existing memories
 * - Hard cap of 5: keeps memory store lean (Letta pattern of curated memory)
 */
const EXTRACTION_SYSTEM_PROMPT = `You are an immersive memory system for a narrative roleplay application. Your job is to extract the moments that MATTER — the emotional beats, relationship shifts, and character-defining choices that make a story feel alive across sessions.

Think like a reader annotating their favorite novel: what would you highlight, bookmark, or scribble in the margin?

EXTRACT THESE (in priority order):

1. RELATIONSHIP DYNAMICS — The heart of any story
   - Bond-forming moments: "She chose to sit next to him instead of her classmates"
   - Trust shifts: "He hesitated before sharing the map, then handed it over"
   - Tension/conflict: "Her voice went cold when he mentioned the tournament"
   - Flirting/attraction: "She lingered at the doorway, glancing back with a half-smile"
   - Inside jokes/callbacks: "She teased him about the fire incident again"

2. EMOTIONAL TEXTURE — What makes characters feel real
   - Vulnerability shown: "Her voice cracked when she mentioned her mother"
   - Joy/humor: "They both burst out laughing at the absurdity of the situation"  
   - Fear/anxiety: "His hands trembled as he approached the dark corridor"
   - Anger/frustration: "She slammed the book shut and refused to explain further"

3. CHARACTER-DEFINING MOMENTS — Who they ARE, not just what they do
   - Traits revealed through action (not stated): "She instinctively stepped in front of him when the noise came"
   - Habits/quirks: "She always tucks her hair behind her ear when nervous"
   - Skills demonstrated: "She cast a wordless barrier spell without seeming to think about it"
   - Contradictions: "Despite claiming she didn't care, she stayed up all night preparing"

4. PLOT BEATS — What HAPPENED
   - Significant events, discoveries, arrivals, departures
   - Decisions with consequences: "He chose to enter the forest alone"
   - Promises, vows, plans made
   - Items received, locations discovered

5. ATMOSPHERE & SETTING — Where the story lives
   - Recurring locations: "The library alcove became their regular meeting spot"
   - Time/weather details that matter: "It was raining the night they first practiced together"
   - World-building facts revealed in conversation

6. USER PREFERENCES — What the player values
   - How they prefer to be addressed
   - Play style signals: cautious vs. reckless, romantic vs. adventure-focused
   - Choices that reveal what they find fun

RULES:
- Each fact MUST be a single, self-contained sentence in third-person past tense.
- Capture the EMOTIONAL WEIGHT, not just the action: "Aria saved him" is weak; "Aria threw herself between him and the blast without hesitation, earning his stunned gratitude" is strong.
- Prioritize moments that would be satisfying to reference later ("Remember when...").
- Do NOT re-extract facts that appear in the EXISTING MEMORIES section below.
- Do NOT extract things obvious from the character card/description.
- Maximum 5 facts. Aim for 2-3 quality extractions per exchange.
- If truly nothing meaningful happened (pure small talk), return an empty array.

OUTPUT FORMAT (strict JSON array, no markdown fencing, no explanation):
[{"summary":"...","category":"character|location|event|relationship|item|decision|emotion|trait|preference|atmosphere"},...]

If nothing meaningful: []`;

/**
 * Build the user prompt for the extraction LLM call.
 * Includes existing memories so the LLM can avoid duplicates and build
 * on what's already known — a critical feature for long-running stories.
 */
function buildExtractionPrompt(
  userMessage: string,
  assistantResponse: string,
  existingMemories?: string[],
): string {
  let prompt = `Extract memorable facts from this roleplay exchange:\n\n`;

  if (existingMemories && existingMemories.length > 0) {
    prompt += `EXISTING MEMORIES (do NOT re-extract these):\n`;
    for (const mem of existingMemories.slice(-15)) {
      prompt += `- ${mem}\n`;
    }
    prompt += `\n`;
  }

  prompt += `[USER MESSAGE]\n${userMessage.slice(0, 1500)}\n\n`;
  prompt += `[CHARACTER RESPONSE]\n${assistantResponse.slice(0, 3000)}`;
  return prompt;
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

    const validCategories = new Set([
      'character', 'location', 'event', 'relationship', 'item', 'decision',
      'emotion', 'trait', 'preference', 'atmosphere',
    ]);

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

// Extract from every message — individual LLM calls are cheap and memories
// are the primary mechanism for cross-session continuity. The old value of 3
// meant most short conversations never got any memories at all.
const EXTRACT_EVERY_N = 1;
const MIN_RESPONSE_LENGTH = 80;
let messageCounter = 0;

/** Check if we should extract from this response. */
export function shouldExtract(): boolean {
  messageCounter++;
  const should = messageCounter % EXTRACT_EVERY_N === 0;
  console.debug(`[Mythic] Memory extraction check: message #${messageCounter}, extract=${should}`);
  return should;
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
  if (assistantResponse.length < MIN_RESPONSE_LENGTH) {
    console.debug(`[Mythic] Skipping extraction — response too short (${assistantResponse.length} < ${MIN_RESPONSE_LENGTH})`);
    return 0;
  }

  const ipc = await import('$lib/services/ipc');
  let facts: ExtractedFact[] = [];

  // Fetch existing memories for this character/conversation to avoid duplicates
  let existingMemoryTexts: string[] = [];
  try {
    const memories = await ipc.listMemories(characterId ?? undefined, conversationId);
    existingMemoryTexts = memories.map((m: any) => m.content);
  } catch {
    // Non-critical — proceed without dedup context
  }

  // --- Tier 1: LLM-powered extraction ---
  try {
    const raw = await ipc.generateRaw(
      EXTRACTION_SYSTEM_PROMPT,
      buildExtractionPrompt(userMessage, assistantResponse, existingMemoryTexts),
      undefined, // use default model
      512,       // max tokens — extraction responses are short
      0.3,       // slightly higher temp for richer extractions
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

  return bestScore >= 3 ? cleanForMemory(bestSentence) : null;
}
