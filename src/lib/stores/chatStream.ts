// ============================================================
//   Janus — Chat Streaming Pipeline
//   Sends messages, retries failed ones, and regenerates responses,
//   all via the same streaming event contract with the backend.
//   Split out of chat.ts.
// ============================================================

import { get } from 'svelte/store';
import type { Message } from '$lib/types';
import { browser } from '$app/environment';
import { error as toastError } from '$lib/stores/toast';
import { humanizeProviderError } from '$lib/utils/providerError';
import { settings } from '$lib/stores/settings';
import { PresentationBuffer, type CharMeta, type PresentationCallbacks } from '$lib/services/presentation-buffer';
import {
  activeCharacterId, activeConversation, activeConversationId, conversationCharMeta,
  isStreaming, lastStreamError, messages, pathState,
} from './chat';
import { runEmotionUpdatePipeline } from './chatEmotion';
import { loadMessages } from './chatMessages';

const isTauri = browser && '__TAURI_INTERNALS__' in window;

/// ── Presentation Buffer Factory ──
// Creates a PresentationBuffer per stream with callbacks wired to the messages store.
// Replaces the old StreamBuffer — provides character-aware streaming with
// pre-resolved avatars, mid-stream marker detection, and per-character bubbles.

function createPresentationCallbacks(): PresentationCallbacks {
  return {
    createMessage(msg: Message) {
      messages.update(msgs => {
        if (msgs.find(m => m.id === msg.id)) return msgs;
        // Sync backing array for pagination
        if (!pathState.fullActivePath.find(m => m.id === msg.id)) {
          pathState.fullActivePath.push(msg);
          pathState.currentRenderCount++;
        }
        return [...msgs, msg];
      });
    },
    appendContent(messageId: string, text: string) {
      messages.update(msgs => {
        // Find the message — may be last or earlier (multi-char creates multiple)
        const idx = msgs.findIndex(m => m.id === messageId);
        if (idx < 0) return msgs;
        const msg = msgs[idx];
        const updated = { ...msg, content: msg.content + text, isStreaming: true };
        const next = msgs.slice();
        next[idx] = updated;
        return next;
      });
    },
    appendReasoning(messageId: string, text: string) {
      messages.update(msgs => {
        const idx = msgs.findIndex(m => m.id === messageId);
        if (idx < 0) return msgs;
        const msg = msgs[idx];
        const updated = { ...msg, reasoning: (msg.reasoning ?? '') + text };
        const next = msgs.slice();
        next[idx] = updated;
        return next;
      });
    },
    markThinkingDone(messageId: string) {
      messages.update(msgs => msgs.map(m => m.id === messageId ? { ...m, isThinking: false } : m));
    },
    finalizeMessage(messageId: string) {
      messages.update(msgs =>
        msgs.map(m => m.id === messageId ? { ...m, isStreaming: false } : m)
      );
    },
  };
}

/** Build a PresentationBuffer with the current conversation's character metadata. */
function createStreamBuffer(): PresentationBuffer {
  const meta = get(conversationCharMeta);
  const primaryId = get(activeCharacterId);
  const primaryMeta = primaryId ? meta.get(primaryId) : null;
  const fallback: CharMeta = primaryMeta || {
    id: primaryId || '',
    name: get(activeConversation)?.characterName || 'Character',
    avatarUrl: null,
    accentColor: '#8B5CF6',
  };
  return new PresentationBuffer(fallback, meta, createPresentationCallbacks());
}

// Active stream buffer — created fresh per stream
let activeBuffer: PresentationBuffer | null = null;

/**
 * Stops the in-flight generation for the active conversation, if any.
 * Whatever content had already streamed is preserved (saved server-side,
 * kept on screen) rather than discarded — a cancelled response is not
 * treated as a failure, so no retry banner appears.
 */
export async function cancelGeneration() {
  const convId = get(activeConversationId);
  if (!isTauri || !convId) return;
  try {
    const ipc = await import('$lib/services/ipc');
    await ipc.cancelGeneration(convId);
  } catch (err) {
    console.error('Failed to cancel generation:', err);
  }
}

/**
 * Client-side safety net for chat streams. The backend now gives up on a
 * stalled provider on its own (see RigProvider::generate_stream's
 * STREAM_EVENT_TIMEOUT) and emits a real 'error' event when it does — but
 * this guards against the rarer case where that event never reaches the
 * frontend at all (a dropped Tauri event, a lost listener). Without it,
 * `isStreaming` would stay true forever with nothing for the user to do but
 * restart the app. Call `.reset()` on every stream event received (or once
 * right after registering the listener, to start the clock); if
 * `timeoutMs` passes with total silence, `onTimeout` fires once. Set well
 * past the backend's own 90s timeout so a real backend error always wins
 * the race and produces the normal, more specific error message.
 */
function createStreamWatchdog(onTimeout: () => void, timeoutMs = 105_000) {
  let handle: ReturnType<typeof setTimeout> | undefined;
  return {
    reset() {
      if (handle) clearTimeout(handle);
      handle = setTimeout(onTimeout, timeoutMs);
    },
    clear() {
      if (handle) { clearTimeout(handle); handle = undefined; }
    },
  };
}

/** Sends a user message and initiates streaming response from the backend. */
export async function sendMessage(
  conversationId: string,
  content: string,
  model?: string,
  attachments?: { relativePath: string; mimeType: string }[],
) {
  if (!isTauri) {
    // Dev mode — just add user message locally
    messages.update(msgs => [...msgs, {
      id: crypto.randomUUID(),
      role: 'user' as const,
      content,
      attachments,
    }]);
    return;
  }

  const ipc = await import('$lib/services/ipc');
  const tempUserId = crypto.randomUUID();
  // Declared outside the try block (not `const unlisten = ...` inside it) so
  // the catch block below can reach it too — if the backend call throws
  // AFTER the listener was registered, the listener must still be torn down,
  // or it stays subscribed to the shared chat-stream channel forever and
  // double-processes the next real generation in this conversation once the
  // user fixes whatever failed and retries (visibly duplicating the
  // streamed reply).
  let unlisten: (() => void) | undefined;
  // Same reasoning as `unlisten` above — declared outside try so the catch
  // block can silence a pending watchdog timer if sendMessage itself throws
  // after the listener (and watchdog) were already set up.
  let watchdog: ReturnType<typeof createStreamWatchdog> | undefined;
  try {
    // Add user message to local state immediately for responsiveness
    const userMsg: Message = {
      id: tempUserId,
      role: 'user' as const,
      content,
      attachments,
    };
    messages.update(msgs => [...msgs, userMsg]);
    pathState.fullActivePath.push(userMsg);
    pathState.currentRenderCount++;

    isStreaming.set(true);
    lastStreamError.set(null);

    // Create a fresh PresentationBuffer for this stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    watchdog = createStreamWatchdog(() => {
      console.error('[Janus] Stream watchdog fired — no event from backend for 105s');
      buffer.reset();
      toastError('AI response timed out — no reply from the backend. Please try again.');
      const timeoutMsgId = crypto.randomUUID();
      const timeoutMsg: Message = { id: timeoutMsgId, role: 'assistant' as const, content: 'No response received (timed out).', isError: true };
      messages.update(msgs => [...msgs, timeoutMsg]);
      pathState.fullActivePath.push(timeoutMsg);
      pathState.currentRenderCount++;
      const realUserMsgId = get(messages).filter(m => m.role === 'user').pop()?.id;
      lastStreamError.set({ conversationId, lastUserContent: content, userMessageId: realUserMsgId });
      isStreaming.set(false);
      unlisten?.();
    });

    // Set up stream listener BEFORE sending
    unlisten = await ipc.onChatStream((event) => {
      // Guard: if the user switched to a different conversation, discard stale events
      if (get(activeConversationId) !== conversationId) {
        if (event.event_type === 'done' || event.event_type === 'error' || event.event_type === 'cancelled') unlisten?.();
        return;
      }
      watchdog?.reset();

      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
      } else if (event.event_type === 'reasoning') {
        buffer.pushReasoning(event.message_id, event.content);
      } else if (event.event_type === 'cancelled') {
        // User stopped generation early — finalize like 'done' (lock in
        // whatever streamed so far, clear streaming state), but skip the
        // auto-memory/emotion pipelines below since there's no complete
        // response to extract anything meaningful from.
        buffer.finalize();
        messages.update(msgs => msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m));
        for (let i = 0; i < pathState.fullActivePath.length; i++) {
          if (pathState.fullActivePath[i].isStreaming) pathState.fullActivePath[i] = { ...pathState.fullActivePath[i], isStreaming: false };
        }
        watchdog?.clear();
        isStreaming.set(false);
        unlisten?.();
      } else if (event.event_type === 'done') {
        // Finalize the presentation buffer — flushes remaining content,
        // marks all active bubbles as done streaming
        buffer.finalize();

        // PresentationBuffer already handled all message creation, content
        // appending, and finalization. No need to overwrite content here.
        // Just ensure any remaining streaming flags are cleared.
        messages.update(msgs =>
          msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m)
        );
        watchdog?.clear();
        isStreaming.set(false);
        unlisten?.();

        // --- Auto-save memories pipeline ---
        // The per-conversation memory_scope is the single source of truth.
        // If memory is enabled on this conversation, extraction runs regardless
        // of the global autoSaveMemories default.
        if (event.content) {
          (async () => {
            try {
              // Check per-conversation memory scope FIRST (the authority)
              const ipcMod = await import('$lib/services/ipc');
              const conv = await ipcMod.getConversation(conversationId);
              if (conv.memory_scope === 'none') return; // Memory disabled for this chat

              const { shouldExtract, extractAndSaveMemories } = await import('$lib/services/memory-extractor');
              if (!shouldExtract()) return; // Throttle: only every Nth message

              const saved = await extractAndSaveMemories(
                conversationId,
                conv.memory_scope === 'character' ? (conv.character_id ?? undefined) : undefined,
                content,           // user's message
                event.content,     // assistant's response
              );
              if (saved > 0) {
                console.debug(`[Janus] Auto-saved ${saved} memor${saved === 1 ? 'y' : 'ies'} (scope: ${conv.memory_scope})`);
              }
            } catch (err) {
              console.warn('[Janus] Auto-memory extraction failed:', err);
            }
          })();
        }

        // --- Emotional state update pipeline ---
        // Runs fire-and-forget after every response regardless of memory scope
        // setting, updating (and snapshotting) every character in the cast —
        // not just whoever spoke. Multi-speaker turns and solo NPC lines are
        // handled by the multi-char-response listener instead (this event's
        // content is emptied for those — see the "done" emit in chat.rs).
        if (event.content) {
          runEmotionUpdatePipeline(conversationId, content, event.content, [event.message_id]);
        }

      } else if (event.event_type === 'error') {
        buffer.reset();
        console.error('Stream error:', event.content);
        toastError(`AI response failed: ${humanizeProviderError(event.content)}`);
        // Mark or create the assistant message as failed so UI shows the error bubble
        messages.update(msgs => {
          const exists = msgs.some(m => m.id === event.message_id);
          if (exists) {
            return msgs.map(m => m.id === event.message_id
              ? { ...m, isStreaming: false, isError: true, content: event.content || 'Generation failed' }
              : m
            );
          } else {
            return [...msgs, { id: event.message_id, role: 'assistant' as const, content: event.content || 'Generation failed', isStreaming: false, isError: true }];
          }
        });

        const assistantMsg: Message = { id: event.message_id, role: 'assistant', content: event.content || 'Generation failed', isError: true };
        const apIdx = pathState.fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) pathState.fullActivePath[apIdx] = assistantMsg;
        else { pathState.fullActivePath.push(assistantMsg); pathState.currentRenderCount++; }
        // The real user_message_id was set after sendMessage returned (line ~706)
        // Grab it from the current messages array to pass to retry
        const realUserMsgId = get(messages).filter(m => m.role === 'user').pop()?.id;
        lastStreamError.set({ conversationId, lastUserContent: content, userMessageId: realUserMsgId });
        watchdog?.clear();
        isStreaming.set(false);
        unlisten?.();
      }
    });
    watchdog?.reset();

    // Send the message — backend will stream/generate response via events
    const currentSettings = get(settings);
    const result = await ipc.sendMessage(
      conversationId, content, model,
      currentSettings.systemPrompt || undefined,
      currentSettings.streamingEnabled,
      currentSettings.postHistoryInstructions || undefined,
      attachments,
    );

    // Replace temp user message ID with real one from backend
    messages.update(msgs =>
      msgs.map(m => m.id === tempUserId ? { ...m, id: result.user_message_id } : m)
    );
    // Sync pathState.fullActivePath
    const fpIdx = pathState.fullActivePath.findIndex(m => m.id === tempUserId);
    if (fpIdx >= 0) pathState.fullActivePath[fpIdx] = { ...pathState.fullActivePath[fpIdx], id: result.user_message_id };
  } catch (err) {
    console.error('Failed to send message:', err);
    watchdog?.clear();
    unlisten?.();
    const msg = (err as any)?.message ?? 'Failed to send message. Is a provider configured?';
    toastError(msg);
    // Mark the user message as failed so UI shows a retry button
    messages.update(msgs =>
      msgs.map(m => m.id === tempUserId
        ? { ...m, isError: true, content: content }
        : m
      )
    );
    lastStreamError.set({ conversationId, lastUserContent: content }); // no userMessageId — sendMessage itself failed, message may not be in DB
    isStreaming.set(false);
  }
}

/**
 * Retries the last failed message.
 * If the user message was already saved to the DB (stream error), reuses it
 * via retry_failed_message. If sendMessage itself failed before saving,
 * falls back to a fresh sendMessage call.
 */
export async function retryLastMessage(model?: string) {
  const err = get(lastStreamError);
  if (!err) return;
  lastStreamError.set(null);

  if (err.userMessageId) {
    // Message exists in DB — remove the failed assistant bubble from UI, keep user message
    messages.update(msgs => msgs.filter(m => !(m.isError && m.role === 'assistant')));
    const beforeCount = pathState.fullActivePath.length;
    pathState.fullActivePath = pathState.fullActivePath.filter(m => !(m.isError && m.role === 'assistant'));
    pathState.currentRenderCount -= beforeCount - pathState.fullActivePath.length;

    // Clear error state on the user message
    messages.update(msgs => msgs.map(m => m.id === err.userMessageId ? { ...m, isError: false } : m));

    const ipc = await import('$lib/services/ipc');
    isStreaming.set(true);

    // Create a fresh PresentationBuffer for this retry stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    const watchdog = createStreamWatchdog(() => {
      console.error('[Janus] Stream watchdog fired — no event from backend for 105s');
      buffer.reset();
      toastError('AI response timed out — no reply from the backend. Please try again.');
      const timeoutMsgId = crypto.randomUUID();
      const timeoutMsg: Message = { id: timeoutMsgId, role: 'assistant' as const, content: 'No response received (timed out).', isError: true };
      messages.update(msgs => [...msgs, timeoutMsg]);
      pathState.fullActivePath.push(timeoutMsg);
      pathState.currentRenderCount++;
      lastStreamError.set({ conversationId: err.conversationId, lastUserContent: err.lastUserContent, userMessageId: err.userMessageId });
      isStreaming.set(false);
      unlisten();
    });

    // Set up stream listener
    const unlisten = await ipc.onChatStream((event) => {
      if (get(activeConversationId) !== err.conversationId) {
        if (event.event_type === 'done' || event.event_type === 'error' || event.event_type === 'cancelled') unlisten();
        return;
      }
      watchdog.reset();

      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
      } else if (event.event_type === 'reasoning') {
        buffer.pushReasoning(event.message_id, event.content);
      } else if (event.event_type === 'cancelled') {
        buffer.finalize();
        messages.update(msgs => msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m));
        for (let i = 0; i < pathState.fullActivePath.length; i++) {
          if (pathState.fullActivePath[i].isStreaming) pathState.fullActivePath[i] = { ...pathState.fullActivePath[i], isStreaming: false };
        }
        watchdog.clear();
        isStreaming.set(false);
        unlisten();
      } else if (event.event_type === 'done') {
        buffer.finalize();
        // Buffer handles all message creation/content — just clear streaming state
        messages.update(msgs =>
          msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m)
        );
        watchdog.clear();
        isStreaming.set(false);
        unlisten();
      } else if (event.event_type === 'error') {
        buffer.reset();
        toastError(`AI response failed: ${humanizeProviderError(event.content)}`);
        messages.update(msgs => {
          const exists = msgs.some(m => m.id === event.message_id);
          if (exists) {
            return msgs.map(m => m.id === event.message_id
              ? { ...m, isStreaming: false, isError: true, content: event.content || 'Generation failed' }
              : m
            );
          } else {
            return [...msgs, { id: event.message_id, role: 'assistant' as const, content: event.content || 'Generation failed', isStreaming: false, isError: true }];
          }
        });
        const assistantMsg: Message = { id: event.message_id, role: 'assistant', content: event.content || 'Generation failed', isError: true };
        const apIdx = pathState.fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) pathState.fullActivePath[apIdx] = assistantMsg;
        else { pathState.fullActivePath.push(assistantMsg); pathState.currentRenderCount++; }

        lastStreamError.set({ conversationId: err.conversationId, lastUserContent: err.lastUserContent, userMessageId: err.userMessageId });
        watchdog.clear();
        isStreaming.set(false);
        unlisten();
      }
    });
    watchdog.reset();

    try {
      const currentSettings = get(settings);
      await ipc.retryFailedMessage(
        err.conversationId, err.userMessageId, model,
        currentSettings.systemPrompt || undefined,
        currentSettings.streamingEnabled,
        currentSettings.postHistoryInstructions || undefined,
      );
    } catch (retryErr) {
      console.error('Retry failed:', retryErr);
      watchdog.clear();
      unlisten();
      toastError('Retry failed — please try again');
      isStreaming.set(false);
    }
  } else {
    // sendMessage itself failed before the message was saved — clean up and resend
    messages.update(msgs => msgs.filter(m => !m.isError));
    const beforeCount = pathState.fullActivePath.length;
    pathState.fullActivePath = pathState.fullActivePath.filter(m => !m.isError);
    pathState.currentRenderCount -= beforeCount - pathState.fullActivePath.length;
    await sendMessage(err.conversationId, err.lastUserContent, model);
  }
}

/** Regenerates the last assistant response, streaming the new content. */
export async function regenerateMessage(conversationId: string, messageId: string, model?: string) {
  if (!isTauri || get(isStreaming)) return;

  const ipc = await import('$lib/services/ipc');
  isStreaming.set(true);

  // Declared outside the try (see the matching comment in `sendMessage`) so
  // the catch block can still tear the listener down if `ipc.regenerateMessage`
  // throws after the listener was already registered.
  let unlisten: (() => void) | undefined;
  let watchdog: ReturnType<typeof createStreamWatchdog> | undefined;
  try {
    // Create a fresh PresentationBuffer for this regeneration stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    // Same ancestor-walk the 'error' branch below uses, so a watchdog
    // timeout surfaces the Retry banner on the right user message too.
    function findAncestorUserMessageId(): string | undefined {
      let ancestor = pathState.fullActivePath.find(m => m.id === messageId);
      while (ancestor && ancestor.role !== 'user') {
        ancestor = ancestor.parent_id ? pathState.fullActivePath.find(m => m.id === ancestor!.parent_id) : undefined;
      }
      return ancestor?.id;
    }

    watchdog = createStreamWatchdog(() => {
      console.error('[Janus] Stream watchdog fired — no event from backend for 105s');
      buffer.reset();
      toastError('Regeneration timed out — no reply from the backend. Please try again.');
      const timeoutMsgId = crypto.randomUUID();
      const timeoutMsg: Message = { id: timeoutMsgId, role: 'assistant' as const, content: 'No response received (timed out).', isError: true };
      messages.update(msgs => [...msgs, timeoutMsg]);
      pathState.fullActivePath.push(timeoutMsg);
      pathState.currentRenderCount++;
      const ancestorId = findAncestorUserMessageId();
      if (ancestorId) {
        lastStreamError.set({ conversationId, lastUserContent: '', userMessageId: ancestorId });
      }
      isStreaming.set(false);
      unlisten?.();
    });

    // Set up stream listener BEFORE triggering regeneration
    unlisten = await ipc.onChatStream((event) => {
      watchdog?.reset();
      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
      } else if (event.event_type === 'reasoning') {
        buffer.pushReasoning(event.message_id, event.content);
      } else if (event.event_type === 'done' || event.event_type === 'cancelled') {
        buffer.finalize();
        // Buffer handles all message creation/content — just clear streaming state
        messages.update(msgs =>
          msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m)
        );
        watchdog?.clear();
        isStreaming.set(false);
        unlisten?.();
        // Reload messages to get sibling info (also re-syncs pathState.fullActivePath
        // from the DB, which already has the cancelled partial content saved)
        loadMessages(conversationId);
      } else if (event.event_type === 'error') {
        buffer.reset();
        console.error('Regeneration stream error:', event.content);
        toastError(`Regeneration failed: ${humanizeProviderError(event.content)}`);
        messages.update(msgs => {
          const exists = msgs.some(m => m.id === event.message_id);
          if (exists) {
            return msgs.map(m => m.id === event.message_id
              ? { ...m, isStreaming: false, isError: true, content: event.content || 'Generation failed' }
              : m
            );
          } else {
            return [...msgs, { id: event.message_id, role: 'assistant' as const, content: event.content || 'Generation failed', isStreaming: false, isError: true }];
          }
        });
        // Mirror into pathState.fullActivePath — unlike sendMessage/retryLastMessage's
        // identical error handlers, this one previously only updated the
        // rendered `messages` window, leaving pathState.fullActivePath (the pagination
        // source of truth) unaware the regenerated message ever failed.
        const assistantMsg: Message = { id: event.message_id, role: 'assistant', content: event.content || 'Generation failed', isError: true };
        const apIdx = pathState.fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) pathState.fullActivePath[apIdx] = assistantMsg;
        else { pathState.fullActivePath.push(assistantMsg); pathState.currentRenderCount++; }

        // Surface the Retry banner, same as a failed send. Walk up from the
        // regenerated message to the user message that prompted it (its
        // direct parent in single-character chats; possibly further up
        // through sibling assistant segments in multi-character turns).
        const ancestorId = findAncestorUserMessageId();
        if (ancestorId) {
          lastStreamError.set({ conversationId, lastUserContent: '', userMessageId: ancestorId });
        }

        watchdog?.clear();
        isStreaming.set(false);
        unlisten?.();
      }
    });
    watchdog?.reset();

    const currentSettings = get(settings);
    await ipc.regenerateMessage(
      conversationId, messageId, model,
      currentSettings.systemPrompt || undefined,
      currentSettings.streamingEnabled,
      currentSettings.postHistoryInstructions || undefined,
    );
  } catch (err) {
    console.error('Failed to regenerate:', err);
    watchdog?.clear();
    unlisten?.();
    const msg = (err as any)?.message ?? 'Failed to regenerate response';
    toastError(msg);
    isStreaming.set(false);
  }
}
