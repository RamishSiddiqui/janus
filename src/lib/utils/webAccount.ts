/** Pure helpers for the browser sign-in screens. */

export const LEVELS = ['Too short', 'Weak', 'Okay', 'Strong', 'Excellent'] as const;

/** 0 (too short) to 4 (excellent), judged mostly by length. */
export function passphraseStrength(p: string): number {
  const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter(r => r.test(p)).length;
  const n = p.length;
  let s = 0;
  if (n >= 8) s = 1;
  if (n >= 12) s = 2;
  if (n >= 16 && classes >= 2) s = 3;
  if (n >= 20 || (n >= 16 && classes >= 3)) s = 4;
  return s;
}

/** 3 to 24 characters: letters, numbers, dot, underscore, dash. */
export function usernameOk(u: string): boolean {
  return /^[a-z0-9][a-z0-9._-]{2,23}$/.test(u);
}

export const MIN_PASSPHRASE = 12;

/** The 16 key characters typed so far, without the JANUS prefix or dashes. */
export function cleanRecoveryKey(v: string): string {
  let raw = v.toUpperCase().replace(/[^A-Z0-9]/g, '');
  if (raw.startsWith('JANUS')) raw = raw.slice(5);
  return raw.slice(0, 16);
}

/** Shows typed key characters as JANUS-XXXX-XXXX-XXXX-XXXX. */
export function formatRecoveryKey(raw: string): string {
  const groups = raw.match(/.{1,4}/g) ?? [];
  return groups.length ? 'JANUS-' + groups.join('-') : '';
}

/** "now", "5 minutes ago", ... for a Unix time given in seconds (or milliseconds). */
export function agoLabel(timestamp: number, nowMs: number = Date.now()): string {
  const ms = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const secs = Math.max(0, Math.round((nowMs - ms) / 1000));
  if (secs < 90) return 'now';
  if (secs < 5400) return `${Math.round(secs / 60)} minutes ago`;
  if (secs < 129600) return `${Math.round(secs / 3600)} hours ago`;
  return `${Math.round(secs / 86400)} days ago`;
}
