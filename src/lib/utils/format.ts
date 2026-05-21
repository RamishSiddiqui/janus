// ============================================================
//   Mythic — Text Formatting Utilities
// ============================================================

import { sanitizeHtml } from './sanitize';

/**
 * Format roleplay message content:
 *  - *asterisks* become italic action text
 *  - Newlines become <br/> tags
 *  - Output is sanitized against XSS
 */
export function formatRoleplayContent(text: string): string {
  // First escape any raw HTML the user may have typed
  const escaped = escapeHtml(text);

  // Split into lines to detect block-level vs inline actions
  const lines = escaped.split('\n');
  const formattedLines = lines.map(line => {
    const trimmed = line.trim();
    // Full-line action: the entire line is wrapped in asterisks (block display)
    if (/^\*[^*]+\*$/.test(trimmed)) {
      return trimmed.replace(/^\*([^*]+)\*$/, '<em class="rp-action rp-action-block">$1</em>');
    }
    // Mixed line: inline emphasis within dialogue/text (inline display)
    return line.replace(/\*([^*]+)\*/g, '<em class="rp-action">$1</em>');
  });

  const formatted = formattedLines.join('\n')
    // Double newlines → paragraph break
    .replace(/\n{2,}/g, '<br/><br/>')
    // Single newline → space (prose continuation)
    .replace(/\n/g, ' ');

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
