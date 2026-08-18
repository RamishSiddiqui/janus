import { describe, expect, it } from 'vitest';
import { parseEmotionSnapshot } from './chatEmotion';

function makeCharacterState(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    id: 'state-1',
    character_id: 'char-1',
    conversation_id: 'conv-1',
    mood: 60,
    trust: 50,
    arousal: 40,
    dominant_emotion: 'curious',
    state_summary: 'Watching closely.',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('parseEmotionSnapshot', () => {
  it('extracts a valid emotional_states map', () => {
    const metadata = {
      emotional_states: {
        'char-1': makeCharacterState(),
      },
    };
    const result = parseEmotionSnapshot(metadata);
    expect(result).toBeDefined();
    expect(result?.['char-1'].dominant_emotion).toBe('curious');
  });

  it('returns undefined when metadata is null or undefined', () => {
    expect(parseEmotionSnapshot(null)).toBeUndefined();
    expect(parseEmotionSnapshot(undefined)).toBeUndefined();
  });

  it('returns undefined when metadata has no emotional_states key', () => {
    expect(parseEmotionSnapshot({ other_field: 'value' })).toBeUndefined();
  });

  it('returns undefined when emotional_states is not an object', () => {
    expect(parseEmotionSnapshot({ emotional_states: 'not-an-object' })).toBeUndefined();
  });

  it('returns undefined when emotional_states is an empty object', () => {
    expect(parseEmotionSnapshot({ emotional_states: {} })).toBeUndefined();
  });

  it('filters out entries missing dominant_emotion', () => {
    const metadata = {
      emotional_states: {
        'char-1': makeCharacterState(),
        'char-2': { id: 'state-2', mood: 50 }, // malformed — no dominant_emotion
      },
    };
    const result = parseEmotionSnapshot(metadata);
    expect(result).toBeDefined();
    expect(Object.keys(result ?? {})).toEqual(['char-1']);
  });

  it('supports multiple characters in one snapshot', () => {
    const metadata = {
      emotional_states: {
        'char-1': makeCharacterState({ dominant_emotion: 'curious' }),
        'char-2': makeCharacterState({ character_id: 'char-2', dominant_emotion: 'wary' }),
      },
    };
    const result = parseEmotionSnapshot(metadata);
    expect(Object.keys(result ?? {})).toHaveLength(2);
    expect(result?.['char-2'].dominant_emotion).toBe('wary');
  });
});
