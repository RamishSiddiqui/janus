/**
 * Type-safe IPC bridge for communicating with the Tauri/Rust backend.
 *
 * All `invoke()` calls go through this module to centralize error handling
 * and provide typed responses. The frontend stores should use these functions
 * instead of calling `invoke()` directly.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  MythicError,
  AppInfo,
  Character_Serialize as Character,
  Conversation_Serialize,
  Message_Serialize as Message,
  ProviderConfig_Serialize as ProviderConfig,
  CharacterState_Serialize as CharacterState,
  ModelEntry,
  SendMessageResult,
  ContextStats,
  Scene_Serialize,
  SceneState_Serialize as SceneState,
  ConversationCharacter_Serialize as ConversationCharacter,
  LorebookEntry_Serialize as LorebookEntry,
  Memory_Serialize as Memory,
  MemoryLink_Serialize,
  MemoryGraphConversation,
  SearchResult_Serialize as SearchResult,
  EmbeddingIndexStatus as EmbeddingIndexStatusBinding,
} from './bindings';

// --- Error Handling ---

export type { MythicError };

/** Wraps an invoke call with error normalization. */
async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err: unknown) {
    // Tauri serializes MythicError as { error, message }
    if (typeof err === 'object' && err !== null && 'message' in err) {
      throw err as MythicError;
    }
    throw { error: 'unknown', message: String(err) } as MythicError;
  }
}

// --- Types matching Rust models ---

export type { Character };

/** 'character' (shared) | 'conversation' (isolated) | 'none' (disabled) */
export type Conversation = Omit<Conversation_Serialize, 'memory_scope'> & {
  memory_scope: 'character' | 'conversation' | 'none';
};

export type { Message };

export type { ProviderConfig };

export interface StreamEvent {
  event_type: 'delta' | 'done' | 'error';
  content: string;
  message_id: string;
}

export type { AppInfo };

// --- App ---

export async function getAppInfo(): Promise<AppInfo> {
  return safeInvoke<AppInfo>('get_app_info');
}

// --- Characters ---

export async function createCharacter(name: string, data: Record<string, unknown>): Promise<Character> {
  return safeInvoke<Character>('create_character', { name, data });
}

export async function getCharacter(id: string): Promise<Character> {
  return safeInvoke<Character>('get_character', { id });
}

export async function listCharacters(): Promise<Character[]> {
  return safeInvoke<Character[]>('list_characters');
}

export async function updateCharacter(
  id: string,
  name?: string,
  data?: Record<string, unknown>,
  avatarPath?: string
): Promise<Character> {
  return safeInvoke<Character>('update_character', {
    id,
    name: name ?? null,
    data: data ?? null,
    avatarPath: avatarPath ?? null,
  });
}

export async function deleteCharacter(id: string): Promise<void> {
  return safeInvoke<void>('delete_character', { id });
}

export async function importCharacterCard(filePath: string): Promise<Character> {
  return safeInvoke<Character>('import_character_card', { filePath });
}

export async function getAvatarPath(avatarRelative: string): Promise<string> {
  return safeInvoke<string>('get_avatar_path', { avatarRelative });
}

// --- Conversations ---

export async function createConversation(
  characterId?: string,
  title?: string
): Promise<Conversation> {
  return safeInvoke<Conversation>('create_conversation', {
    characterId: characterId ?? null,
    title: title ?? null,
  });
}

export async function getConversation(id: string): Promise<Conversation> {
  return safeInvoke<Conversation>('get_conversation', { id });
}

export async function listConversations(limit?: number, offset?: number): Promise<Conversation[]> {
  return safeInvoke<Conversation[]>('list_conversations', {
    limit: limit ?? null,
    offset: offset ?? null,
  });
}

export async function countConversations(): Promise<number> {
  return safeInvoke<number>('count_conversations');
}

export async function deleteConversation(id: string): Promise<void> {
  return safeInvoke<void>('delete_conversation', { id });
}

export async function updateConversation(id: string, title: string): Promise<Conversation> {
  return safeInvoke<Conversation>('update_conversation', { id, title });
}

export async function getConversationMessages(conversationId: string): Promise<Message[]> {
  return safeInvoke<Message[]>('get_conversation_messages', {
    conversationId,
  });
}

export async function setActiveMessage(conversationId: string, messageId: string): Promise<void> {
  return safeInvoke<void>('set_active_message', {
    conversationId,
    messageId,
  });
}

/** Set per-conversation memory scope: 'character' (shared), 'conversation' (isolated), or 'none' (disabled). */
export async function setMemoryScope(conversationId: string, scope: 'character' | 'conversation' | 'none'): Promise<void> {
  return safeInvoke<void>('set_memory_scope', { conversationId, scope });
}

/**
 * Creates a new conversation branched from an existing one.
 *
 * The new conversation contains a copy of all messages up to `branchPointMessageId`,
 * inheriting the same character and memory scope as the parent. All memories from the
 * parent are copied with 'copy' links, which render as dashed COPY connectors in
 * MemoryGraph and MemoryTimeline.
 */
export async function branchConversation(
  parentConversationId: string,
  branchPointMessageId: string,
  newTitle?: string,
): Promise<Conversation> {
  return safeInvoke<Conversation>('branch_conversation', {
    parentConversationId,
    branchPointMessageId,
    newTitle: newTitle ?? null,
  });
}

// --- Character State ---

export type { CharacterState };

/** Returns the current emotional state for a character in a conversation, or null if not yet set. */
export async function getCharacterState(
  characterId:    string,
  conversationId: string,
): Promise<CharacterState | null> {
  return safeInvoke<CharacterState | null>('get_character_state', {
    characterId,
    conversationId,
  });
}

/** Upserts the emotional state — creates on first call, updates on subsequent calls. */
export async function upsertCharacterState(
  characterId:     string,
  conversationId:  string,
  mood:            number,
  trust:           number,
  arousal:         number,
  dominantEmotion: string,
  stateSummary:    string,
): Promise<CharacterState> {
  return safeInvoke<CharacterState>('upsert_character_state', {
    characterId,
    conversationId,
    mood,
    trust,
    arousal,
    dominantEmotion,
    stateSummary,
  });
}

// --- Messages ---

export async function createMessage(
  conversationId: string,
  role: 'user' | 'assistant' | 'system',
  content: string,
  parentId?: string,
  metadata?: Record<string, unknown>
): Promise<Message> {
  return safeInvoke<Message>('create_message', {
    conversationId,
    role,
    content,
    parentId: parentId ?? null,
    metadata: metadata ?? null,
  });
}

export async function updateMessage(id: string, content: string): Promise<Message> {
  return safeInvoke<Message>('update_message', { id, content });
}

export async function deleteMessage(id: string): Promise<void> {
  return safeInvoke<void>('delete_message', { id });
}

export async function getMessageBranch(messageId: string): Promise<Message[]> {
  return safeInvoke<Message[]>('get_message_branch', { messageId });
}

export async function getMessageSiblings(messageId: string): Promise<Message[]> {
  return safeInvoke<Message[]>('get_message_siblings', { messageId });
}

// --- Providers ---

export async function createProvider(
  name: string,
  providerType: string,
  adapter: string,
  config: Record<string, unknown>,
  isDefault?: boolean
): Promise<ProviderConfig> {
  return safeInvoke<ProviderConfig>('create_provider', {
    name,
    providerType,
    adapter,
    config,
    isDefault: isDefault ?? false,
  });
}

export async function getProvider(id: string): Promise<ProviderConfig> {
  return safeInvoke<ProviderConfig>('get_provider', { id });
}

export async function listProviders(providerType?: string): Promise<ProviderConfig[]> {
  return safeInvoke<ProviderConfig[]>('list_providers', {
    providerType: providerType ?? null,
  });
}

export async function updateProvider(
  id: string,
  name?: string,
  config?: Record<string, unknown>
): Promise<ProviderConfig> {
  return safeInvoke<ProviderConfig>('update_provider', {
    id,
    name: name ?? null,
    config: config ?? null,
  });
}

export async function deleteProvider(id: string): Promise<void> {
  return safeInvoke<void>('delete_provider', { id });
}

export async function setDefaultProvider(id: string): Promise<void> {
  return safeInvoke<void>('set_default_provider', { id });
}

export async function testProviderConnection(id: string): Promise<boolean> {
  return safeInvoke<boolean>('test_provider_connection', { id });
}

export async function listProviderModels(id: string): Promise<string[]> {
  return safeInvoke<string[]>('list_provider_models', { id });
}

export type { ModelEntry };

export async function listAllModels(): Promise<ModelEntry[]> {
  return safeInvoke<ModelEntry[]>('list_all_models');
}

export async function listEmbeddingModels(): Promise<ModelEntry[]> {
  return safeInvoke<ModelEntry[]>('list_embedding_models');
}

export async function toggleModelEnabled(
  providerId: string,
  modelId: string,
  modelType: string,
  enabled: boolean,
): Promise<void> {
  return safeInvoke<void>('toggle_model_enabled', {
    providerId, modelId, modelType, enabled,
  });
}

export async function listEnabledModels(providerId?: string): Promise<ModelEntry[]> {
  return safeInvoke<ModelEntry[]>('list_enabled_models', { providerId: providerId ?? null });
}

// --- Chat ---

export type { SendMessageResult };

export async function sendMessage(
  conversationId: string,
  content: string,
  model?: string,
  systemPrompt?: string,
  streaming?: boolean,
  postHistoryInstructions?: string,
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('send_message', {
    conversationId,
    content,
    model: model ?? null,
    systemPrompt: systemPrompt ?? null,
    streaming: streaming ?? null,
    postHistoryInstructions: postHistoryInstructions ?? null,
  });
}

export async function regenerateMessage(
  conversationId: string,
  messageId: string,
  model?: string,
  systemPrompt?: string,
  streaming?: boolean,
  postHistoryInstructions?: string,
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('regenerate_message', {
    conversationId,
    messageId,
    model: model ?? null,
    systemPrompt: systemPrompt ?? null,
    streaming: streaming ?? null,
    postHistoryInstructions: postHistoryInstructions ?? null,
  });
}

/**
 * Retries a failed message by reusing the existing user message in the DB.
 * Unlike sendMessage, this does NOT insert a new user message — it just
 * re-triggers LLM generation from the one that already exists.
 */
export async function retryFailedMessage(
  conversationId: string,
  userMessageId: string,
  model?: string,
  systemPrompt?: string,
  streaming?: boolean,
  postHistoryInstructions?: string,
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('retry_failed_message', {
    conversationId,
    userMessageId,
    model: model ?? null,
    systemPrompt: systemPrompt ?? null,
    streaming: streaming ?? null,
    postHistoryInstructions: postHistoryInstructions ?? null,
  });
}

/**
 * Subscribes to chat stream events from the backend.
 * Returns an unlisten function to stop listening.
 */
export async function onChatStream(
  callback: (event: StreamEvent) => void
): Promise<UnlistenFn> {
  return listen<StreamEvent>('chat-stream', (event) => {
    callback(event.payload);
  });
}

// --- Context Stats ---

export type { ContextStats };

export async function getContextStats(
  conversationId: string,
  messageId: string,
  systemPrompt?: string,
  postHistoryInstructions?: string,
): Promise<ContextStats> {
  return safeInvoke<ContextStats>('get_context_stats', {
    conversationId,
    messageId,
    systemPrompt: systemPrompt ?? null,
    postHistoryInstructions: postHistoryInstructions ?? null,
  });
}

// --- Scenes ---

export type Scene = Omit<Scene_Serialize, 'media_type'> & {
  media_type: 'image' | 'video';
};

export async function generateScene(
  conversationId: string,
  prompt: string,
  options?: {
    messageId?: string;
    negativePrompt?: string;
    width?: number;
    height?: number;
  }
): Promise<Scene> {
  return safeInvoke<Scene>('generate_scene', {
    conversationId,
    messageId: options?.messageId ?? null,
    prompt,
    negativePrompt: options?.negativePrompt ?? null,
    width: options?.width ?? null,
    height: options?.height ?? null,
  });
}

export async function listScenes(conversationId: string): Promise<Scene[]> {
  return safeInvoke<Scene[]>('list_scenes', { conversationId });
}

export async function deleteScene(sceneId: string): Promise<void> {
  return safeInvoke<void>('delete_scene', { sceneId });
}

export async function getScenePath(fileRelative: string): Promise<string> {
  return safeInvoke<string>('get_scene_path', { fileRelative });
}

// --- Scene State ---

export type { SceneState };

export async function getSceneState(conversationId: string): Promise<SceneState | null> {
  return safeInvoke<SceneState | null>('get_scene_state', { conversationId });
}

export async function upsertSceneState(
  conversationId: string,
  update: Partial<Omit<SceneState, 'id' | 'conversation_id' | 'updated_at'>>
): Promise<SceneState> {
  return safeInvoke<SceneState>('upsert_scene_state', {
    conversationId,
    locationName: update.location_name ?? null,
    locationDescription: update.location_description ?? null,
    timePeriod: update.time_period ?? null,
    weather: update.weather ?? null,
    charactersPresent: update.characters_present ?? null,
    ambientDetails: update.ambient_details ?? null,
    sceneMood: update.scene_mood ?? null,
  });
}

export async function deleteSceneState(conversationId: string): Promise<void> {
  return safeInvoke<void>('delete_scene_state', { conversationId });
}

// --- Conversation Characters ---

export type { ConversationCharacter };

export async function listConversationCharacters(conversationId: string): Promise<ConversationCharacter[]> {
  return safeInvoke<ConversationCharacter[]>('list_conversation_characters', { conversationId });
}

export async function addConversationCharacter(
  conversationId: string,
  characterId: string,
  characterName: string,
  role?: string,
  talkativeness?: number,
): Promise<ConversationCharacter> {
  return safeInvoke<ConversationCharacter>('add_conversation_character', {
    conversationId,
    characterId,
    characterName,
    role: role ?? null,
    talkativeness: talkativeness ?? null,
  });
}

export async function removeConversationCharacter(
  conversationId: string,
  characterId: string,
): Promise<void> {
  return safeInvoke<void>('remove_conversation_character', { conversationId, characterId });
}

export async function updateCharacterTalkativeness(
  conversationId: string,
  characterId: string,
  talkativeness: number,
): Promise<void> {
  return safeInvoke<void>('update_character_talkativeness', { conversationId, characterId, talkativeness });
}

export async function toggleCharacterActive(
  conversationId: string,
  characterId: string,
  isActive: boolean,
): Promise<void> {
  return safeInvoke<void>('toggle_character_active', { conversationId, characterId, isActive });
}

// --- Lorebook ---

export type { LorebookEntry };

export async function listLorebookEntries(characterId: string): Promise<LorebookEntry[]> {
  return safeInvoke<LorebookEntry[]>('list_lorebook_entries', { characterId });
}

export async function createLorebookEntry(
  characterId: string | null,
  name: string,
  keys: string[],
  content: string,
  alwaysActive: boolean = false,
): Promise<LorebookEntry> {
  return safeInvoke<LorebookEntry>('create_lorebook_entry', {
    characterId,
    name,
    keys,
    content,
    alwaysActive,
  });
}

export async function toggleLorebookEntry(id: string, enabled: boolean): Promise<void> {
  return safeInvoke<void>('toggle_lorebook_entry', { id, enabled });
}

export async function deleteLorebookEntry(id: string): Promise<void> {
  return safeInvoke<void>('delete_lorebook_entry', { id });
}

// --- Memories ---

export type { Memory };

/**
 * A link between a source memory and a target conversation.
 * Constraint: copy is always one_way; only sync can be two_way.
 */
export type MemoryLink = Omit<MemoryLink_Serialize, 'link_type' | 'direction' | 'sync_mode'> & {
  /** 'copy' = frozen snapshot (always one_way), 'sync' = live link */
  link_type: 'copy' | 'sync';
  /** 'one_way' = source→target, 'two_way' = bidirectional (sync only) */
  direction: 'one_way' | 'two_way';
  sync_mode: 'auto' | 'manual';
};

export type { MemoryGraphConversation };

export interface MemoryGraph {
  character_id: string;
  character_name: string;
  /** Present when multiple characters are selected — one entry per character */
  characters?: { id: string; name: string }[];
  memories: Memory[];
  links: MemoryLink[];
  conversations: MemoryGraphConversation[];
}

export async function listMemories(
  characterId?: string,
  conversationId?: string,
): Promise<Memory[]> {
  return safeInvoke<Memory[]>('list_memories', {
    characterId: characterId ?? null,
    conversationId: conversationId ?? null,
  });
}

export async function createMemory(
  content: string,
  characterId?: string,
  conversationId?: string,
  source?: string,
): Promise<Memory> {
  return safeInvoke<Memory>('create_memory', {
    characterId: characterId ?? null,
    conversationId: conversationId ?? null,
    content,
    source: source ?? null,
  });
}

export async function updateMemory(memoryId: string, content: string): Promise<Memory> {
  return safeInvoke<Memory>('update_memory', { memoryId, content });
}

/** Sets a memory's importance tier (1-10), weighting retrieval ranking alongside relevance and recency. */
export async function setMemoryImportance(memoryId: string, importance: number): Promise<Memory> {
  return safeInvoke<Memory>('set_memory_importance', { memoryId, importance });
}

export async function deleteMemory(memoryId: string): Promise<void> {
  return safeInvoke<void>('delete_memory', { memoryId });
}

export async function promoteToCanon(memoryId: string): Promise<Memory> {
  return safeInvoke<Memory>('promote_to_canon', { memoryId });
}

export async function shareMemory(
  sourceMemoryId: string,
  targetConversationId: string,
  linkType?: 'copy' | 'sync',
  direction?: 'one_way' | 'two_way',
  syncMode?: 'auto' | 'manual',
): Promise<MemoryLink> {
  return safeInvoke<MemoryLink>('share_memory', {
    sourceMemoryId,
    targetConversationId,
    linkType: linkType ?? null,
    direction: direction ?? null,
    syncMode: syncMode ?? null,
  });
}

export async function unlinkMemory(linkId: string): Promise<void> {
  return safeInvoke<void>('unlink_memory', { linkId });
}

export async function getMemoryGraph(characterId: string): Promise<MemoryGraph> {
  return safeInvoke<MemoryGraph>('get_memory_graph', { characterId });
}

// --- Search ---

export type { SearchResult };

/** Searches message content using FTS5 full-text search. */
export async function searchMessages(query: string, limit?: number): Promise<SearchResult[]> {
  return safeInvoke<SearchResult[]>('search_messages', { query, limit: limit ?? null });
}

// --- Raw Generation (Internal pipelines) ---

/**
 * Stateless LLM generation — calls the configured provider without saving
 * anything to the database. Used by internal pipelines like memory extraction.
 */
export async function generateRaw(
  systemPrompt: string,
  userPrompt: string,
  model?: string,
  maxTokens?: number,
  temperature?: number,
): Promise<string> {
  return safeInvoke<string>('generate_raw', {
    systemPrompt,
    userPrompt,
    model: model ?? null,
    maxTokens: maxTokens ?? null,
    temperature: temperature ?? null,
  });
}

// --- Embedding Index ---

/** `coverage_percent` is pinned back to non-null `number`: the backend field is a
 *  plain (never-`Option`) `f64`, but specta conservatively types all floats as
 *  `number | null` on export (NaN/Infinity aren't representable in JSON). */
export type EmbeddingIndexStatus = Omit<EmbeddingIndexStatusBinding, 'coverage_percent'> & {
  coverage_percent: number;
};

export async function getEmbeddingIndexStatus(
  conversationId?: string | null,
  selectedModel?: string,
): Promise<EmbeddingIndexStatus> {
  return safeInvoke<EmbeddingIndexStatus>('get_embedding_index_status', {
    conversationId: conversationId ?? null,
    selectedModel: selectedModel ?? null,
  });
}

export async function rebuildEmbeddingIndex(
  conversationId?: string | null,
  embeddingModel?: string,
): Promise<EmbeddingIndexStatus> {
  return safeInvoke<EmbeddingIndexStatus>('rebuild_embedding_index', {
    conversationId: conversationId ?? null,
    embeddingModel: embeddingModel ?? 'openai/text-embedding-3-small',
  });
}

export async function backfillMissingEmbeddings(
  conversationId?: string | null,
): Promise<EmbeddingIndexStatus> {
  return safeInvoke<EmbeddingIndexStatus>('backfill_missing_embeddings', {
    conversationId: conversationId ?? null,
  });
}
