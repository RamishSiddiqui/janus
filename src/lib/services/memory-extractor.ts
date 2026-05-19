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
//
//   Core insight: Chat history IS the short-term memory. Extraction
//   should ONLY capture facts that transcend the current session —
//   things that, if forgotten, would cause a continuity error in
//   a future conversation. Most messages produce ZERO memories.
// ============================================================

/**
 * Extraction result — a structured, atomic fact suitable for memory storage.
 * Each fact is self-contained and can be stored/updated/deleted independently.
 */
export interface ExtractedFact {
  /** The condensed, atomic fact (one sentence, third-person past tense). */
  summary: string;
  /** Category tag — suggested: identity, relationship, world, decision, revelation. LLM may use others. */
  category: string;
}

/**
 * System prompt for LLM-powered memory extraction.
 *
 * Design philosophy: Memories are NOT a conversation log.
 * The full chat history is already passed to the LLM each turn — so memories
 * should ONLY capture facts that transcend the current session. These are
 * facts that, if lost, would cause a continuity error in a future conversation.
 *
 * Litmus test: "Would not knowing this make the character act wrong next time?"
 *   YES → extract: "The user revealed they were friends with Aria's mother"
 *   NO  → skip: "Aria smiled warmly and welcomed the user to the library"
 */
const EXTRACTION_SYSTEM_PROMPT = `You are a memory filter for a roleplay application. Your job is to identify ONLY the facts that would cause a continuity error if forgotten in a future conversation session.

IMPORTANT CONTEXT: The full chat history is already visible to the AI during each conversation. You are NOT logging what happened — you are identifying facts that must persist ACROSS sessions, after the chat history is gone.

LITMUS TEST — Before extracting anything, ask:
"If the character forgot this in a new conversation next week, would it feel WRONG?"
- YES → Extract it. "The user revealed they knew Aria's mother personally" — forgetting this would break the relationship.
- NO  → Skip it. "Aria smiled and offered to show them around" — normal scene flow, already in chat history.

EXTRACT ONLY:
1. IDENTITY — Who someone actually IS
   - "The user revealed they are a former knight of the Silver Order"
   - "Aria confessed she is secretly the heir to the throne"

2. RELATIONSHIP — Permanent shifts in how people relate to each other
   - "The user told Aria they knew her mother — Aria was visibly shaken"
   - "Aria and the user made a blood pact to investigate the ruins together"
   - NOT: "Aria laughed at the user's joke" (mood, not a relationship shift)

3. WORLD — Discoveries that alter the story's landscape permanently
   - "They discovered the old temple is actually a sealed portal"
   - "The merchant revealed the king has been dead for three months"

4. DECISION — Commitments that constrain future behavior
   - "The user swore an oath to protect Aria's secret"
   - "Aria promised to teach the user fire magic starting next week"
   - NOT: "The user decided to explore the market" (temporary action, not binding)

5. REVELATION — Secrets, confessions, or truths that permanently change the dynamic
   - "Aria admitted she started the fire that burned the academy wing"
   - "The user confessed they are not from this world"

DO NOT EXTRACT:
- Actions that are part of normal scene flow (walking, talking, reacting, emoting)
- Emotional reactions that don't permanently change a relationship
- Descriptions of settings, atmosphere, or how someone looked in a moment
- Character traits that are already in the character's description card
- Anything already present in EXISTING MEMORIES
- Things the chat history already covers — you're not a session logger

RULES:
- Maximum 2 facts per extraction. Most exchanges should produce ZERO.
- Each fact must be one sentence, third-person past tense.
- Returning [] is the EXPECTED outcome for most messages. Only truly significant moments get extracted.

OUTPUT FORMAT (strict JSON array, no markdown fencing, no explanation):
[{"summary":"...","category":"<short tag, e.g. identity, relationship, world, decision, revelation, secret, promise, or any fitting label>"}]

Expected output for most messages: []`;

/**
 * Build the user prompt for the extraction LLM call.
 * Includes existing memories so the LLM avoids duplicates.
 */
function buildExtractionPrompt(
  userMessage: string,
  assistantResponse: string,
  existingMemories?: string[],
): string {
  let prompt = `Analyze this roleplay exchange. Extract ONLY facts that would cause a continuity error if forgotten in a future session.\n\n`;

  if (existingMemories && existingMemories.length > 0) {
    prompt += `EXISTING MEMORIES (already known — do NOT repeat):\n`;
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

    return parsed
      .filter((item: any) =>
        typeof item.summary === 'string' &&
        item.summary.length > 10 &&
        item.summary.length < 300 &&
        typeof item.category === 'string' &&
        item.category.length > 0
      )
      .slice(0, 2) // Hard cap: max 2 facts per extraction
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

// Every 3rd message — most exchanges don't contain memory-worthy facts.
// The LLM itself will return [] for most exchanges anyway, so this is
// purely a cost optimization to avoid unnecessary API calls.
const EXTRACT_EVERY_N = 3;
const MIN_RESPONSE_LENGTH = 150;
let messageCounter = 0;

/** Check if we should extract from this response. */
export function shouldExtract(): boolean {
  messageCounter++;
  const should = messageCounter % EXTRACT_EVERY_N === 0;
  if (should) {
    console.debug(`[Mythic] Memory extraction triggered (message #${messageCounter})`);
  }
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
 * Most calls will produce 0 memories — this is by design.
 */
export async function extractAndSaveMemories(
  conversationId: string,
  characterId: string | null | undefined,
  userMessage: string,
  assistantResponse: string,
): Promise<number> {
  if (assistantResponse.length < MIN_RESPONSE_LENGTH) {
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
      256,       // max tokens — responses are short (usually just [])
      0.1,       // very low temperature for consistent, conservative output
    );
    facts = parseExtractionResponse(raw);
    if (facts.length > 0) {
      console.debug(`[Mythic] LLM extracted ${facts.length} fact(s):`, facts.map(f => f.summary));
    }
  } catch (err) {
    console.warn('[Mythic] LLM extraction failed, falling back to heuristics:', err);
  }

  // --- Tier 2: Heuristic fallback ---
  // Only triggers on high-signal patterns (identity reveals, oaths, confessions)
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
//   Only fires on HIGH-SIGNAL patterns that almost always indicate
//   a memory-worthy fact: identity reveals, oaths, confessions, etc.
// ============================================================

const EXTRACTION_RULES: Array<{ pattern: RegExp; category: ExtractedFact['category'] }> = [
  // Identity reveals
  { pattern: /(?:my name is|i am called|they call me|i'm known as|introduces? (?:him|her|them)self as)\s+["']?(\w[\w\s]{1,30})/i, category: 'identity' },
  // Relationship-altering confessions
  { pattern: /(?:betray(?:s|ed)?|allies? with|joins? forces|forms? (?:a |an )?alliance|becomes? (?:friends?|enemies?|allies?|rivals?)|confesses? (?:love|feelings)|pledg(?:es?|ed) (?:loyalty|allegiance))/i, category: 'relationship' },
  // Binding decisions/oaths
  { pattern: /(?:i (?:promise|swear|vow|pledge)|the pact is sealed)/i, category: 'decision' },
  // World-altering revelations
  { pattern: /(?:the (?:truth|secret|prophecy) (?:is|was)|reveals? (?:that|the truth)|confess(?:es)? (?:that|to))/i, category: 'revelation' },
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
    if (facts.length >= 2) break; // Hard cap: max 2 from heuristics too
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
