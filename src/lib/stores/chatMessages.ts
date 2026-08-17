// ============================================================
//   Janus — Chat Message Loading & Branch Navigation
//   Loads the active branch path for a conversation (with pagination,
//   sibling/branch annotation, and per-character metadata pre-resolution),
//   and switches between sibling branches. Split out of chat.ts.
// ============================================================

import { get } from 'svelte/store';
import type { CharacterState } from '$lib/services/ipc';
import { charAccentColor, type CharMeta } from '$lib/services/presentation-buffer';
import { error as toastError, undoableDelete } from '$lib/stores/toast';
import { browser } from '$app/environment';
import {
  activeConversationId, characterEmotionStates, conversationCharMeta, conversations,
  hasMoreMessages, isLoadingMoreMessages, isStreaming, lastStreamError, loadConversations,
  MESSAGE_RENDER_SIZE, messages, pathState, resetPaginationState, resolveCachedAvatarUrl,
} from './chat';
import { parseEmotionSnapshot } from './chatEmotion';

const isTauri = browser && '__TAURI_INTERNALS__' in window;

/** Extracts `{ relativePath, mimeType }[]` from a stored message's raw
 *  `metadata` JSON (`{"attachments": [...]}`, see backend `MessageAttachment`)
 *  — used when reloading conversation history so attachment thumbnails
 *  persist across a reload, not just on the freshly-sent optimistic message. */
function parseAttachments(metadata: unknown): { relativePath: string; mimeType: string }[] | undefined {
  const raw = (metadata as { attachments?: unknown } | null | undefined)?.attachments;
  if (!Array.isArray(raw)) return undefined;
  const parsed = raw.filter(
    (a): a is { relativePath: string; mimeType: string } =>
      typeof a === 'object' && a !== null &&
      typeof (a as Record<string, unknown>).relativePath === 'string' &&
      typeof (a as Record<string, unknown>).mimeType === 'string'
  );
  return parsed.length > 0 ? parsed : undefined;
}

/** Loads messages for the active conversation, showing only the active branch chain. */
export async function loadMessages(conversationId: string) {
  // Reset auto-memory throttle on conversation switch
  import('$lib/services/memory-extractor').then(m => m.resetCounter()).catch(() => {});

  if (!isTauri) {
    // Dev mode — keep existing mock messages
    return;
  }

  const ipc = await import('$lib/services/ipc');
  try {
    // Flush immediately so stale messages don't show during load
    messages.set([]);
    resetPaginationState();

    // Fetch all messages AND the conversation (for active_message_id) in parallel
    const [msgs, conv] = await Promise.all([
      ipc.getConversationMessages(conversationId),
      ipc.getConversation(conversationId),
    ]);

    // Build lookup maps
    const byId = new Map(msgs.map(m => [m.id, m]));

    // Group all messages by parent_id to compute sibling counts at each branch point
    const byParent = new Map<string, typeof msgs>();
    for (const m of msgs) {
      const key = m.parent_id ?? '__root__';
      if (!byParent.has(key)) byParent.set(key, []);
      byParent.get(key)!.push(m);
    }

    // Walk BACKWARD from active_message_id to build the active branch chain (root → tip)
    // If no active_message_id is set, fall back to showing all root-level messages as-is
    const activeTipId = conv.active_message_id;
    let activePath: typeof msgs;

    if (activeTipId && byId.has(activeTipId)) {
      // Walk from tip → root collecting ancestor IDs
      const pathIds: string[] = [];
      let current: string | null = activeTipId;
      const visited = new Set<string>();
      while (current && byId.has(current) && !visited.has(current)) {
        visited.add(current);
        pathIds.unshift(current);
        current = byId.get(current)!.parent_id;
      }
      activePath = pathIds.map(id => byId.get(id)!);

      // ── Expand multi-char sibling segments ──
      // The backward walk only collects the ancestor chain. If active_message_id
      // points to Finn's segment, Aria's segment (same parent_id) is skipped.
      // We need to include ALL sibling segments with character_name at each level.
      const expanded: typeof msgs = [];
      for (const msg of activePath) {
        if (msg.character_name && msg.parent_id) {
          // Check if there are sibling segments with the same parent
          const siblings = byParent.get(msg.parent_id) ?? [];
          const charSiblings = siblings.filter(s => s.character_name);
          if (charSiblings.length > 1) {
            // Include all character segments in order, but only if we haven't already
            for (const seg of charSiblings) {
              if (!expanded.find(e => e.id === seg.id)) {
                expanded.push(seg);
              }
            }
            continue; // Skip the single push below
          }
        }
        if (!expanded.find(e => e.id === msg.id)) {
          expanded.push(msg);
        }
      }
      activePath = expanded;
    } else {
      // Fallback: show only the first child at each branch level (depth-first active path)
      activePath = [];
      let currentParentKey = '__root__';
      while (byParent.has(currentParentKey)) {
        const children = byParent.get(currentParentKey)!;
        // For multi-char segments (messages with character_name), include ALL
        // siblings — they're sequential dialogue, not branches. Pick the first
        // non-segment child to continue the tree walk.
        const charSegments = children.filter(c => c.character_name);
        if (charSegments.length > 1) {
          // Old data: all segments are siblings. Push them all in order.
          for (const seg of charSegments) {
            activePath.push(seg);
          }
          // Continue from the last segment
          currentParentKey = charSegments[charSegments.length - 1].id;
        } else {
          const next = children[0];
          activePath.push(next);
          currentParentKey = next.id;
        }
      }
    }

    // Annotate each message on the active path with sibling navigator info.
    // Two layers: (a) in-conversation siblings (same parent_id within this conversation),
    // (b) cross-conversation siblings (other conversations that branched at the same point).

    // --- (b) Cross-conversation branch detection ---
    // Ensure $conversations is fresh so branchPointMessageId is available.
    // We do a lightweight reload here when this is a branch or might have children.
    // This is async-safe: we await it before reading get(conversations).
    if (conv.parent_conversation_id) {
      // This is a branch — ensure parent + siblings are in the store
      await loadConversations();
    }

    // Read conversations after potential refresh
    const allConvPreviews = get(conversations);

    // Map: messageId-in-THIS-conversation → { siblingConversationIds, index }
    const convSiblingOverrides = new Map<string, { ids: string[]; index: number }>();

    // Case 1: this conversation IS a branch (has parent_conversation_id)
    if (conv.parent_conversation_id && conv.branch_point_message_id) {
      // Find all conversations that branched from the same parent at the same point
      const otherBranches = allConvPreviews.filter(c =>
        c.parentConversationId === conv.parent_conversation_id &&
        c.branchPointMessageId === conv.branch_point_message_id &&
        c.id !== conversationId
      );

      if (otherBranches.length > 0 || true /* parent itself is always a sibling */) {
        // Ordered list: [parent, ...other branches sorted by id, this branch]
        const parentId = conv.parent_conversation_id;
        const sortedOtherBranches = otherBranches.map(c => c.id).sort();
        // Build ordered list: parent first, then branches by insertion order
        const allBranchIds = [parentId, ...sortedOtherBranches.filter(id => id !== conversationId), conversationId];
        // Remove duplicates (in case this somehow already appeared)
        const uniqueIds = [...new Set(allBranchIds)];
        const myIndex = uniqueIds.indexOf(conversationId);

        // Find the divergence message in THIS conversation's activePath.
        // Strategy: fetch parent messages to count chain length up to branch_point.
        // We do this ONE async fetch here (only for branched conversations).
        try {
          const parentMsgs = await ipc.getConversationMessages(parentId);
          const parentById = new Map(parentMsgs.map(m => [m.id, m]));
          // Walk parent chain from branch_point_message_id → root to get chain length
          let chainLen = 0;
          let cur: string | null = conv.branch_point_message_id;
          const vis = new Set<string>();
          while (cur && parentById.has(cur) && !vis.has(cur)) {
            vis.add(cur);
            chainLen++;
            cur = parentById.get(cur)!.parent_id;
          }
          // In activePath, the divergence message is at index chainLen
          // (indices 0…chainLen-1 are copies, chainLen is the first new user message)
          const divergeMsg = activePath[chainLen];
          if (divergeMsg) {
            convSiblingOverrides.set(divergeMsg.id, { ids: uniqueIds, index: myIndex });
          }
        } catch {
          // If parent fetch fails, fall back gracefully — no navigator shown
        }
      }
    }

    // Case 2: this conversation IS the parent — annotate the branch-point message
    // Refresh conversations if no children found yet (might be first load after branching)
    let childBranches = allConvPreviews.filter(c => c.parentConversationId === conversationId);
    if (childBranches.length === 0 && !conv.parent_conversation_id) {
      // No children seen yet and not a branch itself — do a quick refresh to find any new children
      await loadConversations();
      childBranches = get(conversations).filter(c => c.parentConversationId === conversationId);
    }
    if (childBranches.length > 0) {
      // Group children by branchPointMessageId (there may be multiple branch points)
      const byBranchPoint = new Map<string, string[]>();
      for (const child of childBranches) {
        if (!child.branchPointMessageId) continue;
        if (!byBranchPoint.has(child.branchPointMessageId)) byBranchPoint.set(child.branchPointMessageId, []);
        byBranchPoint.get(child.branchPointMessageId)!.push(child.id);
      }
      for (const [bpMsgId, childIds] of byBranchPoint) {
        // Find bpMsgId in activePath
        const bpIdx = activePath.findIndex(m => m.id === bpMsgId);
        if (bpIdx === -1) continue;
        // The divergence point is the message AFTER the branch_point — branch_point is the last
        // message copied identically into each branch; the NEXT message is where they differ.
        // e.g. if branched from the AI greeting, annotate the user reply (learn/teach magic).
        // Fall back to bpIdx if there's no next message (branch at very last message).
        const divergeIdx = bpIdx + 1 < activePath.length ? bpIdx + 1 : bpIdx;
        const divergeMsg = activePath[divergeIdx];
        // Ordered: [this conversation (parent, index 0), ...child branches sorted]
        const sortedChildIds = [...childIds].sort();
        const uniqueIds = [conversationId, ...sortedChildIds];
        convSiblingOverrides.set(divergeMsg.id, { ids: uniqueIds, index: 0 });
      }
    }

    // Build full annotated message list (kept in memory for scroll-up pagination)
    let foundStreamError = false;
    let streamErrorState: { conversationId: string, lastUserContent: string, userMessageId?: string } | null = null;

    pathState.fullActivePath = activePath.map((m, index) => {
      const key = m.parent_id ?? '__root__';
      const siblings = byParent.get(key) ?? [];
      const siblingIndex = siblings.findIndex(s => s.id === m.id);
      const convSibling = convSiblingOverrides.get(m.id);

      // Multi-character segments (messages with character_name) are sequential
      // dialogue, NOT branch alternates. Exclude them from sibling navigation.
      const isCharSegment = !!m.character_name;
      // Real branch siblings: exclude multi-char segments from count
      const realSiblings = siblings.filter(s => !s.character_name);
      const realSiblingIndex = realSiblings.findIndex(s => s.id === m.id);
      const hasRealSiblings = !isCharSegment && realSiblings.length > 1;

      // Check if this is a failed assistant message (empty content)
      const isFailedAssistant = m.role === 'assistant' && (!m.content || m.content.trim() === '');

      // If the very last message in the conversation is a failed assistant message,
      // we need to restore the global retry state so the user can easily retry it.
      if (isFailedAssistant && index === activePath.length - 1) {
        foundStreamError = true;
        // Find the preceding user message to grab its ID and content for the retry payload
        const prevUser = activePath[index - 1];
        if (prevUser && prevUser.role === 'user') {
          streamErrorState = {
            conversationId,
            lastUserContent: prevUser.content,
            userMessageId: prevUser.id
          };
        }
      }

      return {
        id: m.id,
        role: m.role as 'user' | 'assistant',
        content: isFailedAssistant ? 'Generation failed' : m.content,
        isError: isFailedAssistant,
        parent_id: m.parent_id,
        character_id: m.character_id || null,
        character_name: m.character_name || null,
        // In-conversation siblings (old message-tree branching) — excludes multi-char segments
        siblingIds: hasRealSiblings ? realSiblings.map(s => s.id) : undefined,
        siblingIndex: hasRealSiblings ? realSiblingIndex : undefined,
        alternates: convSibling
          ? convSibling.ids.length
          : (hasRealSiblings ? realSiblings.length : undefined),
        currentAlternate: convSibling
          ? convSibling.index + 1
          : (hasRealSiblings ? realSiblingIndex + 1 : undefined),
        // Cross-conversation branch siblings
        siblingConversationIds: convSibling?.ids,
        siblingConversationIndex: convSibling?.index,
        reasoning: m.reasoning ?? null,
        attachments: parseAttachments(m.metadata),
        emotionSnapshot: parseEmotionSnapshot(m.metadata),
      };
    });

    // Render only the last N messages (paginated rendering)
    pathState.currentRenderCount = Math.min(pathState.fullActivePath.length, MESSAGE_RENDER_SIZE);
    const initialSlice = pathState.fullActivePath.slice(pathState.fullActivePath.length - pathState.currentRenderCount);
    messages.set(initialSlice);
    hasMoreMessages.set(pathState.currentRenderCount < pathState.fullActivePath.length);

    // Restore retry state if the last message was a failure
    if (foundStreamError && streamErrorState) {
      lastStreamError.set(streamErrorState);
    } else {
      lastStreamError.set(null); // Clear it if the last message is successful
    }

    // Pre-load emotional states for ALL characters for immediate HUD display
    try {
      const stateMap = new Map<string, CharacterState>();
      // Load primary character's state
      const charId = conv.character_id;
      if (charId) {
        const existingState = await ipc.getCharacterState(charId, conversationId);
        if (existingState) stateMap.set(charId, existingState);
      }
      // Load additional characters' states (multi-char conversations)
      try {
        const convChars = await ipc.listConversationCharacters(conversationId);
        for (const cc of convChars) {
          if (cc.character_id && cc.character_id !== charId) {
            try {
              const state = await ipc.getCharacterState(cc.character_id, conversationId);
              if (state) stateMap.set(cc.character_id, state);
            } catch { /* no state yet — skip */ }
          }
        }
      } catch { /* no conversation characters — single char mode */ }
      characterEmotionStates.set(stateMap);
    } catch {
      characterEmotionStates.set(new Map());
    }

    // ── Pre-resolve character metadata for PresentationBuffer ──
    // Build a CharMeta map for all characters in this conversation so that
    // streaming bubbles can be created with correct avatars from the first frame,
    // and historical messages can resolve character_avatar_url instantly.
    try {
      const metaMap = new Map<string, CharMeta>();

      // Primary character
      const primaryCharId = conv.character_id;
      if (primaryCharId) {
        try {
          const char = await ipc.getCharacter(primaryCharId);
          const avUrl = await resolveCachedAvatarUrl(char.avatar_path);
          metaMap.set(primaryCharId, {
            id: primaryCharId,
            name: char.name,
            avatarUrl: avUrl,
            accentColor: charAccentColor(char.name),
          });
        } catch { /* character may have been deleted */ }
      }

      // Additional characters
      try {
        const convChars = await ipc.listConversationCharacters(conversationId);
        for (const cc of convChars) {
          if (cc.character_id && !metaMap.has(cc.character_id)) {
            try {
              const char = await ipc.getCharacter(cc.character_id);
              const avUrl = await resolveCachedAvatarUrl(char.avatar_path);
              metaMap.set(cc.character_id, {
                id: cc.character_id,
                name: char.name,
                avatarUrl: avUrl,
                accentColor: charAccentColor(char.name),
              });
            } catch { /* skip */ }
          }
        }
      } catch { /* single-char mode — no additional characters */ }

      conversationCharMeta.set(metaMap);

      // Resolve character_avatar_url on historical messages from cached metadata
      const currentMsgs = get(messages);
      const needsAvatarUpdate = currentMsgs.some(m => m.character_id && !m.character_avatar_url);
      if (needsAvatarUpdate) {
        messages.update(msgs => msgs.map(m => {
          if (m.character_id && !m.character_avatar_url) {
            const meta = metaMap.get(m.character_id);
            if (meta) return { ...m, character_avatar_url: meta.avatarUrl };
          }
          return m;
        }));
        // Also update pathState.fullActivePath
        for (let i = 0; i < pathState.fullActivePath.length; i++) {
          const m = pathState.fullActivePath[i];
          if (m.character_id && !m.character_avatar_url) {
            const meta = metaMap.get(m.character_id);
            if (meta) pathState.fullActivePath[i] = { ...m, character_avatar_url: meta.avatarUrl };
          }
        }
      }
    } catch {
      conversationCharMeta.set(new Map());
    }
  } catch (err) {
    console.error(`Failed to load messages for conversation ${conversationId}:`, err);
    const detail = (err as any)?.message ?? String(err);
    toastError(`Failed to load messages: ${detail}`);
    messages.set([]);
    resetPaginationState();
  }
}

/**
 * Prepends the next batch of older messages from the in-memory active path.
 * Called by the UI when the user scrolls near the top of the messages area.
 *
 * @returns The number of messages that were prepended (used for scroll position preservation).
 */
export async function loadMoreMessages(): Promise<number> {
  if (get(isLoadingMoreMessages)) return 0;
  if (pathState.currentRenderCount >= pathState.fullActivePath.length) {
    hasMoreMessages.set(false);
    return 0;
  }

  isLoadingMoreMessages.set(true);

  // Brief delay so the skeleton loader is perceptible (avoids jarring instant-load)
  await new Promise(r => setTimeout(r, 120));

  const remaining = pathState.fullActivePath.length - pathState.currentRenderCount;
  const batchSize = Math.min(MESSAGE_RENDER_SIZE, remaining);
  const startIdx = pathState.fullActivePath.length - pathState.currentRenderCount - batchSize;
  const batch = pathState.fullActivePath.slice(startIdx, startIdx + batchSize);

  pathState.currentRenderCount += batchSize;
  hasMoreMessages.set(pathState.currentRenderCount < pathState.fullActivePath.length);

  // Prepend older messages to the front of the store
  messages.update(msgs => [...batch, ...msgs]);
  isLoadingMoreMessages.set(false);

  return batchSize;
}

/** Switches to a sibling message at the same branch point.
 *  Loads the full branch from that sibling down to the leaf. */
export async function switchBranch(siblingId: string) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  const convId = get(activeConversationId);
  if (!convId) return;

  try {
    // If a stream is in progress, the stream-guard in sendMessage will discard further
    // events once activeConversationId no longer matches — safe to clear the flag here.
    isStreaming.set(false);

    // Set this sibling as the active message on the backend
    await ipc.setActiveMessage(convId, siblingId);

    // Reload all messages so sibling info is recomputed (store is flushed inside loadMessages)
    await loadMessages(convId);
  } catch (err) {
    console.error('Failed to switch branch:', err);
  }
}

/**
 * Deletes a message with an undo window, via the same undoableDelete
 * pattern already used for memory deletion (see MemoryGraph.svelte /
 * MemoryTimeline.svelte) instead of the previous instant, unrecoverable
 * delete. Unlike conversations, messages have no backend trash/restore
 * mechanism, and reconstructing one after a real delete would also need to
 * repair the parent_id chain of whatever message came after it — so instead
 * of deleting immediately, this only removes the message from the LOCAL
 * view and defers the actual backend delete until the undo toast's window
 * naturally expires (pause-on-hover keeps it safe while the toast is being
 * looked at). Clicking Undo before then just restores it to the store; the
 * backend is never touched.
 */
export function deleteMessageWithUndo(id: string) {
  if (!isTauri) return;

  const msgs = get(messages);
  const idx = msgs.findIndex(m => m.id === id);
  if (idx < 0) return;
  const removed = msgs[idx];
  const apIdx = pathState.fullActivePath.findIndex(m => m.id === id);

  messages.update(list => list.filter(m => m.id !== id));
  if (apIdx >= 0) {
    pathState.fullActivePath = pathState.fullActivePath.filter(m => m.id !== id);
    pathState.currentRenderCount -= 1;
  }

  undoableDelete(
    'Message deleted',
    async () => {
      try {
        const ipc = await import('$lib/services/ipc');
        await ipc.deleteMessage(id);
      } catch {
        toastError('Failed to delete message');
      }
    },
    () => {
      messages.update(list => {
        const copy = [...list];
        copy.splice(Math.min(idx, copy.length), 0, removed);
        return copy;
      });
      if (apIdx >= 0) {
        const copy = [...pathState.fullActivePath];
        copy.splice(Math.min(apIdx, copy.length), 0, removed);
        pathState.fullActivePath = copy;
        pathState.currentRenderCount += 1;
      }
    }
  );
}
