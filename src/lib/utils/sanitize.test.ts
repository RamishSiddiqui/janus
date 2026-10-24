// Tests for the search-snippet escaping used with {@html} in the sidebar search.
import { describe, expect, it } from 'vitest';
import { highlightSnippetHtml } from './sanitize';

describe('highlightSnippetHtml', () => {
  it('keeps the highlight marks the search adds', () => {
    expect(highlightSnippetHtml('the <mark>dragon</mark> sleeps')).toBe('the <mark>dragon</mark> sleeps');
  });

  it('escapes script and event-handler payloads in message text', () => {
    const out = highlightSnippetHtml('<img src=x onerror=alert(1)> and <mark>hit</mark><script>steal()</script>');
    expect(out).toBe('&lt;img src=x onerror=alert(1)&gt; and <mark>hit</mark>&lt;script&gt;steal()&lt;/script&gt;');
    expect(out).not.toMatch(/<img|<script/);
  });

  it('escapes quotes and ampersands so nothing can break out of text', () => {
    expect(highlightSnippetHtml(`a "b" 'c' & d`)).toBe('a &quot;b&quot; &#39;c&#39; &amp; d');
  });

  it('only turns on bare mark tags, not marks that carry attributes', () => {
    const out = highlightSnippetHtml('<mark onmouseover=alert(1)>x</mark>');
    expect(out).toBe('&lt;mark onmouseover=alert(1)&gt;x</mark>');
    expect(out).not.toContain('<mark onmouseover');
  });

  it('handles empty text', () => {
    expect(highlightSnippetHtml('')).toBe('');
  });
});
