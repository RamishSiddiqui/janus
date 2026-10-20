/** Small text edits for the chat composer, kept pure so they can be tested. */

export interface EditResult {
  text: string;
  /** Selection after the edit. */
  start: number;
  end: number;
}

/**
 * Makes the selection bold with Markdown `**`, or removes the markers if the
 * selection is already wrapped in them. With nothing selected it inserts an
 * empty pair and puts the cursor between them.
 */
export function toggleBold(text: string, start: number, end: number): EditResult {
  const lo = Math.max(0, Math.min(start, end));
  const hi = Math.min(text.length, Math.max(start, end));
  const before = text.slice(0, lo);
  const selected = text.slice(lo, hi);
  const after = text.slice(hi);

  // Markers sit just outside the selection: **|word|**
  if (before.endsWith('**') && after.startsWith('**') && selected.length > 0) {
    return { text: before.slice(0, -2) + selected + after.slice(2), start: lo - 2, end: hi - 2 };
  }
  // Markers are part of the selection: |**word**|
  if (selected.length >= 4 && selected.startsWith('**') && selected.endsWith('**')) {
    const inner = selected.slice(2, -2);
    return { text: before + inner + after, start: lo, end: lo + inner.length };
  }
  return { text: `${before}**${selected}**${after}`, start: lo + 2, end: hi + 2 };
}
