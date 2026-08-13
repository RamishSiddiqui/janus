// ============================================================
//   Janus — Memory Auto-Extraction Pipeline
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
  /** Who this fact is primarily about — an exact name from the CAST list
   *  passed in the prompt, "the user" if it's about the player, or omitted
   *  for a general world fact not tied to one person. Used to attribute the
   *  saved memory to the right character instead of always the primary —
   *  see the matching resolution logic in `extractAndSaveMemories`. */
  character?: string | null;
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
- For each fact, identify WHO it's primarily about using the "character" field: the EXACT name from the CAST list below if it matches one of them (including a secondary/NPC character, not just the main one), "the user" if it's about the player, or omit the field for a general world fact not tied to one specific person. Get this right — a fact filed under the wrong character becomes invisible to that character later.

OUTPUT FORMAT (strict JSON array, no markdown fencing, no explanation):
[{"summary":"...","category":"<short tag, e.g. identity, relationship, world, decision, revelation, secret, promise, or any fitting label>","character":"<exact CAST name, \"the user\", or omit>"}]

Expected output for most messages: []`;

/**
 * Build the user prompt for the extraction LLM call.
 * Includes existing memories so the LLM avoids duplicates.
 */
function buildExtractionPrompt(
  userMessage: string,
  assistantResponse: string,
  existingMemories?: string[],
  castNames?: string[],
): string {
  let prompt = `Analyze this roleplay exchange. Extract ONLY facts that would cause a continuity error if forgotten in a future session.\n\n`;

  if (castNames && castNames.length > 0) {
    prompt += `CAST (use one of these exact names for a fact's "character" field, or "the user"):\n${castNames.join(', ')}\n\n`;
  }

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
        character: typeof item.character === 'string' ? item.character.trim() : null,
      }));
  } catch {
    console.warn('[Janus] Failed to parse extraction response:', raw.slice(0, 200));
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
    console.debug(`[Janus] Memory extraction triggered (message #${messageCounter})`);
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
/**
 * Resolves a name the extraction LLM stated a fact was about to an actual
 * cast member's character ID — same exact/case-insensitive/first-name/
 * substring fallback chain as the Rust-side `resolve_character_id` (see
 * `context/response_parser.rs`), kept in lockstep so a name that would
 * resolve during live multi-character response parsing also resolves here.
 */
function resolveFactCharacterId(name: string, castPairs: [string, string][]): string | undefined {
  const exact = castPairs.find(([n]) => n === name);
  if (exact) return exact[1];
  const lower = name.toLowerCase();
  const caseInsensitive = castPairs.find(([n]) => n.toLowerCase() === lower);
  if (caseInsensitive) return caseInsensitive[1];
  const firstName = castPairs.find(([n]) => n.split(/\s+/)[0]?.toLowerCase() === lower);
  if (firstName) return firstName[1];
  const substring = castPairs.find(([n]) => n.toLowerCase().includes(lower) || lower.includes(n.toLowerCase()));
  return substring?.[1];
}

/**
 * Builds (name, id) pairs for the conversation's full cast — the primary
 * character (not itself a `conversation_characters` row — see the matching
 * comment in pipeline.rs) plus every secondary/NPC member. Used both to
 * give the extraction LLM a list of valid names and to resolve its answer
 * back to a real character ID.
 */
async function buildCastPairs(
  ipc: typeof import('$lib/services/ipc'),
  conversationId: string,
  primaryCharacterId: string,
): Promise<[string, string][]> {
  const pairs: [string, string][] = [];
  try {
    const primary = await ipc.getCharacter(primaryCharacterId);
    pairs.push([primary.name, primaryCharacterId]);
  } catch {
    // Best-effort — extraction still works with an incomplete cast list
  }
  try {
    const cast = await ipc.listConversationCharacters(conversationId);
    for (const c of cast) {
      if (!pairs.some(([, id]) => id === c.character_id)) {
        pairs.push([c.character_name, c.character_id]);
      }
    }
  } catch {
    // Best-effort
  }
  return pairs;
}

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

  // Cast list so each fact can be attributed to whichever character it's
  // actually about, instead of always the conversation's primary — see the
  // "why doesn't Refresh from Story pick up an NPC's backstory" writeup:
  // without this, e.g. a maid character's revealed backstory got saved
  // under the *primary* character's ID and was invisible when refreshing
  // the maid's own profile, since that only looks at her own memories.
  // Only attempted in 'character' memory-scope mode (characterId present)
  // — 'conversation' scope intentionally keeps memories un-attributed.
  const castPairs = characterId ? await buildCastPairs(ipc, conversationId, characterId) : [];

  // --- Tier 1: LLM-powered extraction ---
  try {
    const raw = await ipc.generateRaw(
      EXTRACTION_SYSTEM_PROMPT,
      buildExtractionPrompt(userMessage, assistantResponse, existingMemoryTexts, castPairs.map(([name]) => name)),
      undefined, // use default model
      256,       // max tokens — responses are short (usually just [])
      0.1,       // very low temperature for consistent, conservative output
    );
    facts = parseExtractionResponse(raw);
    if (facts.length > 0) {
      console.debug(`[Janus] LLM extracted ${facts.length} fact(s):`, facts.map(f => f.summary));
    }
  } catch (err) {
    console.warn('[Janus] LLM extraction failed, falling back to heuristics:', err);
  }

  // --- Tier 2: Heuristic fallback ---
  // Only triggers on high-signal patterns (identity reveals, oaths, confessions)
  if (facts.length === 0) {
    facts = heuristicExtract(assistantResponse);
    if (facts.length > 0) {
      console.debug(`[Janus] Heuristic extracted ${facts.length} fact(s)`);
    }
  }

  if (facts.length === 0) return 0;

  // --- Save to backend ---
  let saved = 0;
  for (const fact of facts) {
    // Resolve to the specific cast member the fact is about; fall back to
    // the primary (old behavior) when the LLM didn't name anyone resolvable
    // — e.g. a general world fact, or the heuristic tier (which never sets
    // `character` at all).
    const resolvedId = fact.character ? resolveFactCharacterId(fact.character, castPairs) : undefined;
    const targetCharacterId = resolvedId ?? characterId ?? undefined;
    try {
      await ipc.createMemory(
        `[${fact.category}] ${fact.summary}`,
        targetCharacterId,
        conversationId,
        'auto',
      );
      saved++;
    } catch (err) {
      console.warn('[Janus] Failed to save auto-memory:', err);
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
