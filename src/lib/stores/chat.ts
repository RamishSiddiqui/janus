// ============================================================
//   Mythic — Chat State Store
//   Bridges frontend state with Tauri backend via IPC
// ============================================================

import { writable, derived, get } from 'svelte/store';
import type { Message, ConversationPreview } from '$lib/types';
import { browser } from '$app/environment';
import { error as toastError } from '$lib/stores/toast';
import { settings } from '$lib/stores/settings';
import type { CharacterState } from '$lib/services/ipc';

// Detect if we're running inside Tauri (desktop app) or browser (dev mode)
const isTauri = browser && '__TAURI_INTERNALS__' in window;

// In-memory cache for avatar blob URLs to avoid re-reading filesystem on every load
const avatarCache = new Map<string, string>();

// Active conversation ID
export const activeConversationId = writable<string>('');

// Conversation list
export const conversations = writable<ConversationPreview[]>([]);

// Messages for the active conversation
export const messages = writable<Message[]>([]);

// Whether we're currently streaming a response
export const isStreaming = writable<boolean>(false);

// Tracks the last failed stream (set on error, cleared on successful send/regenerate)
export const lastStreamError = writable<{ conversationId: string; lastUserContent: string } | null>(null);

// Whether conversations are being loaded from backend
export const isLoadingConversations = writable<boolean>(false);

// Active conversation derived from ID
export const activeConversation = derived(
  [conversations, activeConversationId],
  ([$conversations, $id]) => $conversations.find(c => c.id === $id)
);

// Active character ID derived from active conversation
export const activeCharacterId = derived(
  activeConversation,
  ($conv) => $conv?.characterId ?? null
);

// Live emotional state for the active character+conversation.
// Updated reactively after each stream completes — subscribed to by EmotionHUD.
export const characterEmotionState = writable<CharacterState | null>(null);

// --- Actions ---

// Pagination state
const PAGE_SIZE = 30;
export const conversationPage = writable<number>(0);
export const totalConversations = writable<number>(0);
export const hasMoreConversations = writable<boolean>(false);

/** Loads the first page of conversations from the backend. */
export async function loadConversations() {
  if (!isTauri) {
    // DEV MODE ONLY — Mock data for browser preview (never runs in production Tauri builds)
    if (import.meta.env.DEV) {
      conversations.set([
        { id: '1', characterId: 'ch-aria', characterName: 'Aria Silverleaf', avatarColor: 'linear-gradient(135deg, #8B5CF6, #BF40FF)', avatarUrl: null, preview: 'Are you a first-year too? This place is a labyrinth...', time: '2m' },
        { id: '6', characterId: 'ch-aria', characterName: 'Aria Silverleaf', avatarColor: 'linear-gradient(135deg, #8B5CF6, #BF40FF)', avatarUrl: null, preview: 'The Enchanted Library — Chapter 2', time: '1h',
          additionalCharacters: [{ id: 'ch-kai', name: 'Kai', description: 'Enigmatic shadow mage with ties to the underground arcane network', avatarUrl: null, avatarColor: 'linear-gradient(135deg, #6366F1, #8B5CF6)' }] },
        { id: '2', characterId: 'ch-rin', characterName: 'Rin', avatarColor: 'linear-gradient(135deg, #EC4899, #BF40FF)', avatarUrl: null, preview: 'What\'s the job, and how illegal is it?', time: '1h' },
        { id: '3', characterId: 'ch-saffron', characterName: 'Saffron Emberheart', avatarColor: 'linear-gradient(135deg, #F43F5E, #F59E0B)', avatarUrl: null, preview: 'The answer is on page 347 of Aldric\'s Third...', time: '3d' },
        { id: '4', characterId: 'ch-kai', characterName: 'Kai', avatarColor: 'linear-gradient(135deg, #6366F1, #8B5CF6)', avatarUrl: null, preview: 'Sit. Put your phone on the table — I need to check...', time: '5d' },
        { id: '5', characterId: 'ch-ryker', characterName: 'Ryker', avatarColor: 'linear-gradient(135deg, #EF4444, #F59E0B)', avatarUrl: null, preview: 'Nobody comes to Level 12 looking that clean...', time: '1w' },
      ]);
      activeConversationId.set('1');
    }
    return;
  }

  isLoadingConversations.set(true);
  conversationPage.set(0);
  const ipc = await import('$lib/services/ipc');
  try {
    const [convos, count] = await Promise.all([
      ipc.listConversations(PAGE_SIZE, 0),
      ipc.countConversations(),
    ]);

    totalConversations.set(count);
    hasMoreConversations.set(convos.length < count);

    const previews = await resolveConversationPreviews(convos);
    conversations.set(previews);
  } catch (err) {
    console.error('Failed to load conversations:', err);
    toastError('Failed to load conversations. Check your connection.');
  }
  isLoadingConversations.set(false);
}

/** Loads the next page of conversations and appends to the existing list. */
export async function loadMoreConversations() {
  if (!isTauri) return;

  const currentPage = get(conversationPage) + 1;
  conversationPage.set(currentPage);

  const ipc = await import('$lib/services/ipc');
  try {
    const offset = currentPage * PAGE_SIZE;
    const convos = await ipc.listConversations(PAGE_SIZE, offset);

    if (convos.length === 0) {
      hasMoreConversations.set(false);
      return;
    }

    const newPreviews = await resolveConversationPreviews(convos);
    conversations.update(existing => [...existing, ...newPreviews]);
    hasMoreConversations.set(offset + convos.length < get(totalConversations));
  } catch (err) {
    console.error('Failed to load more conversations:', err);
    toastError('Failed to load more conversations.');
  }
}

/** Resolves character names and avatars for a batch of raw conversations. */
async function resolveConversationPreviews(convos: Awaited<ReturnType<typeof import('$lib/services/ipc').listConversations>>): Promise<ConversationPreview[]> {
  const ipc = await import('$lib/services/ipc');
  return Promise.all(
    convos.map(async (conv) => {
      let characterName = conv.title || 'Unknown';
      let avatarUrl: string | null = null;
      if (conv.character_id) {
        try {
          const char = await ipc.getCharacter(conv.character_id);
          characterName = char.name;
          // Resolve avatar image (use cache if available)
          if (char.avatar_path) {
            if (avatarCache.has(char.avatar_path)) {
              avatarUrl = avatarCache.get(char.avatar_path)!;
            } else {
              try {
                const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
                const bytes = await readFile(char.avatar_path, { baseDir: BaseDirectory.AppData });
                const ext = char.avatar_path.split('.').pop()?.toLowerCase() || 'jpeg';
                const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg';
                const blob = new Blob([bytes], { type: mime });
                avatarUrl = URL.createObjectURL(blob);
                avatarCache.set(char.avatar_path, avatarUrl);
              } catch { /* avatar file missing */ }
            }
          }
        } catch {
          // Character may have been deleted
        }
      }

      let additionalCharacters: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[] | undefined = undefined;

      if (conv.shared_character_ids) {
        additionalCharacters = [];
        const sharedIds = conv.shared_character_ids.split(',');
        for (const sharedId of sharedIds) {
          try {
            const char = await ipc.getCharacter(sharedId);
            let sharedAvatarUrl: string | null = null;
            if (char.avatar_path) {
              if (avatarCache.has(char.avatar_path)) {
                sharedAvatarUrl = avatarCache.get(char.avatar_path)!;
              } else {
                try {
                  const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
                  const bytes = await readFile(char.avatar_path, { baseDir: BaseDirectory.AppData });
                  const ext = char.avatar_path.split('.').pop()?.toLowerCase() || 'jpeg';
                  const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg';
                  const blob = new Blob([bytes], { type: mime });
                  sharedAvatarUrl = URL.createObjectURL(blob);
                  avatarCache.set(char.avatar_path, sharedAvatarUrl);
                } catch { /* missing */ }
              }
            }
            let charDesc = '';
            try { charDesc = JSON.parse(char.data)?.description || ''; } catch {}
            additionalCharacters.push({
              id: char.id,
              name: char.name,
              description: charDesc,
              avatarUrl: sharedAvatarUrl,
              avatarColor: getAvatarColor(char.name),
            });
          } catch { /* missing character */ }
        }
      }

      // Calculate relative time
      const time = getRelativeTime(conv.updated_at);

      return {
        id: conv.id,
        characterId: conv.character_id,
        characterName,
        avatarColor: getAvatarColor(characterName),
        avatarUrl,
        preview: conv.title,
        time,
        additionalCharacters,
        parentConversationId: conv.parent_conversation_id ?? null,
        branchPointMessageId: conv.branch_point_message_id ?? null,
      };
    })
  );
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
    } else {
      // Fallback: show only the first child at each branch level (depth-first active path)
      activePath = [];
      let currentParentKey = '__root__';
      while (byParent.has(currentParentKey)) {
        const children = byParent.get(currentParentKey)!;
        const next = children[0];
        activePath.push(next);
        currentParentKey = next.id;
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
        const bpMsg = activePath[bpIdx];
        // Ordered: [this conversation (parent, index 0), ...child branches sorted]
        const sortedChildIds = [...childIds].sort();
        const uniqueIds = [conversationId, ...sortedChildIds];
        convSiblingOverrides.set(bpMsg.id, { ids: uniqueIds, index: 0 });
      }
    }

    // Build final annotated message list
    messages.set(activePath.map(m => {
      const key = m.parent_id ?? '__root__';
      const siblings = byParent.get(key) ?? [];
      const siblingIndex = siblings.findIndex(s => s.id === m.id);
      const convSibling = convSiblingOverrides.get(m.id);

      return {
        id: m.id,
        role: m.role as 'user' | 'assistant',
        content: m.content,
        parent_id: m.parent_id,
        // In-conversation siblings (old message-tree branching)
        siblingIds: siblings.length > 1 ? siblings.map(s => s.id) : undefined,
        siblingIndex: siblings.length > 1 ? siblingIndex : undefined,
        alternates: convSibling
          ? convSibling.ids.length
          : (siblings.length > 1 ? siblings.length : undefined),
        currentAlternate: convSibling
          ? convSibling.index + 1
          : (siblings.length > 1 ? siblingIndex + 1 : undefined),
        // Cross-conversation branch siblings
        siblingConversationIds: convSibling?.ids,
        siblingConversationIndex: convSibling?.index,
      };
    }));

    // Pre-load emotional state for immediate HUD display on conversation open
    try {
      const charId = conv.character_id;
      if (charId) {
        const existingState = await ipc.getCharacterState(charId, conversationId);
        characterEmotionState.set(existingState);
      } else {
        characterEmotionState.set(null);
      }
    } catch {
      characterEmotionState.set(null);
    }
  } catch (err) {
    console.error(`Failed to load messages for conversation ${conversationId}:`, err);
    const detail = (err as any)?.message ?? String(err);
    toastError(`Failed to load messages: ${detail}`);
    messages.set([]);
  }
}


/** Sends a user message and initiates streaming response from the backend. */
export async function sendMessage(conversationId: string, content: string, model?: string) {
  if (!isTauri) {
    // Dev mode — just add user message locally
    messages.update(msgs => [...msgs, {
      id: crypto.randomUUID(),
      role: 'user' as const,
      content,
    }]);
    return;
  }

  const ipc = await import('$lib/services/ipc');
  const tempUserId = crypto.randomUUID();
  try {
    // Add user message to local state immediately for responsiveness
    messages.update(msgs => [...msgs, {
      id: tempUserId,
      role: 'user' as const,
      content,
    }]);

    isStreaming.set(true);
    lastStreamError.set(null);

    // Set up stream listener BEFORE sending
    const unlisten = await ipc.onChatStream((event) => {
      // Guard: if the user switched to a different conversation, discard stale events
      if (get(activeConversationId) !== conversationId) {
        if (event.event_type === 'done' || event.event_type === 'error') unlisten();
        return;
      }

      if (event.event_type === 'delta') {
        messages.update(msgs => {
          const last = msgs[msgs.length - 1];
          if (last && last.id === event.message_id) {
            return [...msgs.slice(0, -1), { ...last, content: last.content + event.content, isStreaming: true }];
          } else if (!msgs.find(m => m.id === event.message_id)) {
            // First delta — create the assistant message
            return [...msgs, { id: event.message_id, role: 'assistant', content: event.content, isStreaming: true }];
          }
          return msgs;
        });
      } else if (event.event_type === 'done') {
        messages.update(msgs => {
          const exists = msgs.some(m => m.id === event.message_id);
          if (exists) {
            // Streaming path — message was created by delta events, just finalize
            return msgs.map(m => m.id === event.message_id
              ? { ...m, content: event.content, isStreaming: false }
              : m
            );
          } else {
            // Non-streaming path — message doesn't exist locally yet, create it
            return [...msgs, { id: event.message_id, role: 'assistant' as const, content: event.content, isStreaming: false }];
          }
        });
        isStreaming.set(false);
        unlisten();

        // --- Auto-save memories pipeline ---
        const s = get(settings);
        if (s.autoSaveMemories && event.content) {
          (async () => {
            try {
              const { shouldExtract, extractAndSaveMemories } = await import('$lib/services/memory-extractor');
              if (!shouldExtract()) return;

              // Check per-conversation memory scope
              const ipcMod = await import('$lib/services/ipc');
              const conv = await ipcMod.getConversation(conversationId);
              if (conv.memory_scope === 'none') return; // Auto-save disabled for this chat

              const saved = await extractAndSaveMemories(
                conversationId,
                conv.memory_scope === 'character' ? (conv.character_id ?? undefined) : undefined,
                content,           // user's message
                event.content,     // assistant's response
              );
              if (saved > 0) {
                console.debug(`[Mythic] Auto-saved ${saved} memor${saved === 1 ? 'y' : 'ies'} (scope: ${conv.memory_scope})`);
              }
            } catch (err) {
              console.warn('[Mythic] Auto-memory extraction failed:', err);
            }
          })();
        }

        // --- Emotional state update pipeline ---
        // Runs fire-and-forget after every response regardless of memory scope setting.
        // After the LLM infers the new state and persists it, we push it into
        // characterEmotionState so the EmotionHUD reactively updates without a page reload.
        if (event.content) {
          (async () => {
            try {
              const charId = get(activeCharacterId);
              if (!charId) return;
              const { updateEmotionalState } = await import('$lib/services/emotion-updater');
              await updateEmotionalState(charId, conversationId, content, event.content);
              // Push the freshly-saved state into the reactive store so all HUDs update
              const ipcMod = await import('$lib/services/ipc');
              const newState = await ipcMod.getCharacterState(charId, conversationId);
              characterEmotionState.set(newState);
            } catch (err) {
              console.warn('[Mythic] Emotion update failed:', err);
            }
          })();
        }

      } else if (event.event_type === 'error') {
        console.error('Stream error:', event.content);
        toastError(`AI response failed: ${event.content}`);
        lastStreamError.set({ conversationId, lastUserContent: content });
        isStreaming.set(false);
        unlisten();
      }
    });

    // Send the message — backend will stream/generate response via events
    const currentSettings = get(settings);
    const result = await ipc.sendMessage(
      conversationId, content, model,
      currentSettings.systemPrompt || undefined,
      currentSettings.streamingEnabled,
    );

    // Replace temp user message ID with real one from backend
    messages.update(msgs =>
      msgs.map(m => m.id === tempUserId ? { ...m, id: result.user_message_id } : m)
    );
  } catch (err) {
    console.error('Failed to send message:', err);
    const msg = (err as any)?.message ?? 'Failed to send message. Is a provider configured?';
    toastError(msg);
    // Remove the optimistic user message
    messages.update(msgs => msgs.filter(m => m.id !== tempUserId));
    isStreaming.set(false);
  }
}

/** Retries the last failed streaming response by re-sending the user's message. */
export async function retryLastMessage() {
  const err = get(lastStreamError);
  if (!err) return;
  lastStreamError.set(null);
  // Remove the last assistant message (the failed/empty one)
  messages.update(msgs => {
    const last = msgs[msgs.length - 1];
    if (last && last.role === 'assistant') {
      return msgs.slice(0, -1);
    }
    return msgs;
  });
  await sendMessage(err.conversationId, err.lastUserContent);
}

/** Regenerates the last assistant response, streaming the new content. */
export async function regenerateMessage(conversationId: string, messageId: string, model?: string) {
  if (!isTauri || get(isStreaming)) return;

  const ipc = await import('$lib/services/ipc');
  isStreaming.set(true);

  try {
    // Set up stream listener BEFORE triggering regeneration
    const unlisten = await ipc.onChatStream((event) => {
      if (event.event_type === 'delta') {
        messages.update(msgs => {
          const last = msgs[msgs.length - 1];
          if (last && last.id === event.message_id) {
            return [...msgs.slice(0, -1), { ...last, content: last.content + event.content, isStreaming: true }];
          } else if (!msgs.find(m => m.id === event.message_id)) {
            // First delta — create the new assistant message (replaces old one visually)
            return [...msgs, { id: event.message_id, role: 'assistant', content: event.content, isStreaming: true }];
          }
          return msgs;
        });
      } else if (event.event_type === 'done') {
        messages.update(msgs =>
          msgs.map(m => m.id === event.message_id
            ? { ...m, content: event.content, isStreaming: false }
            : m
          )
        );
        isStreaming.set(false);
        unlisten();
        // Reload messages to get sibling info
        loadMessages(conversationId);
      } else if (event.event_type === 'error') {
        console.error('Regeneration stream error:', event.content);
        toastError(`Regeneration failed: ${event.content}`);
        isStreaming.set(false);
        unlisten();
      }
    });

    const currentSettings = get(settings);
    await ipc.regenerateMessage(
      conversationId, messageId, model,
      currentSettings.systemPrompt || undefined,
      currentSettings.streamingEnabled,
    );
  } catch (err) {
    console.error('Failed to regenerate:', err);
    const msg = (err as any)?.message ?? 'Failed to regenerate response';
    toastError(msg);
    isStreaming.set(false);
  }
}

/** Creates a new conversation for a character and navigates to it. */
export async function createConversation(characterId: string, title?: string) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  try {
    const conv = await ipc.createConversation(characterId, title);
    activeConversationId.set(conv.id);
    messages.set([]);

    // Auto-send character greeting (first_mes) if available
    if (characterId) {
      try {
        const char = await ipc.getCharacter(characterId);
        const data = JSON.parse(char.data);
        const greeting = data.first_mes?.trim();
        if (greeting) {
          // Create the greeting as an assistant message
          const greetingMsg = await ipc.createMessage(conv.id, 'assistant', greeting);
          await ipc.setActiveMessage(conv.id, greetingMsg.id);
          messages.set([{
            id: greetingMsg.id,
            role: 'assistant',
            content: greeting,
          }]);
        }
      } catch (err) {
        console.warn('Could not send greeting:', err);
      }
    }

    await loadConversations();
  } catch (err) {
    console.error('Failed to create conversation:', err);
    toastError('Failed to create conversation');
  }
}

/** Deletes a conversation and refreshes the list. */
export async function deleteConversation(id: string) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  try {
    await ipc.deleteConversation(id);

    // If we deleted the active conversation, clear it
    if (get(activeConversationId) === id) {
      activeConversationId.set('');
      messages.set([]);
    }

    await loadConversations();
  } catch (err) {
    console.error('Failed to delete conversation:', err);
  }
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
 * Navigates to a different conversation (used by the cross-conversation sibling navigator).
 * Equivalent to clicking a different conversation in the sidebar, but triggered from within
 * the message navigator pill.
 */
export async function switchToConversation(targetConversationId: string) {
  isStreaming.set(false);
  activeConversationId.set(targetConversationId);
  await loadMessages(targetConversationId);
}

/**
 * Branches the conversation at `branchPointId` into a new independent conversation.
 *
 * The new conversation:
 *  - Is a copy of the parent up to (and including) `branchPointId`
 *  - Keeps the same character, title, and memory scope as the parent
 *  - Has all memories from the parent bulk-copied with 'copy' links
 *    (renders as dashed COPY arrows in MemoryGraph/MemoryTimeline)
 *  - Becomes the active conversation immediately after creation
 *
 * After branching, `content` is sent as the first new message in the new conversation.
 */
export async function branchConversation(
  parentConversationId: string,
  branchPointId: string,
  content: string,
  model?: string,
) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  try {
    // 1. Create the new branched conversation (backend copies messages + memories)
    const newConv = await ipc.branchConversation(
      parentConversationId,
      branchPointId,
      // Title comes from parent — backend copies it as-is
    );

    // 2. Refresh sidebar list FIRST so $conversations is up to date before loadMessages
    //    reads it for cross-conversation sibling detection.
    await loadConversations();

    // 3. Switch to the new conversation and load its messages
    //    (loadMessages now reads $conversations to annotate the divergence message)
    activeConversationId.set(newConv.id);
    await loadMessages(newConv.id);

    // 4. Send the new message into the new conversation — this is the actual fork point
    await sendMessage(newConv.id, content, model);

  } catch (err) {
    console.error('[Branch] Failed to branch conversation:', err);
    throw err;
  }
}

// --- Helpers ---

function getRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = Date.now();
    const diff = now - date.getTime();

    const minutes = Math.floor(diff / 60000);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}d`;
    const weeks = Math.floor(days / 7);
    return `${weeks}w`;
  } catch {
    return '';
  }
}

function getAvatarColor(name: string): string {
  const colors = [
    'linear-gradient(135deg, #8B5CF6, #BF40FF)',
    'linear-gradient(135deg, #00F2FF, #10B981)',
    'linear-gradient(135deg, #F59E0B, #F43F5E)',
    'linear-gradient(135deg, #BF40FF, #8B5CF6)',
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}
