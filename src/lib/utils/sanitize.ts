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
