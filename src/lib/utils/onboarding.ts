/** Pure helpers for the first-run setup guide. */

export interface ModelSource {
  id: 'ollama' | 'open_router' | 'puter' | 'open_ai_compatible';
  label: string;
  blurb: string;
  adapter: string;
  needsKey: boolean;
  needsBaseUrl: boolean;
  defaultBaseUrl: string;
  keyHint: string;
}

export const MODEL_SOURCES: ModelSource[] = [
  {
    id: 'ollama',
    label: 'Ollama (on this computer)',
    blurb: 'Free and private. No account, but you need Ollama installed and a model pulled.',
    adapter: 'ollama',
    needsKey: false,
    needsBaseUrl: true,
    defaultBaseUrl: 'http://localhost:11434',
    keyHint: '',
  },
  {
    id: 'open_router',
    label: 'OpenRouter',
    blurb: 'One API key, many models. Some are free, marked ":free".',
    adapter: 'open_router',
    needsKey: true,
    needsBaseUrl: false,
    defaultBaseUrl: '',
    keyHint: 'Create a key at openrouter.ai/keys',
  },
  {
    id: 'puter',
    label: 'Puter',
    blurb: 'Free models through a personal Puter token.',
    adapter: 'open_ai_compatible',
    needsKey: true,
    needsBaseUrl: false,
    defaultBaseUrl: 'https://api.puter.com/puterai/openai/v1/',
    keyHint: 'puter.com/dashboard, Account, Create token',
  },
  {
    id: 'open_ai_compatible',
    label: 'Other (OpenAI-compatible)',
    blurb: 'LM Studio, a self-hosted server, or any service with an OpenAI-style API.',
    adapter: 'open_ai_compatible',
    needsKey: false,
    needsBaseUrl: true,
    defaultBaseUrl: 'http://localhost:1234/v1',
    keyHint: 'Leave empty if the server does not need a key',
  },
];

/** Free models first (OpenRouter marks them ":free"), then alphabetical. */
export function rankModels(models: string[]): string[] {
  const seen = new Set<string>();
  const unique = models.filter((m) => m && !seen.has(m) && seen.add(m));
  return unique.sort((a, b) => {
    const fa = a.endsWith(':free') ? 0 : 1;
    const fb = b.endsWith(':free') ? 0 : 1;
    return fa - fb || a.localeCompare(b);
  });
}

/**
 * The guide appears once, for someone with no chat provider yet. Anyone who
 * already has one is treated as set up, so upgrading never nags them.
 */
export function shouldShowWizard(onboardingDone: boolean, llmProviderCount: number): boolean {
  return !onboardingDone && llmProviderCount === 0;
}
