// ============================================================
//   Janus — One-off Audio Preview Helper
//   Decodes a base64 WAV (as returned by ipc.ttsTestSpeak) and plays it
//   immediately via a standalone AudioBufferSourceNode. Not part of the
//   streamed-conversation playback queue (see stores/ttsPlayback.ts) —
//   this is for "preview this voice" buttons in Settings and the
//   character editor, where there's exactly one clip and no ordering to
//   preserve.
// ============================================================

let previewContext: AudioContext | null = null;

/** Creates (once) and resumes the shared preview AudioContext. Must be
 *  called synchronously, directly inside a click handler, before any
 *  `await` — Chromium's autoplay policy only reliably honors
 *  `resume()` as tied to a user gesture while it's still in that same
 *  synchronous call stack. Calling this only after an `await ipc.foo()`
 *  (as the previous version did, lazily inside `playBase64Wav`) lets the
 *  context silently stay "suspended": playback schedules and completes
 *  with no error, but produces no sound, and only a *second* click
 *  (closer to a still-live user-activation window) actually unlocks it —
 *  exactly the "first click does nothing, second click plays" bug this
 *  fixes. */
export function primeAudioPreviewContext(): void {
  if (!previewContext) {
    previewContext = new AudioContext();
  }
  if (previewContext.state === 'suspended') {
    void previewContext.resume();
  }
}

function getPreviewContext(): AudioContext {
  primeAudioPreviewContext();
  return previewContext!;
}

/** Decodes a base64-encoded WAV string into an AudioBuffer. */
export async function decodeBase64Wav(base64: string, ctx: AudioContext): Promise<AudioBuffer> {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return ctx.decodeAudioData(bytes.buffer);
}

export interface WavPlayback {
  /** Live amplitude data for a level-meter/waveform visualizer — read via
   *  `analyser.getByteFrequencyData()` on a rAF loop while `ended` is
   *  still pending. */
  analyser: AnalyserNode;
  /** Resolves once the clip finishes playing. */
  ended: Promise<void>;
}

/** Decodes and immediately plays a base64 WAV clip, routed through an
 *  AnalyserNode so callers can drive a waveform visualizer during
 *  playback. Returns once playback has been scheduled (not once it
 *  finishes) — await `.ended` for that. */
export async function playBase64Wav(base64: string): Promise<WavPlayback> {
  const ctx = getPreviewContext();
  // Belt-and-suspenders: by now `primeAudioPreviewContext()` should already
  // have resolved this (called synchronously before the IPC round-trip
  // that got us here), but await it explicitly rather than assume timing.
  if (ctx.state === 'suspended') {
    await ctx.resume();
  }
  const buffer = await decodeBase64Wav(base64, ctx);
  const source = ctx.createBufferSource();
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 64;
  source.buffer = buffer;
  source.connect(analyser);
  analyser.connect(ctx.destination);
  const ended = new Promise<void>((resolve) => {
    source.onended = () => resolve();
  });
  source.start();
  return { analyser, ended };
}
