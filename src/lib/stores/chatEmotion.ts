// ============================================================
//   Janus — Chat Emotion Pipeline
//   Parses per-message emotional-state snapshots from stored metadata,
//   and runs the after-turn emotional-state update for every character
//   in the cast. Split out of chat.ts — used by both chatStream.ts
//   (ordinary single-speaker turns) and chatMultiChar.ts (multi-speaker
//   turns and solo NPC lines).
// ============================================================

import { get } from 'svelte/store';
import type { CharacterState } from '$lib/services/ipc';
import { activeCharacterId, characterEmotionStates, messages, pathState } from './chat';

/** Extracts the per-character emotional-state snapshot from a stored
 *  message's raw `metadata` JSON (`{"emotional_states": {charId: {...}}}`)
 *  — the frozen state each character was in when this message was created,
 *  as opposed to `characterEmotionStates`' always-current live map. */
export function parseEmotionSnapshot(metadata: unknown): Record<string, CharacterState> | undefined {
  const raw = (metadata as { emotional_states?: unknown } | null | undefined)?.emotional_states;
  if (!raw || typeof raw !== 'object') return undefined;
  const entries = Object.entries(raw as Record<string, unknown>).filter(
    ([, v]) => typeof v === 'object' && v !== null && 'dominant_emotion' in v
  ) as [string, CharacterState][];
  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
}

/** Runs the after-turn emotional-state update for every character in the
 *  cast — mirrors the existing "every character updates every turn" design
 *  (including background/auto-detected NPCs, not just whoever spoke) — then
 *  freezes the resulting snapshot onto the message(s) actually created this
 *  turn, so their EmotionHUD pills stay correct on reload/for other messages
 *  instead of drifting to whatever `characterEmotionStates` holds later.
 *  Fire-and-forget; never throws. Called from both the ordinary single-speaker
 *  "done" handler and the multi-char-response listener (multi-speaker turns
 *  and solo NPC lines, whose "done" event content is intentionally emptied). */
export async function runEmotionUpdatePipeline(
  conversationId: string,
  userMessage: string,
  assistantResponse: string,
  targetMessageIds: string[],
  // In-memory store ids to patch for an immediate (no-reload) pill update —
  // usually the same as targetMessageIds, but a multi-segment turn's live
  // bubbles still carry PresentationBuffer's synthetic `parentId__segN` ids
  // at this point (reconciled to the real backend ids only on next reload),
  // so the caller passes those in separately. Only targetMessageIds are ever
  // sent to the backend — synthetic ids aren't real message rows.
  displayMessageIds: string[] = targetMessageIds,
): Promise<void> {
  if (!assistantResponse || targetMessageIds.length === 0) return;
  try {
    const { updateEmotionalState } = await import('$lib/services/emotion-updater');
    const ipcMod = await import('$lib/services/ipc');

    // Collect all character IDs and names to update emotions for
    const charMap = new Map<string, string>(); // charId → charName
    const primaryCharId = get(activeCharacterId);

    // Add multi-char conversation characters (includes primary, and any
    // background/auto-detected NPCs like an auto-registered "Lena")
    try {
      const convChars = await ipcMod.listConversationCharacters(conversationId);
      for (const cc of convChars) {
        if (cc.character_id) {
          charMap.set(cc.character_id, cc.character_name || 'Character');
        }
      }
    } catch { /* single-char mode, no cast rows yet */ }

    // Ensure primary character is included even if listConversationCharacters is empty
    if (primaryCharId && !charMap.has(primaryCharId)) {
      charMap.set(primaryCharId, 'Character');
    }

    if (charMap.size === 0) return;

    // Run emotion updates in parallel for all characters, building a
    // snapshot of the resulting states as we go.
    const snapshot: Record<string, CharacterState> = {};
    await Promise.allSettled(Array.from(charMap.entries()).map(async ([charId, charName]) => {
      await updateEmotionalState(charId, conversationId, userMessage, assistantResponse, charName);
      const newState = await ipcMod.getCharacterState(charId, conversationId);
      if (newState) {
        characterEmotionStates.update(map => {
          const updated = new Map(map);
          updated.set(charId, newState);
          return updated;
        });
        snapshot[charId] = newState;
      }
    }));

    if (Object.keys(snapshot).length === 0) return;

    // Freeze onto the real message row(s) so it survives a reload, and patch
    // the in-memory store now so the pill(s) update immediately without one.
    await Promise.allSettled(
      targetMessageIds.map(id => ipcMod.setMessageEmotionalSnapshot(id, snapshot))
    );
    messages.update(msgs => msgs.map(m =>
      displayMessageIds.includes(m.id) ? { ...m, emotionSnapshot: snapshot } : m
    ));
    for (let i = 0; i < pathState.fullActivePath.length; i++) {
      if (displayMessageIds.includes(pathState.fullActivePath[i].id)) {
        pathState.fullActivePath[i] = { ...pathState.fullActivePath[i], emotionSnapshot: snapshot };
      }
    }
  } catch (err) {
    console.warn('[Janus] Emotion update failed:', err);
  }
}
