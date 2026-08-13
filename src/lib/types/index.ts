// ============================================================
//   Janus — Shared Type Definitions
// ============================================================

/** A single chat message (user or AI). */
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  parent_id?: string | null;
  alternates?: number;
  currentAlternate?: number;
  siblingIds?: string[];
  siblingIndex?: number;
  /** For cross-conversation branch navigation: list of conversation IDs that forked at this message. */
  siblingConversationIds?: string[];
  /** Index of the current conversation in siblingConversationIds. */
  siblingConversationIndex?: number;
  isStreaming?: boolean;
  /** True when the AI generation failed — UI should show a retry button. */
  isError?: boolean;
  /** Character attribution for multi-character conversations. */
  character_id?: string | null;
  character_name?: string | null;
  /** Character avatar URL (resolved at render time from character data). */
  character_avatar_url?: string | null;
  /** Chain-of-thought/reasoning trace from a reasoning model, kept separate
   *  from `content` — rendered as a collapsible "Thinking" section. */
  reasoning?: string | null;
  /** True while reasoning is actively streaming and no real reply content has
   *  arrived yet — live-only, never persisted. */
  isThinking?: boolean;
  /** Wall-clock ms when reasoning started streaming — live-only, used to
   *  compute "Thought for Ns" once real content begins. */
  thinkingStartedAt?: number;
  /** Images attached to this message (user messages only) — sent as real
   *  multimodal input to vision-capable models. Present immediately on the
   *  optimistic local message when just sent, and reloaded from the
   *  backend's `metadata.attachments` on history load. */
  attachments?: { relativePath: string; mimeType: string }[];
}

/** A character card used across Gallery and Chat. */
export interface Character {
  id: string;
  name: string;
  description: string;
  tags: CharacterTag[];
  avatarGradient: string;
  isFavorite: boolean;
  systemPrompt?: string;
}

/** A tag attached to a character. */
export interface CharacterTag {
  label: string;
  color: string;
  bg: string;
}

/** A sidebar navigation item. */
export interface NavItem {
  readonly path: string;
  readonly label: string;
  readonly icon: string;
  readonly group?: string;
}

/** A conversation preview shown in the sidebar. */
export interface ConversationPreview {
  id: string;
  characterId: string | null;
  characterName: string;
  avatarColor: string;
  avatarUrl: string | null;
  preview: string;
  time: string;
  /** If this conversation was branched from another, the parent's ID. */
  parentConversationId?: string | null;
  /** The message ID in the PARENT conversation where the fork happened. */
  branchPointMessageId?: string | null;
  /** Additional characters in a multi-character conversation (future support) */
  additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
}

/** An AI provider configuration (chat, image, or video). */
export interface ProviderConfig {
  name: string;
  model: string;
  apiKey?: string;
  isActive: boolean;
  isConnected: boolean;
  url?: string;
}

/** Lorebook entry. */
export interface LorebookEntry {
  id: string;
  title: string;
  keys: string[];
  content: string;
  isActive: boolean;
  alwaysActive: boolean;
  priority: number;
  insertionOrder: number;
  /** True for an entry parsed from a character card's embedded lorebook
   *  that hasn't been imported as a real, persisted entry yet — its `id`
   *  isn't a real lorebook_entries record, so edit/delete/toggle/reorder
   *  don't apply until it's imported. */
  isVirtual?: boolean;
}

/** Memory entry auto-saved from conversations. */
export interface MemoryEntry {
  id: string;
  text: string;
  meta: string;
}
