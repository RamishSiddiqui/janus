# Rig Integration — Universal Provider Layer

**Date:** 2026-05-19  
**Status:** Approved  
**Branch:** `feat/rig-integration`

## Overview

Replace Mythic's hand-rolled LLM provider layer (~620 lines across 4 files) with `rig-core` + `rig-dyn`, creating a universal provider system where adding a new LLM provider requires zero Rust code — just a DB config row.

Image generation providers that rig supports natively (OpenAI DALL-E) use rig's `ImageGenerationModel` trait. Providers rig doesn't cover (SiliconFlow, ComfyUI) get thin adapter structs implementing the same rig trait. Video generation stays as a custom trait (rig has no video abstraction).

## Architecture

### LLM Provider Resolution

```
DB row: { adapter: "anthropic", api_key: "sk-..." }
         ↓
  rig_dyn::Provider::try_from("anthropic")
         ↓
  provider.client(&api_key, base_url)
         ↓
  client.completion_model(model_id)
         ↓
  model.stream_chat(...) → mpsc channel → Tauri events
```

For OpenAI-compatible custom endpoints:
```
DB row: { adapter: "openai", api_key: "...", base_url: "http://localhost:1234/v1" }
         ↓
  rig::providers::openai::Client::new(&api_key).with_base_url(base_url)
         ↓
  Same flow as above
```

### Image Provider Resolution

```
DB row: { adapter: "openai", api_key: "sk-..." }
         ↓
  rig::providers::openai → ImageGenerationModel (native)

DB row: { adapter: "siliconflow", api_key: "..." }
         ↓
  SiliconFlowImageAdapter → implements rig's ImageGenerationModel
```

### Provider Type Matrix

| Provider | LLM | Image | Video | Implementation |
|----------|-----|-------|-------|---------------|
| OpenAI | ✅ | ✅ (DALL-E) | ❌ | rig native |
| Anthropic | ✅ | ❌ | ❌ | rig native |
| OpenRouter | ✅ | ❌ | ❌ | rig native |
| Gemini | ✅ | ✅ | ❌ | rig native |
| Ollama | ✅ | ❌ | ❌ | rig native |
| Cohere | ✅ | ❌ | ❌ | rig native |
| DeepSeek | ✅ | ❌ | ❌ | rig native |
| Groq | ✅ | ❌ | ❌ | rig native |
| Perplexity | ✅ | ❌ | ❌ | rig native |
| xAI | ✅ | ❌ | ❌ | rig native |
| Mistral | ✅ | ❌ | ❌ | rig native |
| Together | ✅ | ❌ | ❌ | rig native |
| OpenAI-compat | ✅ | ❌ | ❌ | rig + custom base_url |
| SiliconFlow | ❌ | ✅ | ✅ | custom adapter |
| ComfyUI | ❌ | ✅ | ✅ | custom adapter |

## File Changes

### New Files

| File | Purpose | Lines (est.) |
|------|---------|-------------|
| `providers/unified.rs` | `UnifiedLlmProvider` — single generic LLM provider | ~150 |
| `providers/image_unified.rs` | Unified image provider via rig's trait | ~80 |
| `providers/adapters/mod.rs` | Adapter module | ~5 |
| `providers/adapters/siliconflow_image.rs` | SiliconFlow → rig ImageGenerationModel | ~100 |
| `providers/adapters/comfyui_image.rs` | ComfyUI → rig ImageGenerationModel | ~100 |

### Deleted Files

| File | Lines Removed | Reason |
|------|--------------|--------|
| `providers/openai_client.rs` | 330 | Replaced by rig's built-in SSE streaming |
| `providers/openrouter.rs` | 88 | Replaced by rig-dyn |
| `providers/ollama.rs` | ~100 | Replaced by rig-dyn |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `rig-core`, `rig-dyn`; remove `eventsource-stream` |
| `providers/mod.rs` | Update module declarations |
| `providers/traits.rs` | Keep `StreamChunk` only; remove `LlmProvider` and `ImageProvider` traits |
| `commands/chat.rs` | Replace `create_llm_provider()` with `UnifiedLlmProvider::from_config()` |
| `commands/providers.rs` | Update test_connection and list_models to use rig |
| `models/provider.rs` | Expand `ProviderAdapter` enum with all rig-supported providers |

### Unchanged

- All frontend code (Svelte/TypeScript) — Tauri event interface is identical
- DB schema — `providers` table structure unchanged
- Settings UI — provider management works the same
- Memory pipeline — no changes
- PHI/Narrative Direction — no changes

## Key Design Decisions

1. **rig-dyn for dynamic resolution:** Avoids a massive match statement. Provider string from DB → `Provider::try_from()` → client. Zero code per provider.

2. **OpenAI-compatible as first-class:** Users with LM Studio, KoboldCPP, vLLM, or any OpenAI-compatible server use the `openai` adapter with a custom `base_url`. No separate adapter needed.

3. **Message conversion is a thin function:** Our `ChatMessage` → rig's `Message` is a ~10 line mapping function, not a complex layer.

4. **Stream bridge pattern:** rig produces a stream of completion chunks → we map these to our existing `StreamChunk::Delta/Done/Error` → mpsc channel → Tauri events. The frontend sees zero change.

5. **Image adapters implement rig's trait:** Even custom image providers (SiliconFlow, ComfyUI) implement rig's `ImageGenerationModel`, so calling code doesn't know or care which backend is active.

## Error Handling

- `rig-dyn::Provider::try_from()` returns `Err` for unknown adapter strings → maps to `MythicError::Config`
- rig's stream errors → caught and sent as `StreamChunk::Error` (existing pattern)
- Network errors (connect timeout) → rig uses our shared reqwest client with `connect_timeout(30s)`
- API key validation failures → rig returns HTTP 401 → mapped to user-facing error

## Testing Strategy

1. **Compile check:** `cargo check` passes
2. **Stream test:** Send message to Aria via OpenRouter → tokens stream correctly
3. **Follow-up test:** Second message in same conversation → no hang
4. **Error test:** Invalid API key → error state with retry button
5. **Provider listing:** Settings → models list populates
6. **Health check:** Settings → test connection works
