// ============================================================
//   Janus — Provider Error Humanizer
// ============================================================
//
// Backend generation-failure messages are a Rust error's `Display` output,
// which for a provider/completion failure wraps the raw JSON body from the
// upstream API verbatim (sometimes double-JSON-encoded — OpenRouter nests
// the real upstream provider's own error as a JSON *string* inside its own
// error's `metadata.raw` field). Shown as-is, that's an unreadable wall of
// escaped JSON. This extracts the actual human-relevant bits (which
// provider, what went wrong) and falls back to a cleaned-up version of the
// raw string when nothing recognizable is found. Never throws.

interface ParsedProviderError {
  message?: string;
  code?: number | string;
  metadata?: {
    provider_name?: string;
    provider_error_code?: string | number;
    raw?: string;
  };
}

export function humanizeProviderError(raw: string): string {
  if (!raw) return 'Unknown error.';

  const jsonStart = raw.indexOf('{');
  if (jsonStart === -1) return stripNoise(raw);

  try {
    const parsed = JSON.parse(raw.slice(jsonStart));
    return summarizeErrorObject(parsed) ?? stripNoise(raw);
  } catch {
    return stripNoise(raw);
  }
}

function summarizeErrorObject(parsed: { error?: ParsedProviderError }): string | null {
  const err = parsed?.error;
  if (!err) return null;

  const providerName = err.metadata?.provider_name;
  const code = err.code ?? err.metadata?.provider_error_code;

  // The real upstream error is often nested as a JSON *string* in
  // metadata.raw — unwrap it for the actual message when present.
  let innerMessage: string | undefined;
  if (typeof err.metadata?.raw === 'string') {
    try {
      const inner = JSON.parse(err.metadata.raw);
      innerMessage = inner?.error?.message ?? inner?.message;
    } catch {
      // Not JSON — fall through to the outer message.
    }
  }

  const message = innerMessage || err.message || 'Request failed';
  const prefix = providerName ? `${providerName} rejected the request` : 'Provider error';
  return code ? `${prefix} (${code}): ${message}` : `${prefix}: ${message}`;
}

function stripNoise(raw: string): string {
  const cleaned = raw
    .replace(/^CompletionError:\s*/i, '')
    .replace(/^ProviderError:\s*/i, '')
    .trim();
  return cleaned.length > 240 ? cleaned.slice(0, 240) + '…' : cleaned;
}
