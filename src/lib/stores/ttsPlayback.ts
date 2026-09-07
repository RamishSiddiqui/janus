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
import type { TtsChunkEvent, TtsStreamEndEvent } from '$lib/services/ipc';

const isTauri = browser && '__TAURI_INTERNALS__' in window;

/** True while a scheduled chunk is still playing (or queued) — drives the
 *  speaker/mute indicator in the title bar. */
export const isSpeaking = writable(false);

/** The message_id whose chunks are currently queued/playing, or `null` when
 *  idle — mirrors the internal `currentMessageId` so per-message UI (the
 *  "Play voice" button on each `ChatMessage`) can show its own playing
 *  state accurately instead of relying on the blanket `isSpeaking` flag,
 *  which doesn't say *which* message is sounding. */
export const currentlyPlayingMessageId = writable<string | null>(null);

/** The sentence actually audible *right now* (not just scheduled) — drives
 *  the sentence-level "glow while speaking" highlight in ChatMessage. Only
 *  sentence-level, not word-level: Kokoro's raw audio output carries no
 *  per-word timing to sync against. `null` when nothing's playing. */
export const currentlyPlayingSentence = writable<{ messageId: string; text: string } | null>(null);

let audioContext: AudioContext | null = null;
let unlistenTtsChunk: (() => void) | null = null;
let unlistenTtsStreamEnd: (() => void) | null = null;

// Playback-queue state, reset whenever a new message's chunks start
// arriving (sequence === 0 for a message_id we haven't seen this queue).
let currentMessageId: string | null = null;
let nextExpectedSequence = 0;
let nextStartTime = 0;
let pendingSources: AudioBufferSourceNode[] = [];
let activeSourceCount = 0;
// Highest sequence number whose "now actually playing" activation has
// fired — lets a chunk's delayed callbacks (the setTimeout that flips the
// glow on, the onended that might flip it off) tell whether a *later*
// chunk has already taken over before touching the store, since chunks
// chain gaplessly and their timers don't fire in a strictly predictable
// order relative to each other.
let latestActivatedSequence = -1;
// Set once `tts-stream-end` arrives for the message currently tracked —
// on its own this does NOT mean it's safe to clear state (audio already
// scheduled can still be playing); combined with activeSourceCount === 0
// (checked both here and in each source's onended) it is.
let streamEnded = false;

let sharedAnalyser: AnalyserNode | null = null;

function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
    // One analyser shared by every chunk's source in this queue (not one
    // per chunk, like the standalone preview path uses) — sources connect
    // through it on their way to the destination, so a waveform
    // visualizer watching it reflects whatever's actually playing right
    // now, chunk after chunk, without needing to be re-pointed at a new
    // node each time.
    sharedAnalyser = audioContext.createAnalyser();
    sharedAnalyser.fftSize = 64;
    sharedAnalyser.connect(audioContext.destination);
  }
  if (audioContext.state === 'suspended') {
    void audioContext.resume();
  }
  return audioContext;
}

/** The shared analyser node for this queue's playback, or `null` before
 *  any chunk has ever played (the AudioContext — and this — are created
 *  lazily on first use/prime). Feed straight to a `WaveformBars`. */
export function getTtsPlaybackAnalyser(): AnalyserNode | null {
  return sharedAnalyser;
}

/** Creates (once) and resumes the shared playback AudioContext. Must be
 *  called synchronously from a direct user click handler (e.g. the very
 *  first line of `sendMessage`/`retryLastMessage`/`regenerateMessage`,
 *  before any `await`) — a `tts-chunk` event arrives seconds later, fully
 *  detached from any click by the time it fires, so priming lazily inside
 *  `handleChunk` (the previous approach) let Chromium's autoplay policy
 *  keep the context silently suspended: chunks would schedule and
 *  "complete" with no error, but produce no sound, and — unlike the
 *  preview button — there's no natural second click to unstick it. Exactly
 *  the same bug already found and fixed for `audioPreview.ts`'s preview
 *  buttons, applied here for the live-chat streaming path. Safe/cheap to
 *  call on every send — a no-op once the context is already running. */
export function primeTtsAudioContext(): void {
  if (!isTauri || !get(settings).ttsEnabled) return;
  getAudioContext();
}

function resetQueue(messageId: string) {
  // Stop anything still scheduled from a previous message before starting
  // the new one's queue — otherwise a stale tail could overlap the new
  // message's first chunk.
  stopInternal();
  currentMessageId = messageId;
  currentlyPlayingMessageId.set(messageId);
  nextExpectedSequence = 0;
  nextStartTime = 0;
  latestActivatedSequence = -1;
  streamEnded = false;
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
  currentlyPlayingMessageId.set(null);
  currentlyPlayingSentence.set(null);
  nextExpectedSequence = 0;
  nextStartTime = 0;
  latestActivatedSequence = -1;
  streamEnded = false;
  isSpeaking.set(false);
}

/** `tts-stream-end` handler — records that no more chunks are coming for
 *  this message, and finishes immediately if playback has already caught
 *  up (nothing left scheduled). Otherwise the flag just sits here until
 *  the last source's `onended` checks it. */
function handleStreamEnd(event: TtsStreamEndEvent) {
  if (event.message_id !== currentMessageId) return;
  streamEnded = true;
  if (activeSourceCount <= 0) {
    stopInternal();
  }
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
  // Through the shared analyser (already wired to destination in
  // getAudioContext), not straight to destination — that analyser is what
  // lets a WaveformBars visualize whatever chunk is actually playing.
  source.connect(sharedAnalyser ?? ctx.destination);
  pendingSources.push(source);
  activeSourceCount++;
  isSpeaking.set(true);

  // The glow should track when audio is actually *audible*, not when a
  // chunk merely arrived (chunks are often received and scheduled well
  // ahead of their real playback time). Fire the highlight at the real
  // moment this chunk's start() takes effect, and clear it at the moment
  // it ends — both delayed relative to "now" by ctx's own clock, which is
  // why these are separate setTimeouts keyed off the schedule rather than
  // done inline here.
  const messageId = chunk.message_id;
  const sequence = chunk.sequence;
  const text = chunk.text;
  const activateInMs = Math.max(0, (startTime - ctx.currentTime) * 1000);
  setTimeout(() => {
    if (currentMessageId !== messageId) return; // Superseded by a newer message entirely.
    latestActivatedSequence = Math.max(latestActivatedSequence, sequence);
    if (latestActivatedSequence === sequence) {
      currentlyPlayingSentence.set({ messageId, text });
    }
  }, activateInMs);

  source.onended = () => {
    activeSourceCount--;
    pendingSources = pendingSources.filter((s) => s !== source);
    if (currentMessageId !== messageId) return; // A newer message has already taken over entirely.
    if (activeSourceCount <= 0 && streamEnded) {
      // Nothing left playing/scheduled, and we know nothing more is
      // coming — this message is genuinely done. Clears isSpeaking,
      // currentlyPlayingMessageId, and currentlyPlayingSentence together
      // (see stopInternal) rather than leaving currentlyPlayingMessageId
      // dangling forever, which is what left the "Play voice" button
      // stuck on "Playing…" indefinitely before this fix.
      stopInternal();
      return;
    }
    if (activeSourceCount <= 0) {
      isSpeaking.set(false);
    }
    // Only clear the glow if nothing newer has already taken over (a
    // gaplessly-chained next chunk's activation timeout may well have
    // already fired before this one's onended does).
    if (latestActivatedSequence === sequence) {
      currentlyPlayingSentence.set(null);
    }
  };
  source.start(startTime);
  nextStartTime = startTime + buffer.duration;
}

/** Registers the global `tts-chunk`/`tts-stream-end` listeners. Call once
 *  at app startup (see +layout.svelte). Safe to call more than once —
 *  subsequent calls are no-ops while listeners are already active. */
export async function initTtsPlaybackListener(): Promise<() => void> {
  if (unlistenTtsChunk) return unlistenTtsChunk;
  if (!isTauri) return () => {};

  const { listen } = await import('@tauri-apps/api/event');
  unlistenTtsChunk = await listen<TtsChunkEvent>('tts-chunk', (event) => {
    void handleChunk(event.payload);
  });
  unlistenTtsStreamEnd = await listen<TtsStreamEndEvent>('tts-stream-end', (event) => {
    handleStreamEnd(event.payload);
  });
  return unlistenTtsChunk;
}

/** Tears down the global listeners and stops any in-flight playback. */
export function cleanupTtsPlaybackListener() {
  if (unlistenTtsChunk) {
    unlistenTtsChunk();
    unlistenTtsChunk = null;
  }
  if (unlistenTtsStreamEnd) {
    unlistenTtsStreamEnd();
    unlistenTtsStreamEnd = null;
  }
  stopInternal();
}
