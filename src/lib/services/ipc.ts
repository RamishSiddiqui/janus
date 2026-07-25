/**
 * Type-safe IPC bridge for communicating with the Tauri/Rust backend.
 *
 * All `invoke()` calls go through this module to centralize error handling
 * and provide typed responses. The frontend stores should use these functions
 * instead of calling `invoke()` directly.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// --- Error Handling ---

export interface MythicError {
  error: string;
  message: string;
}

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

export interface Character {
  id: string;
  name: string;
  spec: string;
  data: string;
  avatar_path: string | null;
  created_at: string;
  updated_at: string;
}

export interface Conversation {
  id: string;
  title: string;
  character_id: string | null;
  active_message_id: string | null;
  /** 'character' (shared) | 'conversation' (isolated) | 'none' (disabled) */
  memory_scope: 'character' | 'conversation' | 'none';
  shared_character_ids: string | null;
  /** Set if this conversation was branched from another. */
  parent_conversation_id: string | null;
  /** The exact message in the parent conversation where the fork happened. */
  branch_point_message_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  conversation_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  parent_id: string | null;
  metadata: Record<string, unknown> | null;
  created_at: string;
  character_id?: string | null;
  character_name?: string | null;
}

export interface ProviderConfig {
  id: string;
  name: string;
  provider_type: 'llm' | 'image' | 'video';
  adapter: string;
  config: Record<string, unknown>;
  is_default: boolean;
}

export interface StreamEvent {
  event_type: 'delta' | 'done' | 'error';
  content: string;
  message_id: string;
}

export interface AppInfo {
  name: string;
  version: string;
  description: string;
}

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

export interface CharacterState {
  id:               string;
  character_id:     string;
  conversation_id:  string;
  mood:             number;
  trust:            number;
  arousal:          number;
  dominant_emotion: string;
  state_summary:    string;
  updated_at:       string;
}

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

export interface ModelEntry {
  model_id: string;
  provider_id: string;
  provider_name: string;
  adapter: string;
  model_type: string;
  context_length: number | null;
  enabled: boolean;
  // Rich metadata (populated from OpenRouter API)
  display_name: string | null;
  description: string | null;
  pricing_prompt: string | null;
  pricing_completion: string | null;
  is_free: boolean;
  max_completion_tokens: number | null;
  input_modalities: string[];
  output_modalities: string[];
  supports_tools: boolean;
  supports_vision: boolean;
  supports_reasoning: boolean;
  embedding_dimensions: number | null;
}

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

export interface SendMessageResult {
  user_message_id: string;
  assistant_message_id: string;
}

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

export interface ContextStats {
  total_budget: number;
  fixed_tokens: number;
  history_tokens: number;
  summary_tokens: number;
  total_messages: number;
  included_messages: number;
  evicted_messages: number;
}

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

export interface Scene {
  id: string;
  conversation_id: string;
  message_id: string | null;
  media_type: 'image' | 'video';
  prompt: string;
  file_path: string;
  caption: string | null;
  metadata: Record<string, unknown> | null;
  created_at: string;
}

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

export interface SceneState {
  id: string;
  conversation_id: string;
  location_name: string;
  location_description: string;
  time_period: string;
  weather: string;
  characters_present: string[];
  ambient_details: string;
  scene_mood: string;
  updated_at: string;
}

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

export interface ConversationCharacter {
  id: string;
  conversation_id: string;
  character_id: string;
  role: string;          // 'primary' | 'secondary' | 'npc'
  talkativeness: number; // 0-100
  is_active: boolean;
  character_name: string;
  created_at: string;
}

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

export interface LorebookEntry {
  id: string;
  character_id: string | null;
  keys: string[];
  content: string;
  enabled: boolean;
  always_active: boolean;
  priority: number;
  insertion_order: number;
  name: string | null;
}

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

export interface Memory {
  id: string;
  character_id: string | null;
  conversation_id: string | null;
  content: string;
  source: string;
  parent_id: string | null;
  version: number;
  is_canon: boolean;
  created_at: string;
  /** Manual importance tier (1-10, default 5/neutral) weighting retrieval ranking. */
  importance: number;
  /** When this memory was last surfaced via retrieval, if ever. */
  last_accessed: string | null;
  /** How many times this memory has been surfaced via retrieval. */
  access_count: number;
}

/**
 * A link between a source memory and a target conversation.
 * Constraint: copy is always one_way; only sync can be two_way.
 */
export interface MemoryLink {
  id: string;
  source_memory_id: string;
  target_conversation_id: string;
  /** 'copy' = frozen snapshot (always one_way), 'sync' = live link */
  link_type: 'copy' | 'sync';
  /** 'one_way' = source→target, 'two_way' = bidirectional (sync only) */
  direction: 'one_way' | 'two_way';
  sync_mode: 'auto' | 'manual';
  linked_memory_id: string | null;
  created_at: string;
}

export interface MemoryGraphConversation {
  id: string;
  title: string;
  character_id: string;
  memory_count: number;
  /** If this conversation was branched from another, this is the parent's ID. */
  parent_conversation_id: string | null;
}

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

export interface SearchResult {
  message_id: string;
  conversation_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  /** FTS5 snippet with <mark> tags around matched terms */
  snippet: string;
  conversation_title: string;
  character_name: string | null;
  created_at: string;
}

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

export interface EmbeddingIndexStatus {
  total_messages: number;
  embedded_messages: number;
  index_model: string | null;
  needs_rebuild: boolean;
  coverage_percent: number;
  /** Dimension of existing stored embeddings (null if none exist) */
  index_dimension: number | null;
  /** Dimension of the currently selected embedding model */
  selected_dimension: number | null;
  /** True when stored embeddings have different dimensions than the selected model */
  dimension_mismatch: boolean;
}

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
