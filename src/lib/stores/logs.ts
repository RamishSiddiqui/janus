// ============================================================
//   Janus — Frontend Log Capture
//   Monkey-patches console.* once at startup so Settings > Logging can show
//   frontend activity without needing devtools open, and Export can bundle
//   it together with the backend's persisted log file. Existing
//   console.log/warn/error/debug call sites throughout the app need no
//   changes — this wraps the global console object itself.
// ============================================================

import { writable, get } from 'svelte/store';

export interface FrontendLogEntry {
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
}

// Bounded so a noisy session can't grow this unboundedly in memory — old
// entries just fall off the front once the cap is hit.
const MAX_ENTRIES = 2000;

export const frontendLogs = writable<FrontendLogEntry[]>([]);

let initialized = false;

function stringifyArg(a: unknown): string {
  if (typeof a === 'string') return a;
  if (a instanceof Error) return `${a.name}: ${a.message}`;
  try {
    return JSON.stringify(a);
  } catch {
    return String(a);
  }
}

function push(level: FrontendLogEntry['level'], args: unknown[]) {
  const message = args.map(stringifyArg).join(' ');
  frontendLogs.update((entries) => {
    const next = entries.length >= MAX_ENTRIES ? entries.slice(entries.length - MAX_ENTRIES + 1) : entries;
    return [...next, { timestamp: Date.now(), level, message }];
  });
}

/** Wires the console.* capture — idempotent, safe to call from every page
 *  that mounts the root layout without double-patching. */
export function initFrontendLogCapture() {
  if (initialized || typeof window === 'undefined') return;
  initialized = true;

  const original = {
    log: console.log.bind(console),
    debug: console.debug.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
  };

  console.log = (...args: unknown[]) => { push('info', args); original.log(...args); };
  console.debug = (...args: unknown[]) => { push('debug', args); original.debug(...args); };
  console.warn = (...args: unknown[]) => { push('warn', args); original.warn(...args); };
  console.error = (...args: unknown[]) => { push('error', args); original.error(...args); };

  window.addEventListener('error', (e) => {
    push('error', [`Uncaught error: ${e.message} (${e.filename}:${e.lineno}:${e.colno})`]);
  });
  window.addEventListener('unhandledrejection', (e) => {
    push('error', ['Unhandled promise rejection:', stringifyArg(e.reason)]);
  });
}

export function clearFrontendLogs() {
  frontendLogs.set([]);
}

function formatEntry(e: FrontendLogEntry): string {
  const iso = new Date(e.timestamp).toISOString();
  return `[${iso}] ${e.level.toUpperCase().padEnd(5)} ${e.message}`;
}

export function formatFrontendLogsAsText(entries?: FrontendLogEntry[]): string {
  return (entries ?? get(frontendLogs)).map(formatEntry).join('\n');
}
