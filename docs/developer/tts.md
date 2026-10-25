# Text to speech

Janus has one built-in engine and three cloud adapters (see {doc}`providers`).

## Built-in Kokoro engine

`src-tauri/src/tts/` embeds [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M) in the app process using ONNX Runtime (`ort`, loaded dynamically). It is not an external server.

| File | Role |
|---|---|
| `download.rs` | Downloads the model, the combined voice pack and the ONNX Runtime library into `<app data>/tts/` on first use. Nothing is bundled in the installer. Emits `tts-download-progress`. |
| `engine.rs` | `KokoroEngine`: loads the ONNX session and voice pack, and synthesizes (24 kHz). Building it takes real time (graph parsing, phoneme engine), so it is created once and cached in `AppState.tts_engine`, never per call. |
| `vocab.rs` | The phoneme-to-token table copied from the model's own config. Phonemes come from `misaki-rs`. |
| `chunker.rs` | Splits incrementally arriving text into complete sentences (minimum 8 characters) so synthesis can start before the reply finishes. |

Files used: `kokoro-v1.0.int8.onnx` (the `model_q8f16` weights, about 86 MB), `voices-v1.0.bin` (all 54 voices), and the ONNX Runtime library. Its filename carries the version (`onnxruntime-1.31.0.dll`), because each `ort` release needs one specific ONNX Runtime: after an `ort` upgrade the old library is not loadable, the new name makes the voice count as not downloaded, and the next download fetches only the runtime (the model and voices are kept). Change `RUNTIME_VERSION`, the URL and the zip path in `download.rs` together with the `ort` dependency.

```{note}
The runtime download currently points at the Windows x64 build of ONNX Runtime, so the built-in engine works on Windows only. Supporting macOS and Linux means fetching the matching library per platform.
```

### Streaming synthesis

In `commands/chat/streaming.rs`, when the speaker has a Kokoro voice:

1. each `Delta` is added to a sentence buffer;
2. each complete sentence is synthesized and emitted as a `tts-chunk` event with a sequence number;
3. at `Done`, the remainder is flushed and `tts-stream-end` is emitted.

The frontend's `ttsPlayback.ts` queues chunks by sequence and plays them in order. When no voice is assigned the whole block is skipped, adding nothing to the streaming path. If the speaker's voice belongs to a cloud provider (`voice_provider_id` is set), live streaming is skipped too; the message saves normally and the voice is available through `tts_replay_message`, which makes one whole-message call.

### Commands

`tts_model_status`, `tts_download_model`, `tts_preload_engine` (warm the engine at startup when voice is on), `tts_list_voices`, `tts_set_character_voice`, `tts_test_speak` (preview), `tts_replay_message`. The model and engine commands are host-global and are not exposed to browsers. Voice listing, assignment and preview are. `tts_replay_message` is not exposed to browsers yet (its audio events are not wired to a requesting browser's stream), so replay works on the desktop.

## Voice assignment

A character has `voice_id` and `voice_provider_id`. A `None` provider means Kokoro. A character with no `voice_id` stays silent unless a default voice is set in settings (`ttsDefaultVoiceId`, `ttsDefaultProviderId`), because a wrong voice is worse than none.

## Resource use

Kokoro runs at roughly half real time on a CPU. `get_resource_usage` reports Janus's own memory and CPU so the Settings page can show whether voice is behaving.
