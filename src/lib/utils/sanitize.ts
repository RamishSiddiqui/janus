// ============================================================
//   Janus — HTML Sanitization
// ============================================================

import DOMPurify from 'dompurify';

/**
 * Sanitize HTML string to prevent XSS attacks.
 * Only allows safe inline formatting tags.
 */
export function sanitizeHtml(dirty: string): string {
  return DOMPurify.sanitize(dirty, {
    ALLOWED_TAGS: ['em', 'strong', 'br', 'span', 'p'],
    ALLOWED_ATTR: ['class'],
  });
}

const ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

/**
 * Makes a search snippet safe to render with `{@html}`.
 *
 * The backend returns the stored message text with `<mark>` tags around the
 * matches (SurrealDB `search::highlight`) and does not escape anything else,
 * so the text may contain arbitrary HTML. Everything is escaped, then only the
 * mark tags are turned back on. Message text that literally contains `<mark>`
 * becomes a highlight, which is harmless.
 */
export function highlightSnippetHtml(snippet: string): string {
  return snippet
    .replace(/[&<>"']/g, ch => ESCAPES[ch])
    .replace(/&lt;mark&gt;/g, '<mark>')
    .replace(/&lt;\/mark&gt;/g, '</mark>');
}
