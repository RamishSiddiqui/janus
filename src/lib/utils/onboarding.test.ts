import { describe, expect, it } from 'vitest';
import { MODEL_SOURCES, rankModels, shouldShowWizard } from './onboarding';
import { STARTER_CHARACTERS } from '../data/starterCharacters';

describe('rankModels', () => {
  it('puts free models first, then sorts, and drops duplicates', () => {
    expect(rankModels(['b/pro', 'a/x:free', 'b/pro', 'a/y', 'c/z:free'])).toEqual([
      'a/x:free',
      'c/z:free',
      'a/y',
      'b/pro',
    ]);
  });

  it('handles an empty list', () => {
    expect(rankModels([])).toEqual([]);
  });
});

describe('shouldShowWizard', () => {
  it('shows only for a new user with no provider', () => {
    expect(shouldShowWizard(false, 0)).toBe(true);
    expect(shouldShowWizard(true, 0)).toBe(false);
    expect(shouldShowWizard(false, 2)).toBe(false);
  });
});

describe('starter characters', () => {
  it('are complete and uniquely named', () => {
    const names = STARTER_CHARACTERS.map(c => c.data.name);
    expect(new Set(names).size).toBe(names.length);
    for (const c of STARTER_CHARACTERS) {
      for (const f of ['name', 'description', 'personality', 'scenario', 'first_mes']) {
        expect(String(c.data[f]).length).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

describe('model sources', () => {
  it('give a default address to every source that needs one', () => {
    for (const s of MODEL_SOURCES.filter(m => m.needsBaseUrl)) {
      expect(s.defaultBaseUrl).not.toBe('');
    }
  });
});
