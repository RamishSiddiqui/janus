/** Pure helpers for the in-app update prompt. */

/** How long to wait after launch before the first automatic check, so startup stays quick. */
export const FIRST_CHECK_DELAY_MS = 15_000;

/** Time between automatic checks while the app stays open. */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

/** Whether an automatic check should run at all. Manual checks ignore this. */
export function canAutoCheck(opts: {
  enabled: boolean;
  isDesktop: boolean;
  isDev: boolean;
}): boolean {
  return opts.enabled && opts.isDesktop && !opts.isDev;
}

/** Release notes are Markdown from the changelog; show them as plain lines. */
export function releaseNotesLines(notes: string | null | undefined, max = 8): string[] {
  if (!notes) return [];
  return notes
    .split(/\r?\n/)
    .map((line) => line.replace(/^\s*(#{1,6}\s*|[-*+]\s+)/, '').replace(/\*\*(.+?)\*\*/g, '$1').trim())
    .filter((line) => line.length > 0)
    .slice(0, max);
}

/** 0..1 download progress, or null while the size is unknown. */
export function downloadFraction(downloaded: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null;
  return Math.min(1, Math.max(0, downloaded / total));
}

/** A short, readable reason for a failed check or install. */
export function updateErrorMessage(err: unknown): string {
  const raw = err instanceof Error ? err.message : typeof err === 'string' ? err : '';
  const text = raw.toLowerCase();
  if (text.includes('network') || text.includes('dns') || text.includes('connect') || text.includes('error sending request')) {
    return 'Could not reach the update server. Check your internet connection and try again.';
  }
  if (text.includes('signature')) {
    return 'The update could not be verified, so it was not installed.';
  }
  if (text.includes('404') || text.includes('not found') || text.includes('valid release json')) {
    return 'No update information is published yet.';
  }
  return raw ? `Update failed: ${raw}` : 'Update failed. Please try again.';
}

/** The version the user chose to skip for now, kept in localStorage. */
const DISMISSED_KEY = 'janus.update.dismissed';

export function readDismissed(storage: Pick<Storage, 'getItem'> | null): string | null {
  try {
    return storage?.getItem(DISMISSED_KEY) ?? null;
  } catch {
    return null;
  }
}

export function writeDismissed(storage: Pick<Storage, 'setItem'> | null, version: string): void {
  try {
    storage?.setItem(DISMISSED_KEY, version);
  } catch {
    /* private mode or blocked storage: the prompt just shows again next launch */
  }
}
