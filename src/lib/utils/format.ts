// ============================================================
//   Janus — Text Formatting Utilities
// ============================================================

import { sanitizeHtml } from './sanitize';

/**
 * Format roleplay message content:
 *  - **asterisks** become bold emphasis
 *  - *asterisks* become italic action text
 *  - Newlines become <br/> tags
 *  - Output is sanitized against XSS
 */
export function formatRoleplayContent(text: string): string {
  // First escape any raw HTML the user may have typed
  const escaped = escapeHtml(text);

  // Bold FIRST — this consumes **-delimited pairs before the single-*
  // action pass below runs. Doing it in the other order let a model's
  // stray "**word**" mis-pair: the single-* regex would grab one '*' from
  // the bold pair as an action's closing delimiter, leaving the other
  // marker to pair with some unrelated '*' later in the line and corrupt
  // every action/dialogue boundary after it — the "focused"/"weaving"-type
  // formatting glitch.
  const withBold = escaped.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // Split into lines to detect block-level vs inline actions
  const lines = withBold.split('\n');
  const formattedLines = lines.map(line => {
    const trimmed = line.trim();
    // Full-line action: the entire line is wrapped in asterisks (block display)
    if (/^\*[^*"]+\*$/.test(trimmed)) {
      return trimmed.replace(/^\*([^*"]+)\*$/, '<em class="rp-action rp-action-block">$1</em>');
    }
    // Mixed line: inline emphasis within dialogue/text (inline display).
    // Excludes '"' from the captured span so a model's stray/unbalanced
    // single asterisk (e.g. a redundant "*focused*" nested inside what
    // was meant to be one continuous action, leaving an odd marker count)
    // can't swallow an entire quoted dialogue chunk looking for its next
    // '*' — that's what let one bad marker desync every action/dialogue
    // pairing for the rest of the message. Capped here, pairing resyncs
    // at the next quote boundary instead of cascading.
    //
    // A span inside an open pair of "double quotes" is vocal emphasis on a
    // word the character is SAYING (e.g. "Well. *That* worked.") — it should
    // read as plain italic within the dialogue's own color/size. The
    // '.rp-action' class is for narrative action beats (muted, smaller) and
    // was being applied here too, so a single stressed word mid-sentence
    // came out shrunk and washed-out instead of emphasized. Track quote
    // parity up to each match to tell the two cases apart.
    return line.replace(/\*([^*"]+)\*/g, (match, inner: string, offset: number) => {
      const quotesBefore = (line.slice(0, offset).match(/"/g) ?? []).length;
      const insideDialogue = quotesBefore % 2 === 1;
      return insideDialogue ? `<em>${inner}</em>` : `<em class="rp-action">${inner}</em>`;
    });
  });

  const formatted = formattedLines.join('\n')
    // Double newlines → paragraph break. A single <br/> is enough — block-level
    // action paragraphs (.rp-action-block) already carry their own margin, so
    // doubling up here (as this used to) stacked a full blank line on top of
    // that margin for every paragraph, and RP prose breaks paragraphs after
    // nearly every line, producing far too much vertical gap between lines.
    .replace(/\n{2,}/g, '<br/>')
    // Single newline → space (prose continuation)
    .replace(/\n/g, ' ')
    // Any '*' still here is an orphaned marker the passes above couldn't
    // pair (an unbalanced/malformed emphasis run in the source) — every
    // intentional delimiter has already been consumed into an <em>/
    // <strong> tag by this point, so drop the leftover rather than show
    // a stray asterisk in the message.
    .replace(/\*/g, '');

  // Sanitize the final output
  return sanitizeHtml(formatted);
}

/**
 * Escape HTML entities to prevent injection before formatting.
 */
function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}
