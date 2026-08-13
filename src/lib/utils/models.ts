// ============================================================
//   Janus — Model Table Display Utilities
//   Shared by the LLM Models and Embedding Models pages, which
//   render the same ModelEntry data in the same table layout.
// ============================================================

/** Parses a per-token price string for sorting; unparseable/missing prices sort last. */
export function priceNum(s: string | null): number {
  if (!s) return 999;
  const n = parseFloat(s);
  return isNaN(n) ? 999 : n;
}

/** Formats a per-token price string as a per-million-token USD label. */
export function formatPrice(s: string | null): string {
  if (!s || s === '0') return 'Free';
  const perToken = parseFloat(s);
  if (isNaN(perToken)) return '—';
  const perMillion = perToken * 1_000_000;
  if (perMillion < 0.01) return '<$0.01';
  return `$${perMillion.toFixed(2)}`;
}

/** Formats a token count as a compact label (e.g. 128000 -> "128K"). */
export function ctxLabel(n: number | null): string {
  if (!n) return '—';
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${(n / 1000).toFixed(0)}K` : String(n);
}

/** Context length as a percentage of the widest context in the current list, for the bar chart. */
export function ctxPercent(n: number | null, maxCtx: number): number {
  if (!n || !maxCtx) return 0;
  return Math.min((n / maxCtx) * 100, 100);
}

/** Strips the "org/" prefix from a model ID for display (e.g. "openai/gpt-4o" -> "gpt-4o"). */
export function modelSlug(id: string): string {
  return id.includes('/') ? id.split('/').slice(1).join('/') : id;
}

/** Formats an embedding dimension count with thousands separators. */
export function dimLabel(n: number | null): string {
  if (!n) return '—';
  return n.toLocaleString();
}

const ADAPTER_COLORS: Record<string, string> = {
  open_router: '#8B5CF6', ollama: '#10B981',
  open_ai_compatible: '#3B82F6', openai_compatible: '#3B82F6',
  silicon_flow: '#F59E0B', anthropic: '#D97706', gemini: '#4285F4',
  deepseek: '#06B6D4', groq: '#F97316', cohere: '#8B5CF6',
  perplexity: '#10B981', xai: '#EF4444', together: '#6366F1',
  lm_studio: '#06B6D4',
};

/** Brand color for a provider adapter, used for badges across model tables. */
export function adapterColor(adapter: string): string {
  return ADAPTER_COLORS[adapter] ?? '#6b6b8a';
}
