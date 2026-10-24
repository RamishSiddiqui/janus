// Tests for the insecure-context fallbacks (uuid and clipboard) used over plain HTTP.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyText, uuid } from './insecureContext';

const V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

afterEach(() => vi.unstubAllGlobals());

describe('uuid', () => {
  it('uses crypto.randomUUID when the browser has it', () => {
    vi.stubGlobal('crypto', { randomUUID: () => 'from-crypto' });
    expect(uuid()).toBe('from-crypto');
  });
  it('builds a valid v4 id from getRandomValues when randomUUID is missing', () => {
    vi.stubGlobal('crypto', { getRandomValues: (a: Uint8Array) => a.fill(0xff) });
    expect(uuid()).toMatch(V4);
  });
  it('still returns a valid id with no crypto at all', () => {
    vi.stubGlobal('crypto', undefined);
    expect(uuid()).toMatch(V4);
    expect(uuid()).not.toBe(uuid());
  });
});

describe('copyText', () => {
  it('uses the clipboard API when present', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    expect(await copyText('hi')).toBe(true);
    expect(writeText).toHaveBeenCalledWith('hi');
  });
  it('falls back to a hidden textarea and execCommand', async () => {
    const area: Record<string, unknown> = {
      setAttribute: vi.fn(),
      select: vi.fn(),
      setSelectionRange: vi.fn(),
      remove: vi.fn(),
      style: {},
    };
    const execCommand = vi.fn().mockReturnValue(true);
    vi.stubGlobal('navigator', {});
    vi.stubGlobal('document', { createElement: () => area, body: { appendChild: vi.fn() }, execCommand });
    expect(await copyText('secret')).toBe(true);
    expect(area.value).toBe('secret');
    expect(execCommand).toHaveBeenCalledWith('copy');
  });
  it('reports failure when nothing can copy', async () => {
    vi.stubGlobal('navigator', {});
    vi.stubGlobal('document', undefined);
    expect(await copyText('x')).toBe(false);
  });
});
