// ============================================================
//   Janus — Chat State Store
//   Bridges frontend state with Tauri backend via IPC
//
//   This is the core module: shared stores, pagination state, conversation
//   list CRUD, and avatar caching. Message loading/branching, the streaming
//   pipeline, the emotion pipeline, and the multi-character listener each
//   live in their own sibling module and are re-exported below, so existing
//   `import { X } from '$lib/stores/chat'` call sites are unaffected.
// ============================================================

import { writable, derived, get } from 'svelte/store';
import type { Message, ConversationPreview } from '$lib/types';
import { browser } from '$app/environment';
import { error as toastError, addToast } from '$lib/stores/toast';
import { settings } from '$lib/stores/settings';
import { parseCharacterData } from '$lib/utils/character';
import type { CharacterState } from '$lib/services/ipc';
import type { CharMeta } from '$lib/services/presentation-buffer';
// Used by this module's own createConversation/deleteConversationWithUndo/
// switchToConversation/branchConversation — also re-exported below so
// existing `import { loadMessages, sendMessage } from '$lib/stores/chat'`
// call sites elsewhere in the app keep working unchanged.
import { loadMessages } from './chatMessages';
import { sendMessage } from './chatStream';

// Detect if we're running inside Tauri (desktop app) or browser (dev mode)
const isTauri = browser && '__TAURI_INTERNALS__' in window;

// In-memory cache for avatar blob URLs to avoid re-reading filesystem on every load
const avatarCache = new Map<string, string>();

/** Resolves an avatar path to a blob: URL, reading the file only once per
 *  path (cached indefinitely — avatars don't change without a new path). */
export async function resolveCachedAvatarUrl(avatarPath: string | null): Promise<string | null> {
  if (!avatarPath) return null;
  const cached = avatarCache.get(avatarPath);
  if (cached) return cached;
  try {
    const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
    const url = await loadFileAsBlobUrl(avatarPath);
    avatarCache.set(avatarPath, url);
    return url;
  } catch {
    return null;
  }
}

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
export const MESSAGE_RENDER_SIZE = 30;
/** Mutable pagination state shared across chat.ts's split modules
 *  (chatMessages.ts, chatStream.ts, chatMultiChar.ts all read/write this) —
 *  a bare `let` can't be reassigned across module boundaries in JS/TS, so
 *  this is exported as an object whose properties get mutated instead. */
export const pathState: { fullActivePath: Message[]; currentRenderCount: number } = {
  fullActivePath: [],   // complete annotated branch (root → tip)
  currentRenderCount: 0, // how many messages are currently in the store
};

/** Whether there are older messages available to load on scroll-up. */
export const hasMoreMessages = writable<boolean>(false);

/** Whether a batch of older messages is currently being prepended. */
export const isLoadingMoreMessages = writable<boolean>(false);

/** Resets all pagination state — call whenever the conversation changes or is cleared. */
export function resetPaginationState() {
  pathState.fullActivePath = [];
  pathState.currentRenderCount = 0;
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
    console.log('[Janus] loadConversations: calling listConversations...');
    let convos;
    try {
      convos = await ipc.listConversations(PAGE_SIZE, 0);
      console.log('[Janus] loadConversations: listConversations OK, got', convos.length, 'conversations');
    } catch (listErr) {
      console.error('[Janus] loadConversations: listConversations FAILED:', listErr);
      console.error('[Janus] listErr type:', typeof listErr, 'keys:', listErr ? Object.keys(listErr as object) : 'null');
      throw listErr;
    }

    console.log('[Janus] loadConversations: calling countConversations...');
    let count;
    try {
      count = await ipc.countConversations();
      console.log('[Janus] loadConversations: countConversations OK, count=', count);
    } catch (countErr) {
      console.error('[Janus] loadConversations: countConversations FAILED:', countErr);
      console.error('[Janus] countErr type:', typeof countErr, 'keys:', countErr ? Object.keys(countErr as object) : 'null');
      throw countErr;
    }

    totalConversations.set(count);
    hasMoreConversations.set(convos.length < count);

    console.log('[Janus] loadConversations: resolving previews...');
    const previews = await resolveConversationPreviews(convos);
    console.log('[Janus] loadConversations: previews resolved, count=', previews.length);
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
          avatarUrl = await resolveCachedAvatarUrl(char.avatar_path);
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
            const sharedAvatarUrl = await resolveCachedAvatarUrl(char.avatar_path);
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

/** Creates a new conversation for a character and navigates to it. */
export async function createConversation(characterId: string, title?: string, personaId?: string) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  try {
    const conv = await ipc.createConversation(characterId, title, personaId);
    activeConversationId.set(conv.id);
    messages.set([]);
    resetPaginationState();

    // Auto-send character greeting (first_mes) if available
    if (characterId) {
      try {
        const char = await ipc.getCharacter(characterId);
        const data = parseCharacterData(char.data);
        let greeting = data.first_mes?.trim();
        if (greeting) {
          try {
            const personaName = personaId ? (await ipc.getPersona(personaId)).name : null;
            const { substituteUserMacro } = await import('$lib/utils/personaMacros');
            greeting = substituteUserMacro(greeting, personaName);
          } catch (err) {
            console.warn('Could not resolve persona for greeting substitution:', err);
          }
        }
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
          pathState.fullActivePath = [greetingMessage];
          pathState.currentRenderCount = 1;

          // The greeting never goes through send_message's streaming path,
          // so it'd otherwise never get scene-extracted — fire this off
          // without awaiting, it just needs to happen eventually.
          ipc.extractInitialScene(conv.id, greeting).catch(err =>
            console.warn('Could not extract initial scene:', err));
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

/**
 * Moves a conversation to Trash immediately — a real, durable backend
 * soft-delete, not a client-side timer. The Undo toast's action calls
 * `restoreConversation`, which is just as immediate; there's no window
 * during which the delete could be silently lost if the app reloads or
 * crashes (unlike the old deferred-commit design this replaces). Permanent
 * removal only happens from the Trash page.
 */
export async function deleteConversationWithUndo(id: string, label: string) {
  if (!isTauri) return;

  const ipc = await import('$lib/services/ipc');
  try {
    await ipc.trashConversation(id);
  } catch (err) {
    toastError(`Failed to delete "${label}"`);
    return;
  }

  conversations.update(list => list.filter(c => c.id !== id));
  const wasActive = get(activeConversationId) === id;
  if (wasActive) {
    activeConversationId.set('');
    messages.set([]);
    resetPaginationState();
  }

  addToast(`Moved "${label}" to Trash`, 'info', 5500, {
    label: 'Undo',
    onClick: async () => {
      try {
        const ipc = await import('$lib/services/ipc');
        await ipc.restoreConversation(id);
        await loadConversations();
        if (wasActive) {
          activeConversationId.set(id);
          loadMessages(id);
        }
      } catch {
        toastError('Failed to restore conversation');
      }
    },
  });
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

// ── Re-exports from split modules ──
// Keeps every existing `import { X } from '$lib/stores/chat'` call site
// working unchanged after the split.
export { loadMessages, loadMoreMessages, switchBranch, deleteMessageWithUndo } from './chatMessages';
export { cancelGeneration, sendMessage, retryLastMessage, regenerateMessage } from './chatStream';
export { parseEmotionSnapshot, runEmotionUpdatePipeline } from './chatEmotion';
export { initMultiCharListener, cleanupMultiCharListener } from './chatMultiChar';
