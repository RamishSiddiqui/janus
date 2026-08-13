// ============================================================
//   Janus — Presentation Buffer
//
//   Unified streaming buffer that sits between the backend LLM
//   stream and the frontend message store. Replaces StreamBuffer
//   for ALL conversations (single and multi-character).
//
//   Key behaviors:
//   - Pre-resolves character metadata (avatar, name, accent color)
//   - Detects [CharName]: markers mid-stream for multi-char
//   - Creates per-character bubbles with correct metadata from frame 1
//   - Handles partial markers at token boundaries gracefully
//   - Uses requestAnimationFrame batching for smooth rendering
// ============================================================

import type { Message } from '$lib/types';

/** Pre-resolved character metadata for instant bubble creation. */
export interface CharMeta {
  id: string;
  name: string;
  avatarUrl: string | null;
  accentColor: string;
}

/** Deterministic accent color from character name (matches ChatMessage.svelte). */
const CHAR_ACCENT_COLORS = [
  '#8B5CF6', '#06B6D4', '#F59E0B', '#10B981',
  '#F43F5E', '#3B82F6', '#EC4899', '#6366F1',
];
export function charAccentColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return CHAR_ACCENT_COLORS[Math.abs(hash) % CHAR_ACCENT_COLORS.length];
}

/**
 * Callback interface — the buffer calls these to modify the messages store.
 * This decouples the buffer from Svelte store internals.
 */
export interface PresentationCallbacks {
  /** Create a new message bubble in the store. */
  createMessage(msg: Message): void;
  /** Append content to an existing message bubble. */
  appendContent(messageId: string, text: string): void;
  /** Append to a message's reasoning/thinking trace, distinct from its
   *  visible content — rendered as a separate collapsible section. */
  appendReasoning(messageId: string, text: string): void;
  /** Flips `isThinking` off once real reply content starts arriving. */
  markThinkingDone(messageId: string): void;
  /** Finalize a message (set isStreaming = false). */
  finalizeMessage(messageId: string): void;
}

/**
 * Unified Presentation Buffer.
 *
 * Replaces the old StreamBuffer for ALL conversations. In single-char mode,
 * it creates the bubble with pre-resolved avatar/name from the first frame.
 * In multi-char mode, it detects [CharName]: markers mid-stream and creates
 * sequential per-character bubbles.
 */
export class PresentationBuffer {
  // ── Character context (set before streaming starts) ──
  private primaryChar: CharMeta;
  private allChars: Map<string, CharMeta>;      // charId → meta
  private nameToChar: Map<string, CharMeta>;     // charName (lowercase) → meta
  private knownNames: string[];                  // For marker regex
  private markerRegex: RegExp | null = null;

  // ── Stream parsing state ──
  private rawBuffer: string = '';                // Accumulated text not yet committed
  private activeChar: CharMeta | null = null;    // Character currently streaming
  private activeMsgId: string | null = null;     // Message ID of active bubble
  private segmentIndex: number = 0;              // 0, 1, 2... for composite IDs
  private parentMsgId: string = '';              // Original assistant message ID from backend
  private isFirstDelta: boolean = true;
  private isMultiCharConversation: boolean;

  // ── Rendering state (rAF batching) ──
  private pendingText: string = '';
  private rafId: number | null = null;

  // ── Callbacks ──
  private callbacks: PresentationCallbacks;

  constructor(
    primaryChar: CharMeta,
    allChars: Map<string, CharMeta>,
    callbacks: PresentationCallbacks,
  ) {
    this.primaryChar = primaryChar;
    this.allChars = allChars;
    this.callbacks = callbacks;
    this.isMultiCharConversation = allChars.size > 1;

    // Build name→meta lookup (case-insensitive)
    this.nameToChar = new Map();
    this.knownNames = [];
    for (const meta of allChars.values()) {
      this.nameToChar.set(meta.name.toLowerCase(), meta);
      this.knownNames.push(meta.name);
    }

    // Build marker regex from known names (same pattern as response_parser.rs)
    if (this.knownNames.length > 1) {
      const escaped = this.knownNames.map(n =>
        n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
      );
      // Match [CharName]: at line start (possibly after newlines)
      const bracketPattern = `(?:^|\\n)\\[(?:${escaped.join('|')})\\]:\\s*`;
      // Also match bare CharName: at line start
      const barePattern = `(?:^|\\n)(?:${escaped.join('|')}):\\s*`;
      try {
        this.markerRegex = new RegExp(`(${bracketPattern})|(${barePattern})`);
      } catch {
        this.markerRegex = null;
      }
    }
  }

  /**
   * Push a reasoning/thinking delta — arrives BEFORE the real reply in
   * genuine chain-of-thought models (Nemotron, DeepSeek R1, etc.), while the
   * message bubble doesn't exist yet since `push()` normally creates it.
   * Creates the bubble immediately here instead, so the user sees a live
   * "Thinking…" indicator rather than dead air, then keeps appending as more
   * reasoning streams in. Multi-character conversations skip this (reasoning
   * precedes any [CharName]: marker, so there's no character to attribute the
   * bubble to yet) — reasoning is silently dropped there rather than shown.
   */
  pushReasoning(parentMessageId: string, text: string): void {
    if (this.isMultiCharConversation) return;

    if (!this.activeMsgId) {
      this.parentMsgId = parentMessageId;
      this.activeChar = this.primaryChar;
      this.activeMsgId = parentMessageId;
      this.callbacks.createMessage({
        id: parentMessageId,
        role: 'assistant',
        content: '',
        reasoning: text,
        isStreaming: true,
        isThinking: true,
        thinkingStartedAt: Date.now(),
        character_name: this.primaryChar.name,
        character_id: this.primaryChar.id,
        character_avatar_url: this.primaryChar.avatarUrl,
      });
      return;
    }
    this.callbacks.appendReasoning(this.activeMsgId, text);
  }

  /**
   * Push a new delta token from the stream.
   * Detects character markers and routes content to the correct bubble.
   */
  push(parentMessageId: string, text: string): void {
    if (this.isFirstDelta) {
      this.parentMsgId = parentMessageId;
      this.isFirstDelta = false;

      if (!this.isMultiCharConversation) {
        if (this.activeMsgId === parentMessageId) {
          // Bubble already exists from the reasoning phase — mark thinking
          // done now that real content has started, then fall through to
          // the normal append path below instead of creating a duplicate.
          this.callbacks.markThinkingDone(this.activeMsgId);
        } else {
          // ── Single-char: create the bubble immediately with metadata ──
          this.activeChar = this.primaryChar;
          this.activeMsgId = parentMessageId;
          this.callbacks.createMessage({
            id: parentMessageId,
            role: 'assistant',
            content: text,
            isStreaming: true,
            character_name: this.primaryChar.name,
            character_id: this.primaryChar.id,
            character_avatar_url: this.primaryChar.avatarUrl,
          });
          return;
        }
      }
    }

    if (!this.isMultiCharConversation) {
      // ── Single-char: simple append via rAF batching ──
      this.pendingText += text;
      this.scheduleFlush();
      return;
    }

    // ── Multi-char: accumulate and scan for markers ──
    this.rawBuffer += text;
    this.processMultiCharBuffer();
  }

  /**
   * Scan the raw buffer for [CharName]: markers.
   * When found: flush content before the marker to the current character,
   * then switch to the new character.
   */
  private processMultiCharBuffer(): void {
    if (!this.markerRegex) {
      // No regex (shouldn't happen for multi-char) — treat as single char
      this.pendingText += this.rawBuffer;
      this.rawBuffer = '';
      this.scheduleFlush();
      return;
    }

    // Keep scanning until no more complete markers are found
    while (true) {
      const match = this.markerRegex.exec(this.rawBuffer);

      if (!match) {
        // No marker found — check for partial marker at the end
        const partialIdx = this.findPartialMarkerStart(this.rawBuffer);
        if (partialIdx >= 0 && partialIdx < this.rawBuffer.length) {
          // Commit everything before the potential partial marker
          const safe = this.rawBuffer.slice(0, partialIdx);
          if (safe) {
            this.commitContentToActive(safe);
          }
          // Keep the potential partial marker in the buffer
          this.rawBuffer = this.rawBuffer.slice(partialIdx);
        } else {
          // No partial match — commit everything
          this.commitContentToActive(this.rawBuffer);
          this.rawBuffer = '';
        }
        break;
      }

      // ── We found a complete marker ──
      const markerText = match[0];
      const markerStart = match.index;

      // Content before the marker belongs to the current character
      const contentBefore = this.rawBuffer.slice(0, markerStart);
      if (contentBefore) {
        this.commitContentToActive(contentBefore);
      }

      // Extract character name from the marker
      const charName = this.extractNameFromMarker(markerText);
      if (charName) {
        // Finalize current character's bubble (if any)
        if (this.activeMsgId) {
          this.flushPending();
          this.callbacks.finalizeMessage(this.activeMsgId);
        }

        // Switch to the new character
        const charMeta = this.nameToChar.get(charName.toLowerCase()) || {
          id: '',
          name: charName,
          avatarUrl: null,
          accentColor: charAccentColor(charName),
        };
        this.activeChar = charMeta;

        // Create a new bubble for this character
        this.activeMsgId = `${this.parentMsgId}__seg${this.segmentIndex}`;
        this.segmentIndex++;
        this.callbacks.createMessage({
          id: this.activeMsgId,
          role: 'assistant',
          content: '',
          isStreaming: true,
          character_name: charMeta.name,
          character_id: charMeta.id || null,
          character_avatar_url: charMeta.avatarUrl,
        });
      }

      // Advance past the marker
      this.rawBuffer = this.rawBuffer.slice(markerStart + markerText.length);
    }
  }

  /**
   * Extract character name from a matched marker string.
   * Handles both [CharName]: and CharName: formats.
   */
  private extractNameFromMarker(marker: string): string | null {
    // Try bracket format: [CharName]:
    const bracketMatch = marker.match(/\[([^\]]+)\]:/);
    if (bracketMatch) return bracketMatch[1];

    // Try bare format: CharName:
    const trimmed = marker.replace(/^\n/, '').trim();
    for (const name of this.knownNames) {
      if (trimmed.startsWith(name + ':')) return name;
    }
    return null;
  }

  /**
   * Check if the buffer ends with a partial marker start.
   * Returns the index where the potential partial marker begins, or -1.
   */
  private findPartialMarkerStart(text: string): number {
    // Look for a `[` or a known name prefix near the end
    // The maximum marker length is ~50 chars: \n[LongestCharName]: 
    const searchWindow = Math.min(text.length, 80);
    const tail = text.slice(-searchWindow);
    const tailStart = text.length - searchWindow;

    // Check for `[` that isn't closed with `]:`
    const lastBracket = tail.lastIndexOf('[');
    if (lastBracket >= 0) {
      const afterBracket = tail.slice(lastBracket);
      if (!afterBracket.includes(']:')) {
        return tailStart + lastBracket;
      }
    }

    // Check for newline followed by a known name prefix
    const lastNewline = tail.lastIndexOf('\n');
    if (lastNewline >= 0) {
      const afterNewline = tail.slice(lastNewline + 1).trimStart();
      for (const name of this.knownNames) {
        // Could be a partial match: "Ari" matches start of "Aria Silverleaf"
        if (name.toLowerCase().startsWith(afterNewline.toLowerCase()) && afterNewline.length > 0) {
          return tailStart + lastNewline;
        }
        if (afterNewline.toLowerCase().startsWith(name.toLowerCase())) {
          // Full name match but no colon yet
          if (!afterNewline.includes(':')) {
            return tailStart + lastNewline;
          }
        }
      }
    }

    return -1;
  }

  /**
   * Route content to the active character's bubble.
   * If no active character yet (first content before any marker in multi-char),
   * create a bubble for the primary character.
   */
  private commitContentToActive(content: string): void {
    if (!this.activeMsgId) {
      // No character identified yet — this is pre-marker content.
      // In multi-char, create a bubble for the primary character.
      this.activeChar = this.primaryChar;
      this.activeMsgId = `${this.parentMsgId}__seg${this.segmentIndex}`;
      this.segmentIndex++;
      this.callbacks.createMessage({
        id: this.activeMsgId,
        role: 'assistant',
        content: '',
        isStreaming: true,
        character_name: this.primaryChar.name,
        character_id: this.primaryChar.id,
        character_avatar_url: this.primaryChar.avatarUrl,
      });
    }

    this.pendingText += content;
    this.scheduleFlush();
  }

  /** Schedule a rAF flush if not already scheduled. */
  private scheduleFlush(): void {
    if (this.rafId === null) {
      this.rafId = requestAnimationFrame(() => this.flushPending());
    }
  }

  /** Flush accumulated text to the active bubble via callback. */
  private flushPending(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    const batch = this.pendingText;
    if (!batch || !this.activeMsgId) return;
    this.pendingText = '';
    this.callbacks.appendContent(this.activeMsgId, batch);
  }

  /**
   * Called when the stream completes ('done' event).
   * Flushes any remaining buffered content and finalizes the last bubble.
   */
  finalize(): void {
    // Flush any remaining raw buffer content (multi-char partial markers that
    // turned out to be regular text)
    if (this.rawBuffer) {
      this.commitContentToActive(this.rawBuffer);
      this.rawBuffer = '';
    }

    // Flush pending rAF text
    this.flushPending();

    // Edge case: a model that emits ONLY reasoning blocks, never real Text —
    // the bubble is still marked isThinking from pushReasoning() since push()
    // (which normally clears it) never ran. Clear it now so the UI doesn't
    // show a permanently-pulsing "Thinking…" indicator on a finished message.
    if (this.activeMsgId) {
      this.callbacks.markThinkingDone(this.activeMsgId);
    }

    // Finalize the active bubble
    if (this.activeMsgId) {
      this.callbacks.finalizeMessage(this.activeMsgId);
    }
  }

  /**
   * Called on error or cleanup. Flushes remaining text and resets state.
   */
  reset(): void {
    if (this.rawBuffer) {
      this.commitContentToActive(this.rawBuffer);
      this.rawBuffer = '';
    }
    this.flushPending();
    this.pendingText = '';
    this.rawBuffer = '';
    this.activeMsgId = null;
    this.activeChar = null;
    this.segmentIndex = 0;
    this.parentMsgId = '';
    this.isFirstDelta = true;
  }

  /** Returns the segment IDs created during this stream (for ID reconciliation). */
  get segmentIds(): string[] {
    const ids: string[] = [];
    for (let i = 0; i < this.segmentIndex; i++) {
      ids.push(`${this.parentMsgId}__seg${i}`);
    }
    return ids;
  }

  /** Returns whether this buffer produced multi-char segments. */
  get isMultiChar(): boolean {
    return this.segmentIndex > 0 && this.isMultiCharConversation;
  }

  /** Returns the parent message ID this buffer is handling. */
  get parentId(): string {
    return this.parentMsgId;
  }
}
