// ============================================================
//   Janus — Blob URL Loading
//   Shared "read a file from AppData, wrap it as a blob: URL"
//   primitive. Previously duplicated ~8 times across avatar,
//   scene, and gallery loading code (each with its own
//   extension-to-MIME guess and inconsistent cleanup, which is
//   how the scene-image blob leak happened — see SceneDisplay).
// ============================================================

const MIME_BY_EXT: Record<string, string> = {
  png: 'image/png',
  webp: 'image/webp',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
};

/** Best-effort MIME guess from a file's extension — defaults to JPEG for
 *  unknown/missing extensions, matching every prior duplicate of this logic. */
export function mimeForPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return MIME_BY_EXT[ext] ?? 'image/jpeg';
}

/** Reads a file (relative to AppData) and returns a blob: URL for it.
 *  Throws on failure — callers decide how to handle that (return null,
 *  log, etc.), since prior duplicates disagreed on this.
 *
 *  This only creates the URL; it does NOT revoke anything. Callers that
 *  replace a single blob URL over time (an avatar, the current scene image)
 *  must revoke the previous one themselves — see `revokeIfSet`. Callers
 *  that cache by path (e.g. chat.ts's avatarCache) should never revoke,
 *  since the same URL may still be referenced elsewhere. */
export async function loadFileAsBlobUrl(relativePath: string, mimeType?: string): Promise<string> {
  const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
  const bytes = await readFile(relativePath, { baseDir: BaseDirectory.AppData });
  const blob = new Blob([bytes], { type: mimeType ?? mimeForPath(relativePath) });
  return URL.createObjectURL(blob);
}

/** Revokes `url` if it's set — a one-liner, but makes call sites read as
 *  "revoke the old one" instead of a bare `if` that's easy to skip when
 *  copy-pasting this pattern (which is exactly how the leaks happened). */
export function revokeIfSet(url: string | null | undefined): void {
  if (url) URL.revokeObjectURL(url);
}
