// ============================================================
//   Janus — Centralized Error Handling
//   Logs errors to console AND shows user-facing toast
// ============================================================

import { error as toastError } from '$lib/stores/toast';

/**
 * Extracts a readable message from any error type.
 * Handles Tauri IPC error strings, Error objects, and unknown values.
 */
export function extractErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  if (typeof err === 'object' && err !== null) {
    const obj = err as Record<string, unknown>;
    // MythicError serializes as { error: "variant", message: "human-readable" }
    if (typeof obj.message === 'string') return obj.message;
    // Nested Tauri error wrapping
    if (typeof obj.error === 'string' && typeof obj.message === 'undefined') return obj.error;
    // SurrealDB errors may nest further
    if (typeof obj.Database === 'string') return obj.Database;
    if (typeof obj.Db === 'object' && obj.Db !== null) {
      const db = obj.Db as Record<string, unknown>;
      if (typeof db.Serialization === 'string') return `Deserialization: ${db.Serialization}`;
      return JSON.stringify(obj.Db);
    }
    try { return JSON.stringify(err); } catch { return String(err); }
  }
  return String(err);
}

/**
 * Handles an IPC error: logs to console + shows toast with context.
 * Usage: `} catch (err) { handleIpcError('load providers', err); }`
 */
export function handleIpcError(action: string, err: unknown): void {
  const message = extractErrorMessage(err);
  console.error(`[Mythic IPC] Failed to ${action}:`, err);
  toastError(`Failed to ${action}: ${message}`);
}
