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
  ImagePreset_Serialize as ImagePreset,
  Persona_Serialize as Persona,
  ConnectionTestResult,
  ProfileRefreshResult,
} from './bindings';

// --- Error Handling ---

export type { MythicError };

/** Wraps an invoke call with error normalization. */
async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err: unknown) {
    // Every backend command failure gets logged here, once, regardless of
    // whether the specific call site's own catch block remembers to —
    // mirrors the backend's own MythicError::Serialize impl, which logs
    // every IPC error unconditionally at the source. Without this, a call
    // site whose catch block only shows a toast (no console.error) left
    // that failure with zero trace in the Logging tab, no way to tell what
    // actually went wrong after the fact.
    console.error(`[ipc] ${cmd} failed:`, err);
    // Tauri serializes MythicError as { error, message }
    if (typeof err === 'object' && err !== null && 'message' in err) {
      throw err as MythicError;
    }
    throw { error: 'unknown', message: String(err) } as MythicError;
  }
}

// --- Types matching Rust models ---

export type { Character };

export type { Persona };

/** 'character' (shared) | 'conversation' (isolated) | 'none' (disabled) */
export type Conversation = Omit<Conversation_Serialize, 'memory_scope'> & {
  memory_scope: 'character' | 'conversation' | 'none';
};

export type { Message };

export type { ProviderConfig };

export interface StreamEvent {
  /** 'cancelled' fires when the user stops generation early via cancelGeneration() —
   *  distinct from 'done' so the frontend can skip the auto-memory/emotion pipelines
   *  that shouldn't run over content the user explicitly cut off.
   *  'reasoning' carries a chain-of-thought delta from a reasoning model — kept
   *  separate from 'delta' so it never renders as if the character said it. */
  event_type: 'delta' | 'reasoning' | 'done' | 'error' | 'cancelled';
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

/** Permanently deletes a character — only the Trash view should call this. */
export async function deleteCharacter(id: string): Promise<void> {
  return safeInvoke<void>('delete_character', { id });
}

/** Moves a character to Trash (reversible via restoreCharacter). */
export async function trashCharacter(id: string): Promise<Character> {
  return safeInvoke<Character>('trash_character', { id });
}

export async function restoreCharacter(id: string): Promise<Character> {
  return safeInvoke<Character>('restore_character', { id });
}

export async function importCharacterCard(filePath: string): Promise<Character> {
  return safeInvoke<Character>('import_character_card', { filePath });
}

export async function getAvatarPath(avatarRelative: string): Promise<string> {
  return safeInvoke<string>('get_avatar_path', { avatarRelative });
}

// --- Personas (user-controlled profiles) ---

export async function createPersona(name: string, data: Record<string, unknown>): Promise<Persona> {
  return safeInvoke<Persona>('create_persona', { name, data });
}

export async function getPersona(id: string): Promise<Persona> {
  return safeInvoke<Persona>('get_persona', { id });
}

export async function listPersonas(): Promise<Persona[]> {
  return safeInvoke<Persona[]>('list_personas');
}

export async function updatePersona(
  id: string,
  name?: string,
  data?: Record<string, unknown>,
  avatarPath?: string
): Promise<Persona> {
  return safeInvoke<Persona>('update_persona', {
    id,
    name: name ?? null,
    data: data ?? null,
    avatarPath: avatarPath ?? null,
  });
}

/** Permanently deletes a persona — only the Trash view should call this. */
export async function deletePersona(id: string): Promise<void> {
  return safeInvoke<void>('delete_persona', { id });
}

/** Moves a persona to Trash (reversible via restorePersona). */
export async function trashPersona(id: string): Promise<Persona> {
  return safeInvoke<Persona>('trash_persona', { id });
}

export async function restorePersona(id: string): Promise<Persona> {
  return safeInvoke<Persona>('restore_persona', { id });
}

export async function importPersonaCard(filePath: string): Promise<Persona> {
  return safeInvoke<Persona>('import_persona_card', { filePath });
}

/** Generates a portrait for a persona via the configured image provider. A no-op (persona returned unchanged) if none is configured. */
export async function generatePersonaPortrait(personaId: string, conversationId?: string): Promise<Persona> {
  return safeInvoke<Persona>('generate_persona_portrait', {
    personaId,
    conversationId: conversationId ?? null,
  });
}

export async function setConversationPersona(conversationId: string, personaId: string | null): Promise<void> {
  return safeInvoke<void>('set_conversation_persona', { conversationId, personaId });
}

// --- NPCs (auto-generated cast members) ---

/** Lists the auto-generated NPCs currently in a conversation's cast (both never-promoted and promoted-but-still-cast). */
export async function listConversationNpcs(conversationId: string): Promise<Character[]> {
  return safeInvoke<Character[]>('list_conversation_npcs', { conversationId });
}

/** Promotes an NPC into a real, standalone Gallery character. No memory/cast data moves. */
export async function promoteNpcToGallery(characterId: string): Promise<Character> {
  return safeInvoke<Character>('promote_npc_to_gallery', { characterId });
}

/** Manually promotes an NPC from "Unconfirmed" (role: transient) to a confirmed cast member (role: npc), skipping the automatic detection debounce. */
export async function confirmNpc(conversationId: string, characterId: string): Promise<void> {
  return safeInvoke<void>('confirm_npc', { conversationId, characterId });
}

/** Marks an NPC's auto-generated profile as reviewed, clearing the needs-attention indicator. */
export async function markNpcReviewed(characterId: string): Promise<Character> {
  return safeInvoke<Character>('mark_npc_reviewed', { characterId });
}

/** Dev-only: runs the NPC detection pipeline directly against a hand-written narrative, bypassing live chat. Phase A verification only. */
export async function debugRunNpcDetection(conversationId: string, aiResponse: string): Promise<void> {
  return safeInvoke<void>('debug_run_npc_detection', { conversationId, aiResponse });
}

/** Generates a portrait for an NPC via the configured image provider. A no-op (character returned unchanged) if none is configured. */
export async function generateNpcPortrait(characterId: string, conversationId: string, autoApprove: boolean): Promise<Character> {
  return safeInvoke<Character>('generate_npc_portrait', { characterId, conversationId, autoApprove });
}

/** Sets a character's portrait directly from a user-picked image file, bypassing AI generation. Always approved (no review gate). */
export async function uploadCharacterAvatar(characterId: string, filePath: string): Promise<Character> {
  return safeInvoke<Character>('upload_character_avatar', { characterId, filePath });
}

/** Last `lines` (default 1000) lines of the persisted backend log file — empty string if nothing's been logged yet. */
export async function getBackendLogs(lines?: number): Promise<string> {
  return safeInvoke<string>('get_backend_logs', { lines: lines ?? null });
}

/** One page of backend log lines read backward from `cursor` (a byte offset; omit for the newest lines). Used to load the Logging tab's viewer incrementally as the user scrolls up, instead of reading the whole file at once. */
export async function getBackendLogsPage(cursor?: number, limit?: number): Promise<{ lines: string[]; nextCursor: number | null }> {
  return safeInvoke('get_backend_logs_page', { cursor: cursor ?? null, limit: limit ?? null });
}

/** Absolute path to the backend log file, for display/troubleshooting. */
export async function getBackendLogPath(): Promise<string> {
  return safeInvoke<string>('get_backend_log_path');
}

export type { ProfileRefreshResult };

/** Refreshes an auto-detected character's description/personality/scenario against how they've actually appeared in this conversation's story so far. `systemPrompt` is the user-editable override from Settings > Prompts > Character Profile Refresh — pass undefined to use the built-in default. If the character is shared across multiple conversations, the refresh is saved as a conversation-scoped memory instead of touching the shared card (see `ProfileRefreshResult.scope`). */
export async function refreshCharacterProfile(characterId: string, conversationId: string, systemPrompt?: string): Promise<ProfileRefreshResult> {
  return safeInvoke<ProfileRefreshResult>('refresh_character_profile', { characterId, conversationId, systemPrompt: systemPrompt ?? null });
}

/** Approves a pending NPC portrait — the avatar image itself is unchanged. */
export async function approveNpcPortrait(characterId: string): Promise<Character> {
  return safeInvoke<Character>('approve_npc_portrait', { characterId });
}

/** Rejects a pending NPC portrait, clearing the avatar back to the placeholder. */
export async function rejectNpcPortrait(characterId: string): Promise<Character> {
  return safeInvoke<Character>('reject_npc_portrait', { characterId });
}

/** Multi-character memory graph scoped to one conversation's cast (gallery mains + NPCs), additive to the per-character getMemoryGraph. */
export async function getCastMemoryGraph(conversationId: string): Promise<MemoryGraph> {
  return safeInvoke<MemoryGraph>('get_cast_memory_graph', { conversationId });
}

// --- Conversations ---

export async function createConversation(
  characterId?: string,
  title?: string,
  personaId?: string
): Promise<Conversation> {
  return safeInvoke<Conversation>('create_conversation', {
    characterId: characterId ?? null,
    title: title ?? null,
    personaId: personaId ?? null,
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

/** Permanently deletes a conversation — only the Trash view should call this. */
export async function deleteConversation(id: string): Promise<void> {
  return safeInvoke<void>('delete_conversation', { id });
}

/** Moves a conversation to Trash (reversible via restoreConversation). */
export async function trashConversation(id: string): Promise<Conversation> {
  return safeInvoke<Conversation>('trash_conversation', { id });
}

export async function restoreConversation(id: string): Promise<Conversation> {
  return safeInvoke<Conversation>('restore_conversation', { id });
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

export type { ConnectionTestResult };

export async function testProviderConnection(id: string): Promise<ConnectionTestResult> {
  return safeInvoke<ConnectionTestResult>('test_provider_connection', { id });
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

/** A single image attached to a user message — see `MessageAttachment` on
 *  the backend (`models::conversation`). `relativePath` is relative to
 *  app_data_dir, same convention as avatars/portraits/scenes. */
export interface MessageAttachment {
  relativePath: string;
  mimeType: string;
}

/** Copies a user-picked image (an absolute path from the file dialog) into
 *  app_data_dir/attachments/, ready to be passed to `sendMessage`'s
 *  `attachments` param. Only png/jpg/jpeg/webp/gif are accepted. */
export async function uploadMessageAttachment(filePath: string): Promise<MessageAttachment> {
  return safeInvoke<MessageAttachment>('upload_message_attachment', { filePath });
}

/** Same as `uploadMessageAttachment`, but for an image pasted directly from
 *  the clipboard (e.g. a screenshot) — there's no file path, just raw bytes
 *  and the clipboard blob's MIME-derived extension (e.g. "png"). */
export async function uploadMessageAttachmentBytes(bytes: Uint8Array, extension: string): Promise<MessageAttachment> {
  return safeInvoke<MessageAttachment>('upload_message_attachment_bytes', {
    bytes: Array.from(bytes),
    extension,
  });
}

export async function sendMessage(
  conversationId: string,
  content: string,
  model?: string,
  systemPrompt?: string,
  streaming?: boolean,
  postHistoryInstructions?: string,
  attachments?: MessageAttachment[],
): Promise<SendMessageResult> {
  return safeInvoke<SendMessageResult>('send_message', {
    conversationId,
    content,
    model: model ?? null,
    systemPrompt: systemPrompt ?? null,
    streaming: streaming ?? null,
    postHistoryInstructions: postHistoryInstructions ?? null,
    attachments: attachments && attachments.length > 0 ? attachments : null,
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
 * Cancels the in-flight generation for a conversation, if any. A no-op if
 * nothing is currently generating. Whatever content had already streamed is
 * persisted server-side before the "cancelled" chat-stream event fires.
 */
export async function cancelGeneration(conversationId: string): Promise<void> {
  return safeInvoke<void>('cancel_generation', { conversationId });
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
    /** Overrides the model that would otherwise come from the resolved
     *  preset or the provider's own default, for this generation only. */
    modelOverride?: string;
    /** Path (relative to app_data_dir, e.g. a character's avatar_path) to
     *  use as an img2img reference for this generation. */
    referenceImagePath?: string;
    /** img2img strength when referenceImagePath is set (0.01-1.0, default 0.6). */
    denoisingStrength?: number;
    /** Mirrors the user's "Allow Mature Content" setting — sent as AI Horde's
     *  `nsfw` request flag so ordinary (non-explicit) descriptions aren't
     *  false-positive censored. Defaults to false (strict) if unset. */
    allowNsfw?: boolean;
    /** Cast portraits to feed into a ComfyUI workflow's `{{CHARACTER_IMAGE_n}}`
     *  tokens — ignored by every other provider adapter. See
     *  `listSceneCastMembers` for where the picker's options come from. */
    characterImages?: { characterId: string; characterName: string; relativePath: string }[];
  }
): Promise<Scene> {
  return safeInvoke<Scene>('generate_scene', {
    conversationId,
    messageId: options?.messageId ?? null,
    prompt,
    options: {
      negativePrompt: options?.negativePrompt ?? null,
      width: options?.width ?? null,
      height: options?.height ?? null,
      modelOverride: options?.modelOverride ?? null,
      referenceImagePath: options?.referenceImagePath ?? null,
      denoisingStrength: options?.denoisingStrength ?? null,
      allowNsfw: options?.allowNsfw ?? null,
      characterImages: options?.characterImages ?? null,
    },
  });
}

/** Everyone available as a portrait-reference source for this conversation's
 *  scene generation — the primary character plus its full cast roster (any
 *  role, including still-"Unconfirmed" transients). Used to populate the
 *  ComfyUI character-portrait picker in `SceneDisplay`. */
export interface SceneCastMember {
  characterId: string;
  name: string;
  avatarPath: string | null;
  role: string;
}

export async function listSceneCastMembers(conversationId: string): Promise<SceneCastMember[]> {
  return safeInvoke<SceneCastMember[]>('list_scene_cast_members', { conversationId });
}

export async function listScenes(conversationId: string): Promise<Scene[]> {
  return safeInvoke<Scene[]>('list_scenes', { conversationId });
}

export async function deleteScene(sceneId: string): Promise<void> {
  return safeInvoke<void>('delete_scene', { sceneId });
}

/** Signals a running scene generation for this conversation to stop. The
 *  poll loop notices within one tick and issues a best-effort cancel to AI
 *  Horde before returning an error. No-op if nothing is generating. */
export async function cancelSceneGeneration(conversationId: string): Promise<void> {
  return safeInvoke<void>('cancel_scene_generation', { conversationId });
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

/** Kicks off scene-state extraction for text that never went through the
 *  normal streaming pipeline — currently just a character's greeting, which
 *  is inserted directly rather than generated via send_message. Without
 *  this, scene state (and the default image-generation prompt) stays empty
 *  until the second AI turn. Fire-and-forget on the backend. */
export async function extractInitialScene(conversationId: string, text: string): Promise<void> {
  return safeInvoke<void>('extract_initial_scene', { conversationId, text });
}

// --- Image Presets ---

export type { ImagePreset };

export async function listImagePresets(): Promise<ImagePreset[]> {
  return safeInvoke<ImagePreset[]>('list_image_presets');
}

export async function createImagePreset(
  name: string,
  fields: {
    model?: string;
    samplerName?: string;
    cfgScale?: number;
    steps?: number;
    karras?: boolean;
    style?: string;
    negativePrompt?: string;
    isDefault?: boolean;
    /** CLIP layers to skip (1-12) — anime checkpoints (Pony V6 XL, AAM XL
     *  AnimeMix) typically expect 2. Omit to let AI Horde use its default (1). */
    clipSkip?: number;
    /** AI Horde post-processors, in order — e.g. ['GFPGAN', 'RealESRGAN_x4plus_anime_6B']. */
    postProcessing?: string[];
    /** Re-processes at higher resolution after the base generation — best
     *  single lever for composition/anatomy fixes, ~2x generation time/cost. */
    hiresFix?: boolean;
    hiresFixDenoisingStrength?: number;
  } = {}
): Promise<ImagePreset> {
  return safeInvoke<ImagePreset>('create_image_preset', {
    name,
    fields: {
      model: fields.model ?? null,
      samplerName: fields.samplerName ?? 'k_euler_a',
      cfgScale: fields.cfgScale ?? 7.5,
      steps: fields.steps ?? 30,
      karras: fields.karras ?? true,
      style: fields.style ?? null,
      negativePrompt: fields.negativePrompt ?? null,
      isDefault: fields.isDefault ?? false,
      clipSkip: fields.clipSkip ?? null,
      postProcessing: fields.postProcessing ?? [],
      hiresFix: fields.hiresFix ?? false,
      hiresFixDenoisingStrength: fields.hiresFixDenoisingStrength ?? null,
    },
  });
}

export async function updateImagePreset(
  id: string,
  fields: {
    name?: string;
    model?: string | null;
    samplerName?: string;
    cfgScale?: number;
    steps?: number;
    karras?: boolean;
    style?: string | null;
    negativePrompt?: string | null;
    /** 0 clears back to "no override" (valid range is 1-12). */
    clipSkip?: number;
    postProcessing?: string[];
    hiresFix?: boolean;
    hiresFixDenoisingStrength?: number;
  }
): Promise<ImagePreset> {
  return safeInvoke<ImagePreset>('update_image_preset', {
    id,
    fields: {
      name: fields.name ?? null,
      model: fields.model === undefined ? null : fields.model,
      samplerName: fields.samplerName ?? null,
      cfgScale: fields.cfgScale ?? null,
      steps: fields.steps ?? null,
      karras: fields.karras ?? null,
      style: fields.style === undefined ? null : fields.style,
      negativePrompt: fields.negativePrompt === undefined ? null : fields.negativePrompt,
      clipSkip: fields.clipSkip ?? null,
      postProcessing: fields.postProcessing ?? null,
      hiresFix: fields.hiresFix ?? null,
      hiresFixDenoisingStrength: fields.hiresFixDenoisingStrength ?? null,
    },
  });
}

export async function deleteImagePreset(id: string): Promise<void> {
  return safeInvoke<void>('delete_image_preset', { id });
}

export async function setDefaultImagePreset(id: string): Promise<void> {
  return safeInvoke<void>('set_default_image_preset', { id });
}

export async function setConversationImagePreset(conversationId: string, presetId: string | null): Promise<void> {
  return safeInvoke<void>('set_conversation_image_preset', { conversationId, presetId });
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

export async function updateLorebookEntry(
  id: string,
  name: string,
  keys: string[],
  content: string,
  alwaysActive: boolean,
  priority: number,
  insertionOrder: number,
): Promise<LorebookEntry> {
  return safeInvoke<LorebookEntry>('update_lorebook_entry', {
    id, name, keys, content, alwaysActive, priority, insertionOrder,
  });
}

/** Imports a character's embedded Character Card V2 lorebook (if any) as
 *  real, persisted entries — PNG import already does this automatically;
 *  this is for characters imported before that existed. Returns an empty
 *  array (not an error) if the character has no embedded lorebook. */
export async function importCharacterBookEntries(characterId: string): Promise<LorebookEntry[]> {
  return safeInvoke<LorebookEntry[]>('import_character_book_entries', { characterId });
}

/** Generates new lorebook entries for a character via the LLM ("Generate
 *  from Story") — grounded in the character's profile, known story facts,
 *  and recent dialogue, skipping facets already covered by existing
 *  entries. Newly generated entries are persisted immediately. */
export async function generateCharacterLorebook(characterId: string, conversationId: string): Promise<LorebookEntry[]> {
  return safeInvoke<LorebookEntry[]>('generate_character_lorebook', { characterId, conversationId });
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

// --- Trash ---

export type TrashItemType = 'conversation' | 'character' | 'persona';

export interface TrashItem {
  id: string;
  item_type: TrashItemType;
  name: string;
  avatar_path: string | null;
  deleted_at: string;
}

/** Lists everything currently in the Trash — conversations, characters, and personas — merged and sorted, most recently trashed first. */
export async function listTrash(): Promise<TrashItem[]> {
  return safeInvoke<TrashItem[]>('list_trash');
}

/** Permanently deletes every item currently in the Trash, across all three types. */
export async function emptyTrash(): Promise<void> {
  return safeInvoke<void>('empty_trash');
}
