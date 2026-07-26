// ============================================================
//   Mythic — Chat State Store
//   Bridges frontend state with Tauri backend via IPC
// ============================================================

import { writable, derived, get } from 'svelte/store';
import type { Message, ConversationPreview } from '$lib/types';
import { browser } from '$app/environment';
import { error as toastError } from '$lib/stores/toast';
import { settings } from '$lib/stores/settings';
import { parseCharacterData } from '$lib/utils/character';
import type { CharacterState } from '$lib/services/ipc';
import { PresentationBuffer, charAccentColor, type CharMeta, type PresentationCallbacks } from '$lib/services/presentation-buffer';

// Detect if we're running inside Tauri (desktop app) or browser (dev mode)
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
        if (!fullActivePath.find(m => m.id === msg.id)) {
          fullActivePath.push(msg);
          currentRenderCount++;
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

// In-memory cache for avatar blob URLs to avoid re-reading filesystem on every load
const avatarCache = new Map<string, string>();

// Active conversation ID
export const activeConversationId = writable<string>('');

// Conversation list
export const conversations = writable<ConversationPreview[]>([]);

// Messages for the active conversation (rendered window — NOT the full history)
export const messages = writable<Message[]>([]);

// Whether we're currently streaming a response
export const isStreaming = writable<boolean>(false);

// Tracks the last failed stream (set on error, cleared on successful send/regenerate)
// userMessageId is the real DB id — used by retryLastMessage to reuse the existing record
export const lastStreamError = writable<{ conversationId: string; lastUserContent: string; userMessageId?: string } | null>(null);

// Whether conversations are being loaded from backend
export const isLoadingConversations = writable<boolean>(false);

// ── Message Pagination ──
// The full active branch path lives in memory (SQLite is local, fast to fetch).
// The `messages` store only contains the rendered window (last N messages).
// Scrolling up prepends older messages from the backing array.
const MESSAGE_RENDER_SIZE = 30;
let fullActivePath: Message[] = [];   // complete annotated branch (root → tip)
let currentRenderCount = 0;           // how many messages are currently in the store

/** Whether there are older messages available to load on scroll-up. */
export const hasMoreMessages = writable<boolean>(false);

/** Whether a batch of older messages is currently being prepended. */
export const isLoadingMoreMessages = writable<boolean>(false);

/** Resets all pagination state — call whenever the conversation changes or is cleared. */
function resetPaginationState() {
  fullActivePath = [];
  currentRenderCount = 0;
  hasMoreMessages.set(false);
  isLoadingMoreMessages.set(false);
}

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

// Live emotional states for ALL characters in the active conversation.
// Keyed by character_id → CharacterState. Updated reactively after each
// stream completes — subscribed to by EmotionHUD per-message.
export const characterEmotionStates = writable<Map<string, CharacterState>>(new Map());

// Convenience: get a single character's emotion state from the map.
// Kept for backward compatibility — reads the primary character's state.
export const characterEmotionState = derived(
  [characterEmotionStates, activeCharacterId],
  ([$map, $charId]) => $charId ? ($map.get($charId) ?? null) : null,
);

// Pre-resolved character metadata for the active conversation.
// Populated on conversation open, consumed by PresentationBuffer for instant
// avatar/name attachment to streaming bubbles. Keyed by character_id.
export const conversationCharMeta = writable<Map<string, CharMeta>>(new Map());

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
    console.log('[Mythic] loadConversations: calling listConversations...');
    let convos;
    try {
      convos = await ipc.listConversations(PAGE_SIZE, 0);
      console.log('[Mythic] loadConversations: listConversations OK, got', convos.length, 'conversations');
    } catch (listErr) {
      console.error('[Mythic] loadConversations: listConversations FAILED:', listErr);
      console.error('[Mythic] listErr type:', typeof listErr, 'keys:', listErr ? Object.keys(listErr as object) : 'null');
      throw listErr;
    }

    console.log('[Mythic] loadConversations: calling countConversations...');
    let count;
    try {
      count = await ipc.countConversations();
      console.log('[Mythic] loadConversations: countConversations OK, count=', count);
    } catch (countErr) {
      console.error('[Mythic] loadConversations: countConversations FAILED:', countErr);
      console.error('[Mythic] countErr type:', typeof countErr, 'keys:', countErr ? Object.keys(countErr as object) : 'null');
      throw countErr;
    }

    totalConversations.set(count);
    hasMoreConversations.set(convos.length < count);

    console.log('[Mythic] loadConversations: resolving previews...');
    const previews = await resolveConversationPreviews(convos);
    console.log('[Mythic] loadConversations: previews resolved, count=', previews.length);
    conversations.set(previews);
  } catch (err) {
    console.error('Failed to load conversations:', err);
    console.error('Error details:', JSON.stringify(err, null, 2));
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
            const charData = parseCharacterData(char.data);
            const charDesc = (charData.description as string) || '';
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

    fullActivePath = activePath.map((m, index) => {
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
      };
    });

    // Render only the last N messages (paginated rendering)
    currentRenderCount = Math.min(fullActivePath.length, MESSAGE_RENDER_SIZE);
    const initialSlice = fullActivePath.slice(fullActivePath.length - currentRenderCount);
    messages.set(initialSlice);
    hasMoreMessages.set(currentRenderCount < fullActivePath.length);

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
      const resolveAvatar = async (avatarPath: string | null): Promise<string | null> => {
        if (!avatarPath) return null;
        if (avatarCache.has(avatarPath)) return avatarCache.get(avatarPath)!;
        try {
          const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
          const bytes = await readFile(avatarPath, { baseDir: BaseDirectory.AppData });
          const ext = avatarPath.split('.').pop()?.toLowerCase() || 'jpeg';
          const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg';
          const blob = new Blob([bytes], { type: mime });
          const url = URL.createObjectURL(blob);
          avatarCache.set(avatarPath, url);
          return url;
        } catch { return null; }
      };

      // Primary character
      const primaryCharId = conv.character_id;
      if (primaryCharId) {
        try {
          const char = await ipc.getCharacter(primaryCharId);
          const avUrl = await resolveAvatar(char.avatar_path);
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
              const avUrl = await resolveAvatar(char.avatar_path);
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
        // Also update fullActivePath
        for (let i = 0; i < fullActivePath.length; i++) {
          const m = fullActivePath[i];
          if (m.character_id && !m.character_avatar_url) {
            const meta = metaMap.get(m.character_id);
            if (meta) fullActivePath[i] = { ...m, character_avatar_url: meta.avatarUrl };
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
  if (currentRenderCount >= fullActivePath.length) {
    hasMoreMessages.set(false);
    return 0;
  }

  isLoadingMoreMessages.set(true);

  // Brief delay so the skeleton loader is perceptible (avoids jarring instant-load)
  await new Promise(r => setTimeout(r, 120));

  const remaining = fullActivePath.length - currentRenderCount;
  const batchSize = Math.min(MESSAGE_RENDER_SIZE, remaining);
  const startIdx = fullActivePath.length - currentRenderCount - batchSize;
  const batch = fullActivePath.slice(startIdx, startIdx + batchSize);

  currentRenderCount += batchSize;
  hasMoreMessages.set(currentRenderCount < fullActivePath.length);

  // Prepend older messages to the front of the store
  messages.update(msgs => [...batch, ...msgs]);
  isLoadingMoreMessages.set(false);

  return batchSize;
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
    const userMsg: Message = {
      id: tempUserId,
      role: 'user' as const,
      content,
    };
    messages.update(msgs => [...msgs, userMsg]);
    fullActivePath.push(userMsg);
    currentRenderCount++;

    isStreaming.set(true);
    lastStreamError.set(null);

    // Create a fresh PresentationBuffer for this stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    // Set up stream listener BEFORE sending
    const unlisten = await ipc.onChatStream((event) => {
      // Guard: if the user switched to a different conversation, discard stale events
      if (get(activeConversationId) !== conversationId) {
        if (event.event_type === 'done' || event.event_type === 'error') unlisten();
        return;
      }

      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
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
        isStreaming.set(false);
        unlisten();

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
                console.debug(`[Mythic] Auto-saved ${saved} memor${saved === 1 ? 'y' : 'ies'} (scope: ${conv.memory_scope})`);
              }
            } catch (err) {
              console.warn('[Mythic] Auto-memory extraction failed:', err);
            }
          })();
        }

        // --- Emotional state update pipeline ---
        // Runs fire-and-forget after every response regardless of memory scope setting.
        // In multi-char mode, updates emotions for ALL conversation characters in parallel.
        // After the LLM infers each character's new state, we push them into
        // characterEmotionStates so per-message EmotionHUDs reactively update.
        if (event.content) {
          (async () => {
            try {
              const { updateEmotionalState } = await import('$lib/services/emotion-updater');
              const ipcMod = await import('$lib/services/ipc');

              // Collect all character IDs and names to update emotions for
              const charMap = new Map<string, string>(); // charId → charName
              const primaryCharId = get(activeCharacterId);

              // Add multi-char conversation characters (includes primary)
              try {
                const convChars = await ipcMod.listConversationCharacters(conversationId);
                for (const cc of convChars) {
                  if (cc.character_id) {
                    charMap.set(cc.character_id, cc.character_name || 'Character');
                  }
                }
              } catch { /* single-char mode */ }

              // Ensure primary character is included even if listConversationCharacters is empty
              if (primaryCharId && !charMap.has(primaryCharId)) {
                charMap.set(primaryCharId, 'Character');
              }

              if (charMap.size === 0) return;

              // Run emotion updates in parallel for all characters
              await Promise.allSettled(Array.from(charMap.entries()).map(async ([charId, charName]) => {
                await updateEmotionalState(charId, conversationId, content, event.content, charName);
                const newState = await ipcMod.getCharacterState(charId, conversationId);
                if (newState) {
                  characterEmotionStates.update(map => {
                    const updated = new Map(map);
                    updated.set(charId, newState);
                    return updated;
                  });
                }
              }));
            } catch (err) {
              console.warn('[Mythic] Emotion update failed:', err);
            }
          })();
        }

      } else if (event.event_type === 'error') {
        buffer.reset();
        console.error('Stream error:', event.content);
        toastError(`AI response failed: ${event.content}`);
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
        const apIdx = fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) fullActivePath[apIdx] = assistantMsg;
        else { fullActivePath.push(assistantMsg); currentRenderCount++; }
        // The real user_message_id was set after sendMessage returned (line ~706)
        // Grab it from the current messages array to pass to retry
        const realUserMsgId = get(messages).filter(m => m.role === 'user').pop()?.id;
        lastStreamError.set({ conversationId, lastUserContent: content, userMessageId: realUserMsgId });
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
      currentSettings.postHistoryInstructions || undefined,
    );

    // Replace temp user message ID with real one from backend
    messages.update(msgs =>
      msgs.map(m => m.id === tempUserId ? { ...m, id: result.user_message_id } : m)
    );
    // Sync fullActivePath
    const fpIdx = fullActivePath.findIndex(m => m.id === tempUserId);
    if (fpIdx >= 0) fullActivePath[fpIdx] = { ...fullActivePath[fpIdx], id: result.user_message_id };
  } catch (err) {
    console.error('Failed to send message:', err);
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
    const beforeCount = fullActivePath.length;
    fullActivePath = fullActivePath.filter(m => !(m.isError && m.role === 'assistant'));
    currentRenderCount -= beforeCount - fullActivePath.length;

    // Clear error state on the user message
    messages.update(msgs => msgs.map(m => m.id === err.userMessageId ? { ...m, isError: false } : m));

    const ipc = await import('$lib/services/ipc');
    isStreaming.set(true);

    // Create a fresh PresentationBuffer for this retry stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    // Set up stream listener
    const unlisten = await ipc.onChatStream((event) => {
      if (get(activeConversationId) !== err.conversationId) {
        if (event.event_type === 'done' || event.event_type === 'error') unlisten();
        return;
      }

      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
      } else if (event.event_type === 'done') {
        buffer.finalize();
        // Buffer handles all message creation/content — just clear streaming state
        messages.update(msgs =>
          msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m)
        );
        isStreaming.set(false);
        unlisten();
      } else if (event.event_type === 'error') {
        buffer.reset();
        toastError(`AI response failed: ${event.content}`);
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
        const apIdx = fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) fullActivePath[apIdx] = assistantMsg;
        else { fullActivePath.push(assistantMsg); currentRenderCount++; }

        lastStreamError.set({ conversationId: err.conversationId, lastUserContent: err.lastUserContent, userMessageId: err.userMessageId });
        isStreaming.set(false);
        unlisten();
      }
    });

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
      toastError('Retry failed — please try again');
      isStreaming.set(false);
    }
  } else {
    // sendMessage itself failed before the message was saved — clean up and resend
    messages.update(msgs => msgs.filter(m => !m.isError));
    const beforeCount = fullActivePath.length;
    fullActivePath = fullActivePath.filter(m => !m.isError);
    currentRenderCount -= beforeCount - fullActivePath.length;
    await sendMessage(err.conversationId, err.lastUserContent, model);
  }
}

/** Regenerates the last assistant response, streaming the new content. */
export async function regenerateMessage(conversationId: string, messageId: string, model?: string) {
  if (!isTauri || get(isStreaming)) return;

  const ipc = await import('$lib/services/ipc');
  isStreaming.set(true);

  try {
    // Create a fresh PresentationBuffer for this regeneration stream
    activeBuffer = createStreamBuffer();
    const buffer = activeBuffer;

    // Set up stream listener BEFORE triggering regeneration
    const unlisten = await ipc.onChatStream((event) => {
      if (event.event_type === 'delta') {
        buffer.push(event.message_id, event.content);
      } else if (event.event_type === 'done') {
        buffer.finalize();
        // Buffer handles all message creation/content — just clear streaming state
        messages.update(msgs =>
          msgs.map(m => m.isStreaming ? { ...m, isStreaming: false } : m)
        );
        isStreaming.set(false);
        unlisten();
        // Reload messages to get sibling info
        loadMessages(conversationId);
      } else if (event.event_type === 'error') {
        buffer.reset();
        console.error('Regeneration stream error:', event.content);
        toastError(`Regeneration failed: ${event.content}`);
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
        // Mirror into fullActivePath — unlike sendMessage/retryLastMessage's
        // identical error handlers, this one previously only updated the
        // rendered `messages` window, leaving fullActivePath (the pagination
        // source of truth) unaware the regenerated message ever failed.
        const assistantMsg: Message = { id: event.message_id, role: 'assistant', content: event.content || 'Generation failed', isError: true };
        const apIdx = fullActivePath.findIndex(m => m.id === event.message_id);
        if (apIdx >= 0) fullActivePath[apIdx] = assistantMsg;
        else { fullActivePath.push(assistantMsg); currentRenderCount++; }
        isStreaming.set(false);
        unlisten();
      }
    });

    const currentSettings = get(settings);
    await ipc.regenerateMessage(
      conversationId, messageId, model,
      currentSettings.systemPrompt || undefined,
      currentSettings.streamingEnabled,
      currentSettings.postHistoryInstructions || undefined,
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
    resetPaginationState();

    // Auto-send character greeting (first_mes) if available
    if (characterId) {
      try {
        const char = await ipc.getCharacter(characterId);
        const data = parseCharacterData(char.data);
        const greeting = data.first_mes?.trim();
        if (greeting) {
          // Create the greeting as an assistant message
          const greetingMsg = await ipc.createMessage(conv.id, 'assistant', greeting);
          await ipc.setActiveMessage(conv.id, greetingMsg.id);
          const greetingMessage: Message = {
            id: greetingMsg.id,
            role: 'assistant',
            content: greeting,
          };
          messages.set([greetingMessage]);
          fullActivePath = [greetingMessage];
          currentRenderCount = 1;
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
      resetPaginationState();
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

// ── Multi-Character Response Listener ──
// Listens for parsed multi-character response segments from the backend.
// When a response contains dialogue from multiple characters, the backend
// emits 'multi-char-response' with segment attribution. This function
// annotates messages in the store so the UI can render character badges.

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
    }[];

    if (!segments || segments.length === 0) return;

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
      // Sync fullActivePath
      for (let i = 0; i < fullActivePath.length; i++) {
        const m = fullActivePath[i];
        if (m.id === parentId || m.id === `${parentId}__seg0`) {
          fullActivePath[i] = {
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
      // Sync fullActivePath
      for (let i = 0; i < fullActivePath.length; i++) {
        const m = fullActivePath[i];
        for (let j = 0; j < segments.length; j++) {
          if (m.id === `${parentId}__seg${j}`) {
            const seg = segments[j];
            const meta = seg.character_id ? charMeta.get(seg.character_id) : null;
            fullActivePath[i] = {
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
        const splitMessages: Message[] = segments.map((seg, i) => {
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

      // Sync fullActivePath
      const apIdx = fullActivePath.findIndex(m => m.id === parentId);
      if (apIdx >= 0) {
        const parent = fullActivePath[apIdx];
        const splitAp: Message[] = segments.map((seg, i) => {
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
        fullActivePath.splice(apIdx, 1, ...splitAp);
        // 1 message became splitAp.length messages — keep currentRenderCount
        // in step with fullActivePath's new length, or pagination math on the
        // next loadMoreMessages() call desyncs from what's actually rendered.
        currentRenderCount += splitAp.length - 1;
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

// --- Helpers ---

function getRelativeTime(dateStr: string): string {
  try {
    if (!dateStr) return '';
    const date = new Date(dateStr);
    if (isNaN(date.getTime())) return '';
    const now = Date.now();
    const diff = now - date.getTime();

    const minutes = Math.floor(diff / 60000);
    if (minutes < 1) return 'now';
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
