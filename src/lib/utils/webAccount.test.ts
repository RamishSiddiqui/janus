import { describe, expect, it } from 'vitest';
import {
  cleanRecoveryKey,
  formatRecoveryKey,
  passphraseStrength,
  usernameOk,
} from './webAccount';

describe('passphraseStrength', () => {
  it('grows with length and variety', () => {
    expect(passphraseStrength('short')).toBe(0);
    expect(passphraseStrength('eightchr')).toBe(1);
    expect(passphraseStrength('twelve chars')).toBe(2);
    expect(passphraseStrength('sixteencharsokAB')).toBe(3);
    expect(passphraseStrength('a long enough sentence here')).toBe(4);
  });
});

describe('usernameOk', () => {
  it('accepts 3 to 24 safe characters only', () => {
    expect(usernameOk('ada')).toBe(true);
    expect(usernameOk('a.b_c-9')).toBe(true);
    expect(usernameOk('ab')).toBe(false);
    expect(usernameOk('has space')).toBe(false);
    expect(usernameOk('-lead')).toBe(false);
    expect(usernameOk('x'.repeat(25))).toBe(false);
  });
});

describe('recovery key typing', () => {
  it('strips the prefix and separators and caps at 16 characters', () => {
    expect(cleanRecoveryKey('janus-7k2f-9qxd-m41b-zt8r-extra')).toBe('7K2F9QXDM41BZT8R');
    expect(cleanRecoveryKey('7k2f 9qxd')).toBe('7K2F9QXD');
  });

  it('formats groups of four as you type', () => {
    expect(formatRecoveryKey('')).toBe('');
    expect(formatRecoveryKey('7K2F9Q')).toBe('JANUS-7K2F-9Q');
    expect(formatRecoveryKey('7K2F9QXDM41BZT8R')).toBe('JANUS-7K2F-9QXD-M41B-ZT8R');
  });
});
