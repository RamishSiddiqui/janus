# Providers and models

A **provider** is a service (or program on your computer) that does the AI work: writing replies, drawing images, speaking, or computing embeddings. A **model** is a specific brain a provider offers. You add a provider once, then choose which of its models to use.

Everything is under **AI Studio** in the sidebar: **Providers**, **LLM Models**, **Image/Video Models**, **Embedding Models** and **Voice Models**.

## Adding a provider

Open **Providers** and press **Add Provider**. (New to this? **Setup guide**, next to it, walks you through connecting a model and meeting a character.) Fill in:

| Field | Meaning |
|---|---|
| **Name** | Anything you like. |
| **Type** | **Chat (LLM)**, **Image**, **Video** or **Voice**. |
| **Adapter** | Which service it is (table below). |
| **Base URL** | Only for local or self-hosted adapters; cloud adapters have a fixed address. |
| **API Key** | Your key for the service. Keys are stored only on this computer. |
| **Default Model** | The model to use unless a chat picks another. |

Each provider card shows **Connected** or **Failed** after a test, with the reason when it fails (bad key, wrong address, server down, timeout). Mark one provider **Default** for its type. You can edit or delete a provider at any time.

## Chat providers (LLM)

| Adapter | Notes |
|---|---|
| **Ollama** | Models running on your computer. No account. Default address `http://localhost:11434`. |
| **LM Studio** / **OpenAI Compatible** | Any server speaking the OpenAI chat format: LM Studio, KoboldCPP, vLLM, a self-hosted gateway. Give its address; a key only if it needs one. |
| **OpenRouter** | One key, hundreds of models from many labs. Models whose name ends in `:free` cost nothing. |
| **Puter (free)** | Free models through a personal Puter token (from your own Puter account's dashboard; not your password). Stored as an OpenAI-compatible provider. |
| **Anthropic**, **Gemini**, **Groq**, **DeepSeek**, **Perplexity**, **xAI**, **Cohere**, **Together**, **Hyperbolic**, **Moonshot**, **HuggingFace** | Cloud providers. API key only. |

## Image and video providers

| Adapter | Notes |
|---|---|
| **AI Horde (free)** | Crowdsourced image generation. The key `0000000000` works with no signup (lowest priority); register at aihorde.net for faster service. Generation settings (sampler, CFG scale, steps, Karras, an optional named style, a negative prompt) are set on the provider card. |
| **ComfyUI** | Your own ComfyUI. Paste a workflow exported in **API format**; put placeholder tokens (`{{POSITIVE_PROMPT}}`, `{{SEED}}`, `{{CHARACTER_IMAGE_1}}`, ...) where Janus should fill in values. |
| **WanGP** | A local WanGP server for images and video. Add two provider rows (one per type) pointing at the same address. Janus lists the models it offers. |
| **SiliconFlow** and other OpenAI-images-compatible services | Generic image generation. |

## Voice providers

**ElevenLabs**, **Google Cloud TTS** and **Fish Audio**, each with an API key. See [Voice](voice.md). The built-in Kokoro voice needs no provider.

## Choosing models

Providers can offer hundreds of models, so Janus asks you to **enable** the ones you want. On each Models page:

- **Search** by model name or provider, filter by provider, status (enabled or not) and pricing (free), and sort by name, price or context length.
- Each row shows the model, its provider, capabilities (**Vision**, **Tool Use**, **Reasoning**), price per million tokens, and context size.
- Use the toggle to enable or disable a model. Enabled models appear in the chat's model picker.
- A model a provider stops listing is marked **No longer listed** and disabled automatically.
- Press **Refresh models** to re-read a provider's list.

The Image/Video page shows architecture, whether the model supports image-to-image, and whether it is an inpainting model. The Embedding page shows vector size ("dimensions") and input limits.

Embedding models power semantic memory; see [How Janus remembers](how-janus-remembers.md).
