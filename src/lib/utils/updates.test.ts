// Tests for the pure update-prompt helpers (auto-check rules, notes, progress, errors).
import { describe, expect, it } from 'vitest';
import {
  canAutoCheck,
  downloadFraction,
  readDismissed,
  releaseNotesLines,
  updateErrorMessage,
  writeDismissed,
} from './updates';

describe('canAutoCheck', () => {
  it('needs the setting, a desktop app and a release build', () => {
    expect(canAutoCheck({ enabled: true, isDesktop: true, isDev: false })).toBe(true);
    expect(canAutoCheck({ enabled: false, isDesktop: true, isDev: false })).toBe(false);
    expect(canAutoCheck({ enabled: true, isDesktop: false, isDev: false })).toBe(false);
    expect(canAutoCheck({ enabled: true, isDesktop: true, isDev: true })).toBe(false);
  });
});

describe('releaseNotesLines', () => {
  it('strips Markdown markers and blank lines', () => {
    const notes = '## Features\n\n- **Browser** access (#94)\n* Updates\n\n';
    expect(releaseNotesLines(notes)).toEqual(['Features', 'Browser access (#94)', 'Updates']);
  });
  it('caps the number of lines and handles no notes', () => {
    expect(releaseNotesLines('a\nb\nc', 2)).toEqual(['a', 'b']);
    expect(releaseNotesLines(null)).toEqual([]);
    expect(releaseNotesLines('')).toEqual([]);
  });
});

describe('downloadFraction', () => {
  it('is null while the size is unknown and clamps to 0..1', () => {
    expect(downloadFraction(10, null)).toBeNull();
    expect(downloadFraction(10, 0)).toBeNull();
    expect(downloadFraction(50, 200)).toBe(0.25);
    expect(downloadFraction(500, 200)).toBe(1);
    expect(downloadFraction(-5, 200)).toBe(0);
  });
});

describe('updateErrorMessage', () => {
  it('turns common failures into plain sentences', () => {
    expect(updateErrorMessage(new Error('error sending request for url'))).toMatch(/internet connection/);
    expect(updateErrorMessage('signature verification failed')).toMatch(/could not be verified/);
    expect(updateErrorMessage(new Error('Could not fetch a valid release JSON'))).toMatch(/No update information/);
    expect(updateErrorMessage(new Error('disk full'))).toBe('Update failed: disk full');
    expect(updateErrorMessage(undefined)).toBe('Update failed. Please try again.');
  });
});

describe('dismissed version', () => {
  it('round-trips through storage and tolerates a missing or throwing store', () => {
    const data = new Map<string, string>();
    const store = {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
    };
    expect(readDismissed(store)).toBeNull();
    writeDismissed(store, '0.2.0');
    expect(readDismissed(store)).toBe('0.2.0');
    expect(readDismissed(null)).toBeNull();
    const broken = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(readDismissed(broken)).toBeNull();
    expect(() => writeDismissed(broken, '0.2.0')).not.toThrow();
  });
});
