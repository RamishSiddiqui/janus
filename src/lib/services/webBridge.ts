/**
 * Browser access to Janus (issue #94 / #14).
 *
 * When the app is opened in a normal browser through Janus's own web server,
 * there is no Tauri runtime. This module installs a small stand-in for the
 * parts of `window.__TAURI_INTERNALS__` that `@tauri-apps/api` calls, so every
 * existing `invoke()` / `listen()` in the frontend works unchanged:
 *
 *  - `invoke(cmd, args)`  ->  `POST /api/rpc/{cmd}` (same command, same JSON)
 *  - `listen(event, cb)`  ->  one shared Server-Sent Events stream, `/api/events`
 *
 * The session lives in an HttpOnly cookie the page never sees. A 401 from any
 * call raises `janus:signed-out` so the shell can show the sign-in page.
 */

export interface WebUser {
  id: string;
  username: string;
  role: 'admin' | 'member';
  must_change: boolean;
  created_at: string;
}

export interface WebStatus {
  has_users: boolean;
  signup_mode: 'open' | 'admin_only';
  user: WebUser | null;
}

export interface SignedIn {
  user: WebUser;
  /** Shown once, when a new key was just issued (sign-up, recovery). */
  recovery_key: string | null;
}

export interface WebSession {
  id: string;
  label: string;
  trusted: boolean;
  created_at: string;
  last_seen: number;
  expires_at: number;
  current: boolean;
}

export interface WebError {
  error: string;
  message: string;
}

export const SIGNED_OUT_EVENT = 'janus:signed-out';

let webMode = false;

/** True once the bridge is installed (the page is served by Janus's web server). */
export function isWebMode(): boolean {
  return webMode;
}

let initialStatus: WebStatus | null = null;

/** The account status read at startup, for the shell to pick its first screen. */
export function getInitialStatus(): WebStatus | null {
  return initialStatus;
}

/**
 * Called once at startup (SvelteKit's client `init` hook). Inside Tauri it does
 * nothing; in a browser served by Janus it installs the bridge.
 */
export async function initWebMode(): Promise<void> {
  if (typeof window === 'undefined' || '__TAURI_INTERNALS__' in window) return;
  const status = await detectWebMode();
  if (!status) return;
  initialStatus = status;
  installBridge();
}

const HEADERS = { 'content-type': 'application/json', 'x-janus-request': '1' };

/** Turns a failed response into the `{ error, message }` shape the app already handles. */
export function toWebError(status: number, text: string): WebError {
  try {
    const body = JSON.parse(text);
    if (body && typeof body.message === 'string') {
      return { error: String(body.error ?? 'unknown'), message: body.message };
    }
  } catch {
    // not JSON
  }
  return { error: 'unknown', message: `Request failed (${status}).` };
}

async function request<T>(method: string, url: string, body?: unknown): Promise<T | null> {
  const res = await fetch(url, {
    method,
    headers: HEADERS,
    credentials: 'same-origin',
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) {
    if (res.status === 401 && !url.startsWith('/api/auth/')) {
      window.dispatchEvent(new CustomEvent(SIGNED_OUT_EVENT));
    }
    throw toWebError(res.status, text);
  }
  return text ? (JSON.parse(text) as T) : null;
}

// ── account calls (used by the sign-in pages) ──────────────────────────────

export const webAuth = {
  status: () => request<WebStatus>('GET', '/api/auth/status') as Promise<WebStatus>,
  register: (username: string, passphrase: string, trusted: boolean) =>
    request<SignedIn>('POST', '/api/auth/register', { username, passphrase, trusted }) as Promise<SignedIn>,
  login: (username: string, passphrase: string, trusted: boolean) =>
    request<SignedIn>('POST', '/api/auth/login', { username, passphrase, trusted }) as Promise<SignedIn>,
  reset: (username: string, recoveryKey: string, newPassphrase: string, trusted: boolean) =>
    request<SignedIn>('POST', '/api/auth/reset', {
      username,
      recovery_key: recoveryKey,
      new_passphrase: newPassphrase,
      trusted,
    }) as Promise<SignedIn>,
  sessions: () => request<WebSession[]>('GET', '/api/auth/sessions') as Promise<WebSession[]>,
  revokeSession: async (id: string) => {
    await request('DELETE', `/api/auth/sessions/${encodeURIComponent(id)}`);
  },
  logout: async () => {
    await request('POST', '/api/auth/logout');
  },
  changePassphrase: async (current: string, next: string) => {
    await request('POST', '/api/auth/change-passphrase', { current, new: next });
  },
};

/**
 * Asks the page's own origin whether it is a Janus web server. A plain dev
 * server answers with HTML (or 404), which is not the status JSON.
 */
export async function detectWebMode(): Promise<WebStatus | null> {
  try {
    const res = await fetch('/api/auth/status', { credentials: 'same-origin' });
    if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) return null;
    const body = await res.json();
    return typeof body?.has_users === 'boolean' ? (body as WebStatus) : null;
  } catch {
    return null;
  }
}

// ── command calls ──────────────────────────────────────────────────────────

/** Runs one backend command over HTTP. Exported for tests. */
export async function rpcInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  if (cmd === 'upload_message_attachment_bytes') {
    return uploadAttachmentBytes(args ?? {});
  }
  const upload = pickedFileUpload(cmd, args ?? {});
  if (upload) return upload;
  return request('POST', `/api/rpc/${encodeURIComponent(cmd)}`, args ?? {});
}

async function uploadAttachmentBytes(args: Record<string, unknown>): Promise<unknown> {
  const bytes = args.bytes as ArrayLike<number> | undefined;
  const ext = String(args.extension ?? '').toLowerCase();
  if (!bytes || bytes.length === 0) {
    throw { error: 'validation', message: 'Pasted image is empty' } as WebError;
  }
  const res = await fetch(`/api/upload/attachment?ext=${encodeURIComponent(ext)}`, {
    method: 'POST',
    headers: { 'x-janus-request': '1' },
    credentials: 'same-origin',
    // Always copy into a fresh ArrayBuffer-backed array: a Uint8Array over an
    // ArrayBufferLike (possibly shared) buffer is not a valid fetch body type.
    body: Uint8Array.from(bytes),
  });
  const text = await res.text();
  if (!res.ok) throw toWebError(res.status, text);
  return JSON.parse(text);
}

// ── file pickers ───────────────────────────────────────────────────────────
//
// The desktop's file dialog returns a path on the host, which a browser
// doesn't have. In a browser the dialog opens the browser's own picker and
// hands back a token for the chosen File; the few commands that take a
// `filePath` then upload those bytes instead.

const PICKED_PREFIX = 'browser-file:';
const picked = new Map<string, File>();
let nextPick = 1;

interface DialogOptions {
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
}

function pickFiles(options: DialogOptions): Promise<string | string[] | null> {
  return new Promise(resolve => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = !!options.multiple;
    const exts = (options.filters ?? []).flatMap(f => f.extensions.map(e => `.${e}`));
    if (exts.length) input.accept = exts.join(',');
    input.addEventListener('change', () => {
      const files = Array.from(input.files ?? []);
      if (!files.length) return resolve(null);
      const tokens = files.map(f => {
        const token = `${PICKED_PREFIX}${nextPick++}/${f.name}`;
        picked.set(token, f);
        return token;
      });
      resolve(options.multiple ? tokens : tokens[0]);
    });
    input.addEventListener('cancel', () => resolve(null));
    input.click();
  });
}

function pickedFile(token: unknown): File {
  const f = typeof token === 'string' ? picked.get(token) : undefined;
  if (!f) throw { error: 'validation', message: 'Choose the file again.' } as WebError;
  return f;
}

async function postFile(url: string, file: File): Promise<unknown> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'x-janus-request': '1' },
    credentials: 'same-origin',
    body: file,
  });
  const text = await res.text();
  if (!res.ok) throw toWebError(res.status, text);
  return JSON.parse(text);
}

/** The commands that take a picked file's path, turned into uploads. */
function pickedFileUpload(cmd: string, args: Record<string, unknown>): Promise<unknown> | null {
  switch (cmd) {
    case 'upload_message_attachment': {
      const f = pickedFile(args.filePath);
      const ext = f.name.split('.').pop()?.toLowerCase() ?? '';
      return postFile(`/api/upload/attachment?ext=${encodeURIComponent(ext)}`, f);
    }
    case 'upload_character_avatar':
      return postFile(
        `/api/upload/portrait/${encodeURIComponent(String(args.characterId))}`,
        pickedFile(args.filePath),
      );
    case 'import_character_card':
      return postFile('/api/upload/character-card', pickedFile(args.filePath));
    case 'import_persona_card':
      return postFile('/api/upload/persona-card', pickedFile(args.filePath));
    default:
      return null;
  }
}

/** The size of a picked file, for the attachment size check. */
function statPicked(args: Record<string, unknown>) {
  const f = pickedFile(args.path);
  return {
    isFile: true,
    isDirectory: false,
    isSymlink: false,
    size: f.size,
    mtime: null,
    atime: null,
    birthtime: null,
    readonly: false,
  };
}

/** A stored file as an object URL. Throws when it isn't yours or doesn't exist. */
export async function fetchFileBlobUrl(relativePath: string, mimeType?: string): Promise<string> {
  const url = '/api/files/' + relativePath.split('/').map(encodeURIComponent).join('/');
  const res = await fetch(url, { credentials: 'same-origin' });
  if (!res.ok) throw toWebError(res.status, await res.text());
  const blob = await res.blob();
  return URL.createObjectURL(mimeType ? new Blob([blob], { type: mimeType }) : blob);
}

// ── events ─────────────────────────────────────────────────────────────────

type Handler = (e: { event: string; id: number; payload: unknown }) => void;

interface EventHub {
  source: EventSource | null;
  names: Set<string>;
  /** event name -> event id -> callback id */
  subs: Map<string, Map<number, number>>;
}

const hub: EventHub = { source: null, names: new Set(), subs: new Map() };
const callbacks = new Map<number, Handler>();
let nextId = 1;

function ensureSource(): EventSource {
  if (!hub.source) {
    hub.source = new EventSource('/api/events', { withCredentials: true });
  }
  return hub.source;
}

function attach(name: string) {
  if (hub.names.has(name)) return;
  hub.names.add(name);
  ensureSource().addEventListener(name, ev => {
    let payload: unknown;
    try {
      payload = JSON.parse((ev as MessageEvent).data);
    } catch {
      return;
    }
    for (const [eventId, cbId] of hub.subs.get(name) ?? []) {
      callbacks.get(cbId)?.({ event: name, id: eventId, payload });
    }
  });
}

function listenCommand(args: Record<string, unknown>): number {
  const name = String(args.event);
  const cbId = Number(args.handler);
  const eventId = nextId++;
  if (!hub.subs.has(name)) hub.subs.set(name, new Map());
  hub.subs.get(name)!.set(eventId, cbId);
  attach(name);
  return eventId;
}

function unlistenCommand(args: Record<string, unknown>) {
  const subs = hub.subs.get(String(args.event));
  const cbId = subs?.get(Number(args.eventId));
  subs?.delete(Number(args.eventId));
  if (cbId !== undefined) callbacks.delete(cbId);
}

/** A "save as" in a browser: remember the file name and let the next write become a download. */
const DOWNLOAD_PREFIX = 'browser-download:';

function saveDialog(options: DialogOptions & { defaultPath?: string }): string {
  const name = (options.defaultPath ?? '').split(/[\\/]/).pop() || 'janus-export';
  return `${DOWNLOAD_PREFIX}${name}`;
}

/** Hands bytes to the browser as a file download. */
export function downloadBytes(name: string, bytes: BlobPart) {
  const url = URL.createObjectURL(new Blob([bytes]));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** The file plugin's write calls carry the path in a header and the bytes as the body. */
function writeDownload(body: unknown, options: { headers?: Record<string, string> } | undefined) {
  let path = options?.headers?.path ?? '';
  try {
    path = decodeURIComponent(path);
  } catch {
    /* already plain */
  }
  if (!path.startsWith(DOWNLOAD_PREFIX)) {
    throw { error: 'unsupported', message: 'This is not available in the browser.' } as WebError;
  }
  downloadBytes(path.slice(DOWNLOAD_PREFIX.length), body as BlobPart);
  return null;
}

async function bridgeInvoke(cmd: string, args?: any, options?: { headers?: Record<string, string> }): Promise<unknown> {
  if (cmd === 'plugin:dialog|save') return saveDialog((args?.options ?? {}) as DialogOptions);
  if (cmd === 'plugin:fs|write_text_file' || cmd === 'plugin:fs|write_file') {
    return writeDownload(args, options);
  }
  if (cmd === 'plugin:event|listen') return listenCommand(args ?? {});
  if (cmd === 'plugin:event|unlisten') return unlistenCommand(args ?? {});
  if (cmd === 'plugin:dialog|open') return pickFiles((args?.options ?? {}) as DialogOptions);
  if (cmd === 'plugin:fs|stat') return statPicked(args ?? {});
  if (cmd.startsWith('plugin:')) {
    throw { error: 'unsupported', message: 'This is not available in the browser.' } as WebError;
  }
  return rpcInvoke(cmd, args);
}

/** Closes the event stream (on sign-out) so the next sign-in starts clean. */
export function closeEvents() {
  hub.source?.close();
  hub.source = null;
  hub.names.clear();
}

/** Puts the stand-in in place. Safe to call once, before any component loads. */
export function installBridge() {
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    invoke: bridgeInvoke,
    transformCallback: (cb: Handler) => {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc: (p: string) => p,
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { windowLabel: 'main', label: 'main' },
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  webMode = true;
}
