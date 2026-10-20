// Tests for the composer's Markdown helpers.
import { describe, expect, it } from 'vitest';
import { toggleBold } from './markdownEdit';

describe('toggleBold', () => {
  it('wraps the selection and keeps it selected', () => {
    expect(toggleBold('say hello now', 4, 9)).toEqual({ text: 'say **hello** now', start: 6, end: 11 });
  });
  it('inserts an empty pair with the cursor between when nothing is selected', () => {
    expect(toggleBold('hi ', 3, 3)).toEqual({ text: 'hi ****', start: 5, end: 5 });
  });
  it('removes markers that sit around the selection', () => {
    expect(toggleBold('say **hello** now', 6, 11)).toEqual({ text: 'say hello now', start: 4, end: 9 });
  });
  it('removes markers that are part of the selection', () => {
    expect(toggleBold('say **hello** now', 4, 13)).toEqual({ text: 'say hello now', start: 4, end: 9 });
  });
  it('accepts a backwards selection', () => {
    expect(toggleBold('abc', 3, 0).text).toBe('**abc**');
  });
});
