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
    avatar_path: avatarPath ?? null,
  });
}

export async function deleteCharacter(id: string): Promise<void> {
  return safeInvoke<void>('delete_character', { id });
}

export async function importCharacterCard(filePath: string): Promise<Character> {
  return safeInvoke<Character>('import_character_card', { file_path: filePath });
}

export async function getAvatarPath(avatarRelative: string): Promise<string> {
  return safeInvoke<string>('get_avatar_path', { avatar_relative: avatarRelative });
}

// --- Conversations ---

export async function createConversation(
  characterId?: string,
  title?: string
): Promise<Conversation> {
  return safeInvoke<Conversation>('create_conversation', {
    character_id: characterId ?? null,
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
    conversation_id: conversationId,
  });
}

export async function setActiveMessage(conversationId: string, messageId: string): Promise<void> {
  return safeInvoke<void>('set_active_message', {
    conversation_id: conversationId,
    message_id: messageId,
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
    conversation_id: conversationId,
    role,
    content,
    parent_id: parentId ?? null,
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
  return safeInvoke<Message[]>('get_message_branch', { message_id: messageId });
}

export async function getMessageSiblings(messageId: string): Promise<Message[]> {
  return safeInvoke<Message[]>('get_message_siblings', { message_id: messageId });
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
    provider_type: providerType,
    adapter,
    config,
    is_default: isDefault ?? false,
  });
}

export async function getProvider(id: string): Promise<ProviderConfig> {
  return safeInvoke<ProviderConfig>('get_provider', { id });
}

export async function listProviders(providerType?: string): Promise<ProviderConfig[]> {
  return safeInvoke<ProviderConfig[]>('list_providers', {
    provider_type: providerType ?? null,
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
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('send_message', {
    conversation_id: conversationId,
    content,
    model: model ?? null,
    system_prompt: systemPrompt ?? null,
    streaming: streaming ?? null,
  });
}

export async function regenerateMessage(
  conversationId: string,
  messageId: string,
  model?: string,
  systemPrompt?: string,
  streaming?: boolean,
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('regenerate_message', {
    conversation_id: conversationId,
    message_id: messageId,
    model: model ?? null,
    system_prompt: systemPrompt ?? null,
    streaming: streaming ?? null,
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
    conversation_id: conversationId,
    message_id: options?.messageId ?? null,
    prompt,
    negative_prompt: options?.negativePrompt ?? null,
    width: options?.width ?? null,
    height: options?.height ?? null,
  });
}

export async function listScenes(conversationId: string): Promise<Scene[]> {
  return safeInvoke<Scene[]>('list_scenes', { conversation_id: conversationId });
}

export async function deleteScene(sceneId: string): Promise<void> {
  return safeInvoke<void>('delete_scene', { scene_id: sceneId });
}

export async function getScenePath(fileRelative: string): Promise<string> {
  return safeInvoke<string>('get_scene_path', { file_relative: fileRelative });
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
  return safeInvoke<LorebookEntry[]>('list_lorebook_entries', { character_id: characterId });
}

export async function createLorebookEntry(
  characterId: string | null,
  name: string,
  keys: string[],
  content: string,
  alwaysActive: boolean = false,
): Promise<LorebookEntry> {
  return safeInvoke<LorebookEntry>('create_lorebook_entry', {
    character_id: characterId,
    name,
    keys,
    content,
    always_active: alwaysActive,
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
  created_at: string;
}

export async function listMemories(
  characterId?: string,
  conversationId?: string,
): Promise<Memory[]> {
  return safeInvoke<Memory[]>('list_memories', {
    character_id: characterId ?? null,
    conversation_id: conversationId ?? null,
  });
}

export async function createMemory(
  content: string,
  characterId?: string,
  conversationId?: string,
  source?: string,
): Promise<Memory> {
  return safeInvoke<Memory>('create_memory', {
    character_id: characterId ?? null,
    conversation_id: conversationId ?? null,
    content,
    source: source ?? null,
  });
}

export async function deleteMemory(memoryId: string): Promise<void> {
  return safeInvoke<void>('delete_memory', { memory_id: memoryId });
}
