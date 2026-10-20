// ============================================================
//   Janus — App update state
//   Checks the release feed, downloads a signed update and
//   restarts into it. Desktop only; see issue #139.
// ============================================================

import { get, writable } from 'svelte/store';
import { browser } from '$app/environment';
import { settings } from '$lib/stores/settings';
import { isWebMode } from '$lib/services/webBridge';
import {
  CHECK_INTERVAL_MS,
  FIRST_CHECK_DELAY_MS,
  canAutoCheck,
  downloadFraction,
  readDismissed,
  updateErrorMessage,
  updatesBlockedInDev,
  writeDismissed,
} from '$lib/utils/updates';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'uptodate'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'error';

export interface UpdateState {
  status: UpdateStatus;
  /** The new version, when one is available. */
  version: string | null;
  /** The release notes of that version. */
  notes: string | null;
  /** 0..1 while downloading, null when the size is unknown. */
  progress: number | null;
  error: string | null;
  /** When the last check finished, in ms since the epoch. */
  lastChecked: number | null;
  /** True once the person chose "Later" for `version`; the prompt stays hidden for it. */
  dismissed: boolean;
}

const initial: UpdateState = {
  status: 'idle',
  version: null,
  notes: null,
  progress: null,
  error: null,
  lastChecked: null,
  dismissed: false,
};

export const updates = writable<UpdateState>({ ...initial });

/** The plugin's handle for the pending update; it carries the download methods. */
let pending: { downloadAndInstall: (cb: (e: DownloadEvent) => void) => Promise<void> } | null = null;

type DownloadEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' };

const isDesktop = () => browser && '__TAURI_INTERNALS__' in window && !isWebMode();

/** Looks for a newer version. `manual` checks report "up to date" and errors; automatic ones stay quiet. */
export async function checkForUpdates(manual = false): Promise<void> {
  if (!isDesktop()) return;
  if (updatesBlockedInDev(import.meta.env.DEV, import.meta.env.VITE_JANUS_UPDATES)) {
    if (manual) {
      updates.update((s) => ({ ...s, status: 'error', error: 'Updates are turned off in development builds.' }));
    }
    return;
  }
  const current = get(updates);
  if (current.status === 'checking' || current.status === 'downloading' || current.status === 'installing') return;

  updates.update((s) => ({ ...s, status: 'checking', error: null }));
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const found = await check();
    const now = Date.now();
    if (!found) {
      pending = null;
      updates.update((s) => ({ ...s, status: 'uptodate', version: null, notes: null, lastChecked: now }));
      return;
    }
    pending = found;
    const dismissed = readDismissed(localStorage) === found.version;
    updates.update((s) => ({
      ...s,
      status: 'available',
      version: found.version,
      notes: found.body ?? null,
      lastChecked: now,
      dismissed: manual ? false : dismissed,
    }));
  } catch (err) {
    console.warn('[updates] check failed:', err);
    updates.update((s) => ({
      ...s,
      status: manual ? 'error' : 'idle',
      error: manual ? updateErrorMessage(err) : null,
      lastChecked: Date.now(),
    }));
  }
}

/** Downloads the pending update, installs it and restarts Janus. */
export async function installUpdate(): Promise<void> {
  if (!pending) return;
  let total: number | undefined;
  let downloaded = 0;
  updates.update((s) => ({ ...s, status: 'downloading', progress: null, error: null }));
  try {
    await pending.downloadAndInstall((e) => {
      if (e.event === 'Started') {
        total = e.data.contentLength;
      } else if (e.event === 'Progress') {
        downloaded += e.data.chunkLength;
        updates.update((s) => ({ ...s, progress: downloadFraction(downloaded, total) }));
      } else {
        updates.update((s) => ({ ...s, status: 'installing', progress: 1 }));
      }
    });
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (err) {
    console.warn('[updates] install failed:', err);
    updates.update((s) => ({ ...s, status: 'error', error: updateErrorMessage(err) }));
  }
}

/** "Later": hides the prompt for this version (Settings still offers it). */
export function dismissUpdate(): void {
  const { version } = get(updates);
  if (version && browser) writeDismissed(localStorage, version);
  updates.update((s) => ({ ...s, dismissed: true }));
}

let timer: ReturnType<typeof setInterval> | null = null;
let first: ReturnType<typeof setTimeout> | null = null;

/** Starts the quiet background checks. Safe to call once at startup. */
export function startAutoUpdateChecks(): void {
  stopAutoUpdateChecks();
  const run = () => {
    const s = get(settings);
    if (canAutoCheck({ enabled: s.checkForUpdates, isDesktop: isDesktop(), isDev: updatesBlockedInDev(import.meta.env.DEV, import.meta.env.VITE_JANUS_UPDATES) })) {
      void checkForUpdates(false);
    }
  };
  first = setTimeout(run, FIRST_CHECK_DELAY_MS);
  timer = setInterval(run, CHECK_INTERVAL_MS);
}

export function stopAutoUpdateChecks(): void {
  if (first) clearTimeout(first);
  if (timer) clearInterval(timer);
  first = null;
  timer = null;
}
