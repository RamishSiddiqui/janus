// ============================================================
//   Mythic — Chat State Store
//   Bridges frontend state with Tauri backend via IPC
// ============================================================

import { writable, derived, get } from 'svelte/store';
import type { Message, ConversationPreview } from '$lib/types';
import { browser } from '$app/environment';
import { error as toastError } from '$lib/stores/toast';
import { settings } from '$lib/stores/settings';

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
        { id: '1', characterId: null, characterName: 'Aria Silverleaf', avatarColor: 'linear-gradient(135deg, #8B5CF6, #BF40FF)', avatarUrl: null, preview: 'Are you a first-year too? This place is a labyrinth...', time: '2m' },
        { id: '2', characterId: null, characterName: 'Rin', avatarColor: 'linear-gradient(135deg, #EC4899, #BF40FF)', avatarUrl: null, preview: 'What\'s the job, and how illegal is it?', time: '1h' },
        { id: '3', characterId: null, characterName: 'Saffron Emberheart', avatarColor: 'linear-gradient(135deg, #F43F5E, #F59E0B)', avatarUrl: null, preview: 'The answer is on page 347 of Aldric\'s Third...', time: '3d' },
        { id: '4', characterId: null, characterName: 'Kai', avatarColor: 'linear-gradient(135deg, #6366F1, #8B5CF6)', avatarUrl: null, preview: 'Sit. Put your phone on the table — I need to check...', time: '5d' },
        { id: '5', characterId: null, characterName: 'Ryker', avatarColor: 'linear-gradient(135deg, #EF4444, #F59E0B)', avatarUrl: null, preview: 'Nobody comes to Level 12 looking that clean...', time: '1w' },
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
      };
    })
  );
}

/** Loads messages for the active conversation. */
export async function loadMessages(conversationId: string) {
  if (!isTauri) {
    // Dev mode — keep existing mock messages
    return;
  }

  const ipc = await import('$lib/services/ipc');
  try {
    const msgs = await ipc.getConversationMessages(conversationId);

    // Group messages by parent_id to compute sibling info
    const byParent = new Map<string, typeof msgs>();
    for (const m of msgs) {
      const key = m.parent_id ?? '__root__';
      if (!byParent.has(key)) byParent.set(key, []);
      byParent.get(key)!.push(m);
    }

    messages.set(msgs.map(m => {
      const key = m.parent_id ?? '__root__';
      const siblings = byParent.get(key) ?? [];
      const siblingIndex = siblings.findIndex(s => s.id === m.id);

      return {
        id: m.id,
        role: m.role as 'user' | 'assistant',
        content: m.content,
        parent_id: m.parent_id,
        siblingIds: siblings.length > 1 ? siblings.map(s => s.id) : undefined,
        siblingIndex: siblings.length > 1 ? siblingIndex : undefined,
        alternates: siblings.length > 1 ? siblings.length : undefined,
        currentAlternate: siblings.length > 1 ? siblingIndex + 1 : undefined,
      };
    }));
  } catch (err) {
    console.error('Failed to load messages:', err);
    toastError('Failed to load messages for this conversation.');
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
  try {
    // Add user message to local state immediately for responsiveness
    const tempUserId = crypto.randomUUID();
    messages.update(msgs => [...msgs, {
      id: tempUserId,
      role: 'user' as const,
      content,
    }]);

    isStreaming.set(true);
    lastStreamError.set(null);

    // Set up stream listener BEFORE sending
    const unlisten = await ipc.onChatStream((event) => {
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
          return msgs.map(m => m.id === event.message_id
            ? { ...m, content: event.content, isStreaming: false }
            : m
          );
        });
        isStreaming.set(false);
        unlisten();
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
    // Set this sibling as the active message on the backend
    await ipc.setActiveMessage(convId, siblingId);

    // Reload all messages so sibling info is recomputed
    await loadMessages(convId);
  } catch (err) {
    console.error('Failed to switch branch:', err);
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
