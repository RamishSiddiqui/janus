# Providers

A **provider** is a stored configuration (`provider_configs`) for an external service or local program. Its **type** is `llm`, `image`, `video` or `tts`. Its **adapter** names the integration. Models a provider offers are tracked in `enabled_models`.

## Chat (LLM) providers

All chat adapters go through one type, `providers::unified::RigProvider`, built on [rig-core](https://github.com/0xPlaygrounds/rig). `RigProvider::from_config(adapter, config)` reads the stored adapter string and builds the right rig client (rig-core's model API is used directly: one request type and one event stream for every provider):

| Adapter string(s) | Backend |
|---|---|
| `openai`, `openai_compatible`, `open_ai_compatible`, `lm_studio`, `lmstudio` | OpenAI. With no base URL it speaks the **Responses API**; with a custom base URL (LM Studio, KoboldCPP, vLLM, Puter, gateways) it speaks Chat Completions, which is what those servers implement. |
| `anthropic`, `gemini`, `cohere` | The matching rig client (API key). |
| `deepseek` / `deep_seek`, `groq`, `perplexity`, `xai`, `hugging_face` / `huggingface`, `hyperbolic`, `moonshot`, `together` | Rig's OpenAI-shaped client on that vendor's dialect (API key; endpoint and quirks come from the dialect). |
| `open_router`, `openrouter` | OpenRouter. |
| `ollama` | Ollama at the configured base URL, on its native chat API. |

Adding another provider that rig supports is a few lines in `from_config` plus a UI option (the Providers page adapter list). The **Puter** preset in the UI is not a separate adapter: it pre-fills the OpenAI-compatible adapter with Puter's address.

`RigProvider` exposes three operations:

- `generate_stream(...)` sends `StreamChunk`s over a channel (`Delta`, `ReasoningDelta`, `Done`, `Error`). A silent stream is cut after 90 seconds.
- `generate(...)` returns the whole reply (non-streaming, and the stateless `generate_raw`).
- `generate_embedding(...)` returns vectors for retrieval.

Image attachments are passed as image parts; whether a model accepts them is a model capability the UI shows.

### Resolution (`providers/resolve.rs`)

`get_default_llm_provider` picks the account's default LLM provider; `resolve_model_id` picks the model (a chat's choice, else the provider's stored default, else an enabled model); `create_rig_provider` builds the client. Background work (summaries, memory extraction, NPC detection, embeddings) uses the same resolution, scoped to the conversation's owner. Embeddings resolve through the enabled **embedding** models instead of the chat provider's config.

### Model lists

`list_provider_models` and `list_all_models` query provider APIs. `toggle_model_enabled` records the choice in `enabled_models`. A model that disappears from a provider's list is auto-disabled and flagged "no longer listed".

## Image and video providers

Each adapter returns `(Vec<u8>, serde_json::Value)`: the raw media and a metadata object. The adapter does **not** write files or database rows; `commands/scenes.rs` does. Each takes a `cancel_flag: &Arc<AtomicBool>` and checks it each poll so the Stop button works (and so a remote job can be released).

| Adapter | File | Mechanism |
|---|---|---|
| AI Horde | `ai_horde.rs` | Submit an async job, poll every 4 s (up to 60 minutes), fetch the image. Anonymous key `0000000000` works. Model catalogue cached in `ai_horde_model_info`. Emits `ai_horde_progress`. |
| ComfyUI | `comfyui.rs` | The user's API-format workflow JSON with placeholder tokens replaced (`{{POSITIVE_PROMPT}}`, `{{SEED}}`, `{{CHARACTER_IMAGE_n}}`, ...); polls every 2 s up to 10 minutes. |
| WanGP | `wangp.rs` | WanGP only speaks MCP (`wgp.py --mcp --mcp-transport streamable-http`), so this adapter is an MCP client (the `rmcp` crate); polls every 2 s up to 30 minutes. Images and video. Emits `wangp_progress`. |
| Generic images | `commands/scenes.rs` | `generate_via_generic_provider` for OpenAI-images-compatible endpoints (SiliconFlow and others). |

To add an image or video adapter, copy `comfyui.rs` (HTTP) or `wangp.rs` (MCP), keep the return shape and cancel flag, and wire it into `generate_scene` / `generate_video_scene`. It will then share the single-flight guard and Stop button.

## Voice providers

Plain `reqwest` modules with no database access; the caller resolves the provider config and passes it in:

| Adapter | File | Notes |
|---|---|---|
| ElevenLabs | `elevenlabs.rs` | Default model `eleven_multilingual_v2`. |
| Google Cloud TTS | `google_cloud_tts.rs` | Uses a plain API key (`?key=`), not OAuth. |
| Fish Audio | `fish_audio.rs` | Default model `s2.1-pro`, paged voice list. |

The built-in Kokoro engine is not a provider; see {doc}`tts`.

## Connection tests

`test_provider_connection` returns `{ ok, detail }`, and `detail` always says why a test failed, because a bare "unreachable" hides very different causes (key, address, server, timeout).

## Keys

API keys live in the provider's `config` JSON in the local database. Commands that return providers to a browser should be treated as sensitive: the browser path never exposes the host-wide voice model commands, and only the owner's providers are returned.
