// ============================================================
//   Janus — Scene Generation Store
//   Tracks in-flight AI Horde scene generations keyed by
//   conversation_id, independent of any component's lifecycle.
//
//   Local component state (previously used by SceneDisplay) didn't
//   survive switching chats and back, and wasn't visible to any other
//   component (e.g. the scene gallery) that wanted to show the same
//   "generating..." status. Keeping it here instead means it's immune
//   to remounts and shared by every consumer.
// ============================================================

import { writable } from 'svelte/store';
import { info as infoToast } from '$lib/stores/toast';

export interface AiHordeProgress {
  phase: 'queued' | 'waiting' | 'processing' | 'finalizing';
  queue_position?: number | null;
  wait_time?: number | null;
  is_possible?: boolean;
  kudos?: number | null;
}

/** Shape of the `wangp_progress` Tauri event — a numeric step/percentage
 *  progress model, unlike AI Horde's queue-position-based one. Distinguished
 *  from `AiHordeProgress` at the type level by `current_step` (present here,
 *  absent there) — see `describeProgress`'s branch on it. */
export interface WanGpProgress {
  phase?: string | null;
  status?: string | null;
  progress?: number | null;
  current_step?: number | null;
  total_steps?: number | null;
}

export interface SceneGenerationState {
  isLoading: boolean;
  progress: AiHordeProgress | WanGpProgress | null;
  /** Timestamp of the most recent completion (success or failure) for this
   *  conversation — consumers watch for this changing rather than diffing
   *  isLoading, so they can react even if they weren't mounted when it flipped. */
  completedAt: number | null;
}

const EMPTY_STATE: SceneGenerationState = { isLoading: false, progress: null, completedAt: null };

export const sceneGenerations = writable<Record<string, SceneGenerationState>>({});

export function getSceneGenerationState(
  all: Record<string, SceneGenerationState>,
  conversationId: string | null | undefined
): SceneGenerationState {
  if (!conversationId) return EMPTY_STATE;
  return all[conversationId] ?? EMPTY_STATE;
}

function patchState(convId: string, patch: Partial<SceneGenerationState>) {
  sceneGenerations.update(s => ({
    ...s,
    [convId]: { ...EMPTY_STATE, ...s[convId], ...patch },
  }));
}

/** The exact key format `generate_npc_portrait` passes as its AI Horde
 *  `conversation_id` parameter (see the matching comment in
 *  `commands::npc::generate_npc_portrait`) — namespaced so it never
 *  collides with a real conversation's own scene-generation progress.
 *  NPC-only — `generate_persona_portrait` uses a different key scheme, see
 *  `personaPortraitGenerationKey` below. (This function used to claim, in
 *  its own doc comment, to also match personas — it never did; nothing
 *  wired a persona portrait through this store at all until that was fixed.) */
export function portraitGenerationKey(characterId: string): string {
  return `npc-portrait-${characterId}`;
}

/** The exact key format `generate_persona_portrait` passes as its AI Horde
 *  `conversation_id` parameter (see `commands::personas::generate_persona_portrait`). */
export function personaPortraitGenerationKey(personaId: string): string {
  return `persona-portrait-${personaId}`;
}

let listenerReady: Promise<void> | null = null;

/** Lazily wires the single global `ai_horde_progress` listener the first
 *  time any generation is kicked off — safe to call repeatedly, registers
 *  exactly once for the whole app's lifetime. */
function ensureProgressListener(): Promise<void> {
  if (!listenerReady) {
    listenerReady = import('@tauri-apps/api/event').then(({ listen }) => {
      listen<{ conversation_id: string } & AiHordeProgress>('ai_horde_progress', (event) => {
        const { conversation_id, ...progress } = event.payload;
        patchState(conversation_id, { isLoading: true, progress });
      });
      listen<{ conversation_id: string } & WanGpProgress>('wangp_progress', (event) => {
        const { conversation_id, ...progress } = event.payload;
        patchState(conversation_id, { isLoading: true, progress });
      });
    });
  }
  return listenerReady;
}

export function formatWait(seconds: number): string {
  if (seconds <= 0) return 'any moment now';
  if (seconds < 60) return `~${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return s > 0 ? `~${m}m ${s}s` : `~${m}m`;
}

/** Human-readable label for the current phase of an in-flight generation —
 *  shared between the Scene panel and the scene gallery placeholder so both
 *  describe the same state identically. */
export function describeProgress(progressIn: AiHordeProgress | WanGpProgress | null): string {
  if (!progressIn) return 'Generating...';
  if ('current_step' in progressIn) return describeWanGpProgress(progressIn);
  const progress = progressIn as AiHordeProgress;
  switch (progress.phase) {
    case 'queued':
      return 'Submitted to AI Horde — queuing…';
    case 'waiting': {
      const pos = progress.queue_position;
      const wait = progress.wait_time;
      if (pos != null && pos > 0) {
        return wait != null ? `Queued (#${pos}) — ${formatWait(wait)}` : `Queued — position ${pos}`;
      }
      return wait != null ? `Starting soon — ${formatWait(wait)}` : 'Waiting for a worker…';
    }
    case 'processing':
      return 'A worker is painting your scene…';
    case 'finalizing':
      return 'Finalizing image…';
    default:
      return 'Generating...';
  }
}

/** WanGp's own progress shape is step/percentage-based rather than
 *  queue-position-based — formats e.g. "Step 4/8 — Denoising (54%)". */
function describeWanGpProgress(progress: WanGpProgress): string {
  const parts: string[] = [];
  if (progress.current_step != null && progress.total_steps != null) {
    parts.push(`Step ${progress.current_step}/${progress.total_steps}`);
  }
  if (progress.status) parts.push(progress.status);
  if (progress.progress != null) parts.push(`${Math.round(progress.progress)}%`);
  return parts.length > 0 ? parts.join(' — ') : 'Generating...';
}

/** Shows a one-time-per-generation heads-up when a provider had to guess a
 *  model because nothing was pinned as the default (AI Horde's live-roster
 *  auto-pick — see `pick_default_model` in scenes.rs) — otherwise this is
 *  invisible in the UI and only shows up as a backend log line, which is
 *  exactly what made "is X actually set as default or not?" so confusing. */
function warnIfModelWasAutoSelected(metadata: unknown): void {
  const meta = metadata as { model_was_auto_selected?: boolean; model?: string } | null;
  if (!meta?.model_was_auto_selected) return;
  const model = meta.model ? `"${meta.model}"` : 'a model';
  infoToast(`No default model set — auto-selected ${model} for this generation. Set one in Settings → Providers.`);
}

/** Runs a scene generation for `conversationId`, tracking its live status in
 *  `sceneGenerations`. Any mounted component can read that conversation's
 *  state regardless of which component (or component instance) actually
 *  called this — switching chats and back, or the context panel remounting,
 *  doesn't lose the in-flight status or drop progress events. */
export async function runSceneGeneration(
  conversationId: string,
  prompt: string,
  options?: {
    messageId?: string; negativePrompt?: string; width?: number; height?: number;
    modelOverride?: string; referenceImagePath?: string; denoisingStrength?: number;
    allowNsfw?: boolean;
    characterImages?: { characterId: string; characterName: string; relativePath: string }[];
  }
) {
  // Set isLoading synchronously, BEFORE awaiting the listener setup — the
  // await below yields to the event loop, and on the very first call
  // ensureProgressListener does real async work (dynamic import + listener
  // registration). A second call for the same key landing inside that
  // window used to see isLoading still false, pass the "already in
  // progress" guard, and then its own failure handler would stomp the
  // first (genuinely still-running) call's state back to isLoading:false.
  patchState(conversationId, { isLoading: true, progress: null });
  await ensureProgressListener();
  try {
    const ipc = await import('$lib/services/ipc');
    const scene = await ipc.generateScene(conversationId, prompt, options);
    patchState(conversationId, { isLoading: false, progress: null, completedAt: Date.now() });
    warnIfModelWasAutoSelected(scene.metadata);
    return scene;
  } catch (err) {
    patchState(conversationId, { isLoading: false, progress: null, completedAt: Date.now() });
    throw err;
  }
}

/** Same tracking as `runSceneGeneration`, for video scenes — see
 *  `generateVideoScene`. WanGP is currently the only adapter that can
 *  actually produce a video scene, but this stays adapter-agnostic like the
 *  image path. */
export async function runVideoSceneGeneration(
  conversationId: string,
  prompt: string,
  options?: {
    messageId?: string; negativePrompt?: string; width?: number; height?: number;
    durationSeconds?: number; fps?: number; modelOverride?: string; allowNsfw?: boolean;
    characterImages?: { characterId: string; characterName: string; relativePath: string }[];
  }
) {
  patchState(conversationId, { isLoading: true, progress: null });
  await ensureProgressListener();
  try {
    const ipc = await import('$lib/services/ipc');
    const scene = await ipc.generateVideoScene(conversationId, prompt, options);
    patchState(conversationId, { isLoading: false, progress: null, completedAt: Date.now() });
    return scene;
  } catch (err) {
    patchState(conversationId, { isLoading: false, progress: null, completedAt: Date.now() });
    throw err;
  }
}

/** Same live-progress tracking as `runSceneGeneration`, generalized over any
 *  async call that goes through `generate_via_ai_horde` under a
 *  `portraitGenerationKey(characterId)`-keyed job (currently
 *  `generate_npc_portrait`) — those emit the exact same `ai_horde_progress`
 *  event, just keyed by that string instead of a real conversation id, so
 *  the same store/listener/UI helpers (`getSceneGenerationState`,
 *  `describeProgress`, `formatWait`) work unchanged for portrait generation
 *  too. */
export async function trackPortraitGeneration<T>(characterId: string, run: () => Promise<T>): Promise<T> {
  return trackGenerationByKey(portraitGenerationKey(characterId), run);
}

/** Same tracking as `trackPortraitGeneration`, for persona portraits — see
 *  `personaPortraitGenerationKey` for why this can't just reuse the NPC
 *  version's key format. */
export async function trackPersonaPortraitGeneration<T>(personaId: string, run: () => Promise<T>): Promise<T> {
  return trackGenerationByKey(personaPortraitGenerationKey(personaId), run);
}

/** Fully generic remount-safe busy-tracking for ANY async operation, keyed
 *  by an arbitrary string — not just AI Horde jobs. `ensureProgressListener`
 *  is harmless to call even when `key` will never actually receive an
 *  `ai_horde_progress` event (e.g. an LLM call, a local file copy): it's
 *  idempotent and just guarantees the listener is registered once for the
 *  app's lifetime. Use this instead of a component-local `$state` busy flag
 *  for anything that should still show "in progress" if the component
 *  unmounts and remounts mid-operation (switching explorer tabs, closing
 *  and reopening a popover, etc) — a local flag silently resets to its
 *  initial value on remount even though the operation keeps running
 *  server-side. */
export async function trackGenerationByKey<T>(key: string, run: () => Promise<T>): Promise<T> {
  // See the matching comment in `runSceneGeneration` — set isLoading before
  // the listener-setup await, not after, to close the same TOCTOU gap.
  patchState(key, { isLoading: true, progress: null });
  await ensureProgressListener();
  try {
    const result = await run();
    patchState(key, { isLoading: false, progress: null, completedAt: Date.now() });
    return result;
  } catch (err) {
    patchState(key, { isLoading: false, progress: null, completedAt: Date.now() });
    throw err;
  }
}
