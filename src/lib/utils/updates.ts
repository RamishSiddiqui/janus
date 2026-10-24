/** Pure helpers for the in-app update prompt. */

/** How long to wait after launch before the first automatic check, so startup stays quick. */
export const FIRST_CHECK_DELAY_MS = 15_000;

/** Time between automatic checks while the app stays open. */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

/** Whether an automatic check should run at all. Manual checks ignore this. */
export function canAutoCheck(opts: { enabled: boolean; isDesktop: boolean; isDev: boolean }): boolean {
  return opts.enabled && opts.isDesktop && !opts.isDev;
}

/**
 * Development builds (`tauri dev`) never install updates: their version is
 * whatever the branch says, and replacing a dev build with a release would be
 * wrong. A developer testing the update flow sets `VITE_JANUS_UPDATES=1`.
 */
export function updatesBlockedInDev(isDev: boolean, override: string | undefined): boolean {
  return isDev && override !== '1';
}

export interface NotesGroup {
  /** The changelog section ("Fixed", "Build", ...), or null for lines before any heading. */
  heading: string | null;
  items: string[];
}

const plain = (text: string) => text.replace(/\*\*(.+?)\*\*/g, '$1').trim();

/**
 * Turns the release's Markdown changelog into headed groups of bullets. The
 * leading "[0.1.4] - date" line is dropped, because the card already names the
 * version. At most `maxItems` bullets are kept.
 */
export function releaseNotesGroups(notes: string | null | undefined, maxItems = 12): NotesGroup[] {
  if (!notes) return [];
  const groups: NotesGroup[] = [];
  let current: NotesGroup = { heading: null, items: [] };
  let count = 0;
  const flush = () => {
    if (current.items.length) groups.push(current);
  };
  for (const raw of notes.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const heading = line.match(/^#{1,6}\s*(.+)$/);
    if (heading) {
      const title = plain(heading[1]);
      if (/^\[?v?\d+\.\d+\.\d+/.test(title)) continue;
      flush();
      current = { heading: title, items: [] };
      continue;
    }
    if (count >= maxItems) break;
    const item = plain(line.replace(/^[-*+]\s+/, ''));
    if (item) {
      current.items.push(item);
      count++;
    }
  }
  flush();
  return groups;
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
  if (
    text.includes('network') ||
    text.includes('dns') ||
    text.includes('connect') ||
    text.includes('error sending request')
  ) {
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
