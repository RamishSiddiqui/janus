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

  // Apply roleplay formatting
  const formatted = escaped
    .replace(/\*([^*]+)\*/g, '<em class="rp-action">$1</em>')
    .replace(/\n/g, '<br/>');

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
