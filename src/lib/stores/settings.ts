// ============================================================
//   Janus — Persisted Settings Store
// ============================================================

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

type Theme = 'dark' | 'light' | 'system';

export interface AppSettings {
  theme: Theme;
  fontSize: string;
  streamingEnabled: boolean;
  /** Whether to show the collapsible "Thinking" section for reasoning-model
   *  chain-of-thought. When off, reasoning is discarded from the UI entirely. */
  showThinking: boolean;
  autoGenerateImages: boolean;
  /** Sent as AI Horde's `nsfw` request flag for scene generation — NOT "make
   *  everything explicit," but "don't route through the safety filter that
   *  censors anything a worker's classifier flags as NSFW-ish." Defaults on
   *  since ordinary (non-explicit) roleplay character descriptions routinely
   *  brush that classifier's threshold and get false-positive blocked. */
  allowMatureContent: boolean;
  /** Auto-generates a portrait for each auto-detected NPC via the configured
   *  image provider. A no-op if no image provider is configured. */
  autoGenerateNpcPortraits: boolean;
  /** When on, an auto-generated NPC portrait is used immediately; when off,
   *  it sits in a pending-review state until approved/regenerated/rejected. */
  autoApproveNpcPortraits: boolean;
  autoSaveMemories: boolean;
  localStorageOnly: boolean;
  systemPrompt: string;
  /** Post-History Instructions — injected AFTER conversation history, before generation.
   *  Shapes how the AI ends responses (narrative hooks, pacing, tone). */
  postHistoryInstructions: string;
  /** System instructions for the "Refresh from Story" character-profile
   *  refinement pass (manual button + automatic still-placeholder trigger).
   *  Must keep the response contract intact — the app parses the model's
   *  reply as JSON with exactly `description`/`personality`/`scenario`
   *  fields, so an edit that drops that instruction will break refreshes. */
  profileRefreshPrompt: string;
  /** Maximum context window size in tokens. Should match the model's limit. */
  maxContextTokens: number;
  /** Whether to auto-generate rolling summaries of evicted messages. */
  autoSummarize: boolean;
  /** Whether vector RAG (semantic memory) is enabled. */
  ragEnabled: boolean;
  /** Embedding model to use for RAG. */
  ragEmbeddingModel: string;
  /** Number of top results to retrieve from vector search. */
  ragTopK: number;
  /** Minimum cosine similarity threshold for RAG results. */
  ragMinSimilarity: number;
  /** Internal version counter — bumped when managed defaults (PHI, systemPrompt) change.
   *  When saved version < current version, stale prompts are refreshed automatically. */
  _settingsVersion: number;
}

const STORAGE_KEY = 'mythic-settings';

// ── Bump this whenever systemPrompt or postHistoryInstructions defaults change ──
const CURRENT_SETTINGS_VERSION = 2;

const defaultSettings: AppSettings = {
  theme: 'dark',
  fontSize: 'Medium',
  streamingEnabled: true,
  showThinking: true,
  autoGenerateImages: true,
  allowMatureContent: true,
  autoGenerateNpcPortraits: true,
  autoApproveNpcPortraits: false,
  autoSaveMemories: false,
  localStorageOnly: true,
  _settingsVersion: CURRENT_SETTINGS_VERSION,
  maxContextTokens: 16384,
  autoSummarize: true,
  ragEnabled: false,
  ragEmbeddingModel: 'openai/text-embedding-3-small',
  ragTopK: 5,
  ragMinSimilarity: 0.7,
  systemPrompt: `You are {{char}}, a character in an immersive roleplay. Stay in character at all times. Use vivid, descriptive prose with *actions* in asterisks. Never break the fourth wall. Respond naturally to the user's actions and advance the narrative.`,
  postHistoryInstructions: `[Narrative Direction — MANDATORY]
RULE: Never write a response that ends the scene without beginning the next one. Farewells are scene TRANSITIONS, not endings. If a goodbye, departure, time-skip, or scene conclusion occurs, you MUST continue writing past it into the next scene within the same response.

When a scene is ending, your response MUST follow this structure:
1. The farewell or departure moment (keep it brief)
2. A transition beat — time passing, location shifting, atmosphere changing
3. The opening of the NEXT scene — new setting, new action, or new tension beginning

Never end on a closing atmospheric image (e.g. "silence settled", "the door closed", "footsteps fading"). Always end mid-action in a NEW scene with unresolved momentum.

Show, don't tell — weave all hooks and transitions into the prose naturally without breaking character.`,
  profileRefreshPrompt: `You are refining an existing roleplay character's profile using how they've actually appeared in the story so far. You'll be given their CURRENT profile, known facts about them established in the story (both settled canon and things that happened in this conversation), recent story dialogue/narration to infer voice and mannerisms from, and the existing cast for consistency.

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
- Output ONLY the JSON object — no markdown fences, no commentary, no <think> preamble.`,
};

// These are the fields that get force-refreshed when _settingsVersion is bumped.
// Everything else (theme, fontSize, etc.) is always preserved from the user's save.
const MANAGED_PROMPT_KEYS: (keyof AppSettings)[] = [
  'systemPrompt',
  'postHistoryInstructions',
  'profileRefreshPrompt',
];

function loadSettings(): AppSettings {
  if (!browser) return defaultSettings;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const saved = JSON.parse(raw) as Partial<AppSettings>;
      const merged = { ...defaultSettings, ...saved };

      // ── Version migration ──
      // If the saved version is older (or missing), force-refresh managed prompts
      // so the user gets updated PHI/systemPrompt defaults.
      const savedVersion = saved._settingsVersion ?? 0;
      if (savedVersion < CURRENT_SETTINGS_VERSION) {
        for (const key of MANAGED_PROMPT_KEYS) {
          (merged as any)[key] = defaultSettings[key];
        }
        merged._settingsVersion = CURRENT_SETTINGS_VERSION;
        // Persist the migration immediately
        localStorage.setItem(STORAGE_KEY, JSON.stringify(merged));
        console.info(
          `[settings] Migrated v${savedVersion} → v${CURRENT_SETTINGS_VERSION}: refreshed managed prompts`
        );
      }

      return merged;
    }
  } catch {
    // Corrupted storage — fall back to defaults
  }
  return defaultSettings;
}

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(loadSettings());

  // Persist on every change
  subscribe((value) => {
    if (browser) {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
      } catch {
        // Storage full or unavailable
      }
    }
  });

  return {
    subscribe,
    set,
    update,
    reset: () => set(defaultSettings),
  };
}

export const settings = createSettingsStore();

/**
 * Privacy guard — returns true when the user has opted into local-only mode.
 * Other modules should call this before making any external requests
 * beyond the core LLM provider (e.g. analytics, telemetry, cloud sync).
 */
export function isLocalOnly(): boolean {
  let value = true;
  settings.subscribe(s => { value = s.localStorageOnly; })();
  return value;
}
