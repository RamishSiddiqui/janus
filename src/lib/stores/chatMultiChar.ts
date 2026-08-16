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

    // Multiple segments — reconcile with PresentationBuffer's pre-created bubbles.
    // The buffer creates messages with IDs: parentId__seg0, __seg1, etc.
    // If those exist, just update their content/attribution. If not (buffer
    // didn't detect markers), fall back to the old split-replace behavior.
    const currentMsgs = get(messages);
    const liveSegExists = currentMsgs.some(m => m.id === `${parentId}__seg0`);

    if (liveSegExists) {
      // ── Reconcile: update pre-existing segment bubbles ──
      messages.update(msgs =>
        msgs.map(m => {
          // Check if this message is a live segment for this parent
          for (let i = 0; i < segments.length; i++) {
            const segId = `${parentId}__seg${i}`;
            if (m.id === segId) {
              const seg = segments[i];
              const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
              return {
                ...m,
                content: seg.content,
                character_name: seg.character_name,
                character_id: seg.character_id || null,
                character_avatar_url: meta?.avatarUrl ?? m.character_avatar_url ?? null,
                isStreaming: false,
              };
            }
          }
          return m;
        })
      );
      // Sync pathState.fullActivePath
      for (let i = 0; i < pathState.fullActivePath.length; i++) {
        const m = pathState.fullActivePath[i];
        for (let j = 0; j < segments.length; j++) {
          if (m.id === `${parentId}__seg${j}`) {
            const seg = segments[j];
            const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
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
      }
    } else {
      // ── Fallback: buffer didn't create segments (no markers detected during stream) ──
      // Split the combined parent message into N individual per-character bubbles.
      messages.update(msgs => {
        const parentIdx = msgs.findIndex(m => m.id === parentId);
        if (parentIdx < 0) return msgs;

        const parent = msgs[parentIdx];
        const splitMessages: typeof msgs = segments.map((seg, i) => {
          const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
          return {
            ...parent,
            id: `${parentId}__seg${i}`,
            content: seg.content,
            character_name: seg.character_name,
            character_id: seg.character_id || null,
            character_avatar_url: meta?.avatarUrl ?? null,
            siblingIds: i === 0 ? parent.siblingIds : undefined,
            siblingIndex: i === 0 ? parent.siblingIndex : undefined,
            siblingConversationIds: i === 0 ? parent.siblingConversationIds : undefined,
            siblingConversationIndex: i === 0 ? parent.siblingConversationIndex : undefined,
            isStreaming: false,
          };
        });

        const updated = [...msgs];
        updated.splice(parentIdx, 1, ...splitMessages);
        return updated;
      });

      // Sync pathState.fullActivePath
      const apIdx = pathState.fullActivePath.findIndex(m => m.id === parentId);
      if (apIdx >= 0) {
        const parent = pathState.fullActivePath[apIdx];
        const splitAp: typeof pathState.fullActivePath = segments.map((seg, i) => {
          const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
          return {
            ...parent,
            id: `${parentId}__seg${i}`,
            content: seg.content,
            character_name: seg.character_name,
            character_id: seg.character_id || null,
            character_avatar_url: meta?.avatarUrl ?? null,
            siblingIds: i === 0 ? parent.siblingIds : undefined,
            siblingIndex: i === 0 ? parent.siblingIndex : undefined,
            siblingConversationIds: i === 0 ? parent.siblingConversationIds : undefined,
            siblingConversationIndex: i === 0 ? parent.siblingConversationIndex : undefined,
            isStreaming: false,
          };
        });
        pathState.fullActivePath.splice(apIdx, 1, ...splitAp);
        // 1 message became splitAp.length messages — keep pathState.currentRenderCount
        // in step with pathState.fullActivePath's new length, or pagination math on the
        // next loadMoreMessages() call desyncs from what's actually rendered.
        pathState.currentRenderCount += splitAp.length - 1;
      }
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
