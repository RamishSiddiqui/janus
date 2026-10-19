import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  closeEvents,
  fetchFileBlobUrl,
  installBridge,
  isWebMode,
  rpcInvoke,
  SIGNED_OUT_EVENT,
  toWebError,
} from './webBridge';

type Listener = (ev: { data: string }) => void;

class FakeEventSource {
  static last: FakeEventSource | null = null;
  listeners = new Map<string, Listener[]>();
  closed = false;
  constructor(public url: string) {
    FakeEventSource.last = this;
  }
  addEventListener(name: string, fn: Listener) {
    this.listeners.set(name, [...(this.listeners.get(name) ?? []), fn]);
  }
  close() {
    this.closed = true;
  }
  emit(name: string, payload: unknown) {
    for (const fn of this.listeners.get(name) ?? []) fn({ data: JSON.stringify(payload) });
  }
}

function response(status: number, body: string, headers: Record<string, string> = {}) {
  return new Response(body, { status, headers });
}

let target: EventTarget;
let internals: any;

beforeEach(() => {
  target = new EventTarget();
  vi.stubGlobal('window', Object.assign(target, {}) as unknown as Window);
  vi.stubGlobal('EventSource', FakeEventSource);
  vi.stubGlobal('CustomEvent', class extends Event {
    constructor(type: string) {
      super(type);
    }
  });
  installBridge();
  internals = (window as any).__TAURI_INTERNALS__;
});

afterEach(() => {
  closeEvents();
  vi.unstubAllGlobals();
});

describe('toWebError', () => {
  it('keeps the server message', () => {
    expect(toWebError(404, '{"error":"not_found","message":"No such thing."}')).toEqual({
      error: 'not_found',
      message: 'No such thing.',
    });
  });

  it('falls back when the body is not the error shape', () => {
    expect(toWebError(502, '<html>bad gateway</html>')).toEqual({
      error: 'unknown',
      message: 'Request failed (502).',
    });
  });
});

describe('commands over HTTP', () => {
  it('posts the arguments to the rpc endpoint with the CSRF header', async () => {
    const fetchMock = vi.fn().mockResolvedValue(response(200, '{"id":"c1"}'));
    vi.stubGlobal('fetch', fetchMock);

    const out = await internals.invoke('get_character', { id: 'c1' });

    expect(out).toEqual({ id: 'c1' });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/rpc/get_character');
    expect(init.method).toBe('POST');
    expect(init.headers['x-janus-request']).toBe('1');
    expect(JSON.parse(init.body)).toEqual({ id: 'c1' });
  });

  it('rejects with the { error, message } shape the app already handles', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(response(404, '{"error":"not_found","message":"Gone."}')),
    );
    await expect(rpcInvoke('get_character', { id: 'x' })).rejects.toEqual({
      error: 'not_found',
      message: 'Gone.',
    });
  });

  it('announces a signed-out session', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(response(401, '{"error":"unauthorized","message":"Sign in first."}')),
    );
    const seen = vi.fn();
    target.addEventListener(SIGNED_OUT_EVENT, seen);
    await expect(rpcInvoke('list_characters')).rejects.toBeTruthy();
    expect(seen).toHaveBeenCalledOnce();
  });

  it('refuses desktop-only plugin commands', async () => {
    await expect(internals.invoke('plugin:dialog|open', {})).rejects.toMatchObject({
      error: 'unsupported',
    });
  });

  it('sends pasted image bytes to the upload endpoint, not as JSON', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(response(200, '{"relativePath":"attachments/a.png","mimeType":"image/png"}'));
    vi.stubGlobal('fetch', fetchMock);

    const out = await rpcInvoke('upload_message_attachment_bytes', {
      bytes: new Uint8Array([1, 2, 3]),
      extension: 'PNG',
    });

    expect(out).toEqual({ relativePath: 'attachments/a.png', mimeType: 'image/png' });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/upload/attachment?ext=png');
    expect(init.body).toBeInstanceOf(Uint8Array);
  });
});

describe('events', () => {
  it('delivers a streamed event to its listeners in the app event shape', async () => {
    const got = vi.fn();
    const cb = internals.transformCallback(got);
    const eventId = await internals.invoke('plugin:event|listen', { event: 'chat-stream', handler: cb });

    FakeEventSource.last!.emit('chat-stream', { content: 'hi' });
    expect(got).toHaveBeenCalledWith({ event: 'chat-stream', id: eventId, payload: { content: 'hi' } });

    FakeEventSource.last!.emit('other-event', { content: 'no' });
    expect(got).toHaveBeenCalledOnce();
  });

  it('stops delivering after unlisten and shares one stream', async () => {
    const got = vi.fn();
    const cb = internals.transformCallback(got);
    const a = await internals.invoke('plugin:event|listen', { event: 'e', handler: cb });
    const source = FakeEventSource.last;
    await internals.invoke('plugin:event|listen', {
      event: 'f',
      handler: internals.transformCallback(() => {}),
    });
    expect(FakeEventSource.last).toBe(source);

    await internals.invoke('plugin:event|unlisten', { event: 'e', eventId: a });
    source!.emit('e', {});
    expect(got).not.toHaveBeenCalled();
  });
});

describe('files', () => {
  it('fetches a stored file through the files endpoint', async () => {
    const fetchMock = vi.fn().mockResolvedValue(response(200, 'png-bytes', { 'content-type': 'image/png' }));
    vi.stubGlobal('fetch', fetchMock);
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: () => 'blob:abc' }));

    expect(await fetchFileBlobUrl('portraits/a b.png', 'image/png')).toBe('blob:abc');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/files/portraits/a%20b.png');
  });

  it('throws when the file is not available to this account', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(response(404, '{"error":"not_found","message":"File not found."}')),
    );
    await expect(fetchFileBlobUrl('portraits/x.png')).rejects.toMatchObject({ error: 'not_found' });
  });
});

describe('file pickers', () => {
  it('uploads a picked file instead of sending a host path', async () => {
    const file = new File(['png'], 'face.png', { type: 'image/png' });
    // Stand in for the browser's picker: the dialog resolves with this file.
    vi.stubGlobal('document', {
      createElement: () => {
        const handlers: Record<string, () => void> = {};
        const el: any = {
          files: [file],
          addEventListener: (n: string, fn: () => void) => (handlers[n] = fn),
          click: () => queueMicrotask(() => handlers.change?.()),
        };
        return el;
      },
    });
    const token = (await internals.invoke('plugin:dialog|open', { options: { multiple: false } })) as string;
    expect(token).toMatch(/^browser-file:\d+\/face\.png$/);
    expect((await internals.invoke('plugin:fs|stat', { path: token })).size).toBe(3);

    const fetchMock = vi.fn().mockResolvedValue(response(200, '{"id":"c1"}'));
    vi.stubGlobal('fetch', fetchMock);
    await rpcInvoke('upload_character_avatar', { characterId: 'c1', filePath: token });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/upload/portrait/c1');
    expect(fetchMock.mock.calls[0][1].body).toBe(file);

    await rpcInvoke('import_character_card', { filePath: token });
    expect(fetchMock.mock.calls[1][0]).toBe('/api/upload/character-card');
  });

  it('asks to choose again for a path it does not know', async () => {
    await expect(rpcInvoke('import_persona_card', { filePath: 'C:/x.png' })).rejects.toMatchObject({
      error: 'validation',
    });
  });
});

describe('install', () => {
  it('marks the page as served by Janus', () => {
    expect(isWebMode()).toBe(true);
  });
});
