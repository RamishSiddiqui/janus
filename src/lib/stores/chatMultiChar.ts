// ============================================================
//   Janus — Multi-Character Response Listener
//   Listens for parsed multi-character response segments from the backend.
//   When a response contains dialogue from multiple characters, the backend
//   emits 'multi-char-response' with segment attribution. This function
//   annotates messages in the store so the UI can render character badges.
//   Split out of chat.ts.
// ============================================================

import { get } from 'svelte/store';
import { browser } from '$app/environment';
import { activeConversationId, conversationCharMeta, messages, pathState } from './chat';
import { runEmotionUpdatePipeline } from './chatEmotion';

const isTauri = browser && '__TAURI_INTERNALS__' in window;

let unlistenMultiChar: (() => void) | null = null;

/**
 * Initializes the global listener for multi-character response events.
 * Should be called once when the app starts (e.g., in the root layout).
 * Returns a cleanup function to unsubscribe.
 */
export async function initMultiCharListener(): Promise<() => void> {
  // Avoid duplicate listeners
  if (unlistenMultiChar) return unlistenMultiChar;

  if (!isTauri) return () => {};

  const { listen } = await import('@tauri-apps/api/event');

  unlistenMultiChar = await listen('multi-char-response', (event: any) => {
    const payload = event.payload;
    if (!payload) return;

    // Guard: only process events for the active conversation
    if (payload.conversation_id !== get(activeConversationId)) return;

    // The segments contain character-attributed dialogue from the response parser.
    // With the PresentationBuffer, the frontend may already have per-character
    // bubbles (IDs: parentId__seg0, __seg1, ...). This listener reconciles
    // the final parsed content and character attribution from the backend.
    const segments = payload.segments as {
      character_name: string;
      character_id?: string;
      content: string;
      index: number;
      id?: string;
    }[];

    if (!segments || segments.length === 0) return;

    // Multi-speaker turns (and solo NPC lines) empty out the "done" event's
    // content specifically to avoid re-rendering a duplicate combined
    // message — which also means the ordinary emotion-update pipeline never
    // fires for them. Backend enriches this event with real message IDs
    // (segments[i].id) plus the raw full_text/user_message so it can run
    // here instead, covering background/auto-detected characters (e.g. an
    // NPC like "Lena" speaking her own segment) the same as any named cast
    // member, regardless of whether the conversation is nominally single- or
    // multi-character — that's purely how many rows exist in the cast table.
    const fullText = payload.full_text as string | undefined;
    const userMessage = payload.user_message as string | undefined;
    const emotionParentId = payload.parent_message_id as string;
    if (fullText && userMessage) {
      const targetIds = segments.map(s => s.id).filter((id): id is string => !!id);
      if (targetIds.length > 0) {
        // Also patch PresentationBuffer's synthetic per-segment display ids
        // (parentId, parentId__seg0, __seg1, ...) — whichever the live
        // message store is currently using for these bubbles.
        const displayIds = [
          emotionParentId,
          ...segments.map((_, i) => `${emotionParentId}__seg${i}`),
        ];
        runEmotionUpdatePipeline(payload.conversation_id as string, userMessage, fullText, targetIds, displayIds);
      }
    }

    const parentId = payload.parent_message_id as string;
    const charMeta = get(conversationCharMeta);

    if (segments.length === 1) {
      const seg = segments[0];
      const meta = seg.character_id ? charMeta.get(seg.character_id) : null;

      // Single segment — update the parent message in-place with stripped
      // content and character attribution. Works whether the buffer created
      // the message as parentId or parentId__seg0.
      messages.update(msgs =>
        msgs.map(m => {
          if (m.id === parentId || m.id === `${parentId}__seg0`) {
            return {
              ...m,
              content: seg.content,
              character_name: seg.character_name,
              character_id: seg.character_id || null,
              character_avatar_url: meta?.avatarUrl ?? m.character_avatar_url ?? null,
              isStreaming: false,
            };
          }
          return m;
        })
      );
      // Sync pathState.fullActivePath
      for (let i = 0; i < pathState.fullActivePath.length; i++) {
        const m = pathState.fullActivePath[i];
        if (m.id === parentId || m.id === `${parentId}__seg0`) {
          pathState.fullActivePath[i] = {
            ...m,
            content: seg.content,
            character_name: seg.character_name,
            character_id: seg.character_id || null,
            character_avatar_url: meta?.avatarUrl ?? m.character_avatar_url ?? null,
            isStreaming: false,
          };
          break;
        }
      }
      return;
    }

    // Multiple segments — replace whatever bubble(s) PresentationBuffer
    // created for this turn with the backend's canonical split.
    //
    // The live buffer's own [Name]: marker detection may have produced
    // anywhere from 0 to N bubbles by the time streaming finished: all N
    // (ids parentId__seg0.. __segN-1) when the model emitted live markers
    // for every switch, or just ONE (parentId__seg0, holding the entire
    // un-split turn) when the model never emitted a marker at all and the
    // buffer's "no marker yet" fallback (commitContentToActive) put all of
    // it under the primary character — exactly what response_parser.rs's
    // paragraph-name-cue fallback exists to catch *after the fact*. Testing
    // only "does parentId__seg0 exist" to decide between "reconcile in
    // place" vs. "full split" used to treat that one placeholder bubble as
    // proof the live split fully succeeded, so only segment 0 ever got
    // updated and every later segment (e.g. a second character's dialogue)
    // was silently dropped from the live view — it only reappeared after a
    // full reload (switching conversations and back) because that fetches
    // the real, correctly-split rows straight from the DB. Instead: find
    // every existing bubble that belongs to this turn (id === parentId, or
    // parentId__seg0, __seg1, ...), remove all of them, and insert exactly
    // the segments the backend parsed at that position — correct whether
    // the live buffer created 0, 1, or all of them.
    const isThisTurn = (id: string) => id === parentId || id.startsWith(`${parentId}__seg`);
    const buildSegMessage = <T extends { id: string; siblingIds?: unknown; siblingIndex?: unknown; siblingConversationIds?: unknown; siblingConversationIndex?: unknown }>(
      template: T,
      seg: (typeof segments)[number],
      i: number
    ): T => {
      const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
      return {
        ...template,
        id: `${parentId}__seg${i}`,
        content: seg.content,
        character_name: seg.character_name,
        character_id: seg.character_id || null,
        character_avatar_url: meta?.avatarUrl ?? null,
        siblingIds: i === 0 ? template.siblingIds : undefined,
        siblingIndex: i === 0 ? template.siblingIndex : undefined,
        siblingConversationIds: i === 0 ? template.siblingConversationIds : undefined,
        siblingConversationIndex: i === 0 ? template.siblingConversationIndex : undefined,
        isStreaming: false,
      };
    };

    messages.update(msgs => {
      const firstIdx = msgs.findIndex(m => isThisTurn(m.id));
      if (firstIdx < 0) return msgs;
      const template = msgs[firstIdx];
      const splitMessages = segments.map((seg, i) => buildSegMessage(template, seg, i));
      const insertAt = msgs.slice(0, firstIdx).filter(m => !isThisTurn(m.id)).length;
      const kept = msgs.filter(m => !isThisTurn(m.id));
      kept.splice(insertAt, 0, ...splitMessages);
      return kept;
    });

    // Sync pathState.fullActivePath the same way.
    const apFirstIdx = pathState.fullActivePath.findIndex(m => isThisTurn(m.id));
    if (apFirstIdx >= 0) {
      const template = pathState.fullActivePath[apFirstIdx];
      const splitAp = segments.map((seg, i) => buildSegMessage(template, seg, i));
      const removedCount = pathState.fullActivePath.filter(m => isThisTurn(m.id)).length;
      const insertAt = pathState.fullActivePath.slice(0, apFirstIdx).filter(m => !isThisTurn(m.id)).length;
      const keptAp = pathState.fullActivePath.filter(m => !isThisTurn(m.id));
      keptAp.splice(insertAt, 0, ...splitAp);
      pathState.fullActivePath = keptAp;
      // N-bubble turn became segments.length bubbles — keep
      // pathState.currentRenderCount in step with fullActivePath's new
      // length, or pagination math on the next loadMoreMessages() call
      // desyncs from what's actually rendered.
      pathState.currentRenderCount += splitAp.length - removedCount;
    }
  });

  return unlistenMultiChar;
}

/**
 * Tears down the multi-character response listener.
 * Called during app cleanup or when no longer needed.
 */
export function cleanupMultiCharListener() {
  if (unlistenMultiChar) {
    unlistenMultiChar();
    unlistenMultiChar = null;
  }
}
