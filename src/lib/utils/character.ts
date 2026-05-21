// ============================================================
//   Mythic — Character Data Parser
//   Safely extracts CharacterData whether it comes as a
//   JSON string or a native object from SurrealDB/Tauri IPC.
// ============================================================

/**
 * Parses character card data from the backend `data` field.
 *
 * SurrealDB stores `data` as a native JSON object, so Tauri IPC
 * delivers it as a plain JS object — NOT a string. However, imported
 * character cards (PNG) may store `data` as a JSON string.
 *
 * This helper handles both cases and always returns a usable object.
 */
export function parseCharacterData(raw: unknown): Record<string, unknown> {
  if (!raw) return {};

  // Already an object (normal case from SurrealDB)
  if (typeof raw === 'object' && !Array.isArray(raw)) {
    return raw as Record<string, unknown>;
  }

  // String (possible import / legacy data)
  if (typeof raw === 'string') {
    try {
      const parsed = JSON.parse(raw);
      if (typeof parsed === 'object' && parsed !== null) return parsed;
    } catch {
      // Not valid JSON
    }
    return {};
  }

  return {};
}
