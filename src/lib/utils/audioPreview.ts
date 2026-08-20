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

function getPreviewContext(): AudioContext {
  if (!previewContext) {
    previewContext = new AudioContext();
  }
  if (previewContext.state === 'suspended') {
    // Browsers require a user-gesture to start/resume an AudioContext —
    // safe to call unconditionally since this is only ever invoked from a
    // click handler.
    void previewContext.resume();
  }
  return previewContext;
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

/** Decodes and immediately plays a base64 WAV clip. Returns once playback
 *  has been scheduled (not once it finishes). */
export async function playBase64Wav(base64: string): Promise<void> {
  const ctx = getPreviewContext();
  const buffer = await decodeBase64Wav(base64, ctx);
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(ctx.destination);
  source.start();
}
