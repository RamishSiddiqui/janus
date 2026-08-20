// ============================================================
//   Janus — Streamed TTS Playback Queue
//   Subscribes to `tts-chunk` events (one per synthesized sentence during
//   a live chat response) and plays them back gaplessly via the Web Audio
//   API. There is no other audio code anywhere in this codebase — this is
//   genuinely new frontend surface, not an extension of an existing
//   pattern (unlike image/scene handling, which reuses blob URLs).
//
//   Registered once at app root (see +layout.svelte), mirroring
//   chatMultiChar.ts's initMultiCharListener/cleanupMultiCharListener
//   shape — a single global listener filtered by the active conversation,
//   not a per-conversation attach/detach the caller has to manage.
// ============================================================

import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { activeConversationId } from './chat';
import { settings } from './settings';
import { decodeBase64Wav } from '$lib/utils/audioPreview';
import type { TtsChunkEvent } from '$lib/services/ipc';

const isTauri = browser && '__TAURI_INTERNALS__' in window;

/** True while a scheduled chunk is still playing (or queued) — drives the
 *  speaker/mute indicator in the title bar. */
export const isSpeaking = writable(false);

let audioContext: AudioContext | null = null;
let unlistenTtsChunk: (() => void) | null = null;

// Playback-queue state, reset whenever a new message's chunks start
// arriving (sequence === 0 for a message_id we haven't seen this queue).
let currentMessageId: string | null = null;
let nextExpectedSequence = 0;
let nextStartTime = 0;
let pendingSources: AudioBufferSourceNode[] = [];
let activeSourceCount = 0;

function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
  }
  if (audioContext.state === 'suspended') {
    void audioContext.resume();
  }
  return audioContext;
}

function resetQueue(messageId: string) {
  // Stop anything still scheduled from a previous message before starting
  // the new one's queue — otherwise a stale tail could overlap the new
  // message's first chunk.
  stopInternal();
  currentMessageId = messageId;
  nextExpectedSequence = 0;
  nextStartTime = 0;
}

function stopInternal() {
  for (const source of pendingSources) {
    try {
      source.stop();
    } catch {
      // Already finished/stopped — fine to ignore.
    }
  }
  pendingSources = [];
  activeSourceCount = 0;
  currentMessageId = null;
  nextExpectedSequence = 0;
  nextStartTime = 0;
  isSpeaking.set(false);
}

/** Stops all current/queued playback immediately (mute/stop control). */
export function stop() {
  stopInternal();
}

async function handleChunk(chunk: TtsChunkEvent) {
  if (!get(settings).ttsEnabled) return;
  if (chunk.conversation_id !== get(activeConversationId)) return;

  if (chunk.sequence === 0 && chunk.message_id !== currentMessageId) {
    resetQueue(chunk.message_id);
  } else if (chunk.message_id !== currentMessageId) {
    // A chunk for a message we're not tracking (e.g. arrived after the
    // queue was reset/stopped, or out of order from a message we already
    // moved past) — drop rather than corrupt playback order.
    console.warn(`[tts] Dropping stray chunk for message ${chunk.message_id} (tracking ${currentMessageId})`);
    return;
  }
  if (chunk.sequence < nextExpectedSequence) {
    console.warn(`[tts] Dropping late chunk ${chunk.sequence} (already scheduled through ${nextExpectedSequence - 1})`);
    return;
  }
  nextExpectedSequence = chunk.sequence + 1;

  const ctx = getAudioContext();
  let buffer: AudioBuffer;
  try {
    buffer = await decodeBase64Wav(chunk.audio, ctx);
  } catch (err) {
    console.warn('[tts] Failed to decode audio chunk (non-fatal):', err);
    return;
  }

  // Gapless scheduling: chain each chunk's start time off the previous
  // chunk's scheduled end, never earlier than "now" — explicit timestamps
  // avoid the audible gaps naive back-to-back .start() calls risk under
  // JS event-loop jitter.
  const startTime = Math.max(nextStartTime, ctx.currentTime);
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(ctx.destination);
  pendingSources.push(source);
  activeSourceCount++;
  isSpeaking.set(true);
  source.onended = () => {
    activeSourceCount--;
    pendingSources = pendingSources.filter((s) => s !== source);
    if (activeSourceCount <= 0) {
      isSpeaking.set(false);
    }
  };
  source.start(startTime);
  nextStartTime = startTime + buffer.duration;
}

/** Registers the global `tts-chunk` listener. Call once at app startup
 *  (see +layout.svelte). Safe to call more than once — subsequent calls
 *  are no-ops while a listener is already active. */
export async function initTtsPlaybackListener(): Promise<() => void> {
  if (unlistenTtsChunk) return unlistenTtsChunk;
  if (!isTauri) return () => {};

  const { listen } = await import('@tauri-apps/api/event');
  unlistenTtsChunk = await listen<TtsChunkEvent>('tts-chunk', (event) => {
    void handleChunk(event.payload);
  });
  return unlistenTtsChunk;
}

/** Tears down the global listener and stops any in-flight playback. */
export function cleanupTtsPlaybackListener() {
  if (unlistenTtsChunk) {
    unlistenTtsChunk();
    unlistenTtsChunk = null;
  }
  stopInternal();
}
