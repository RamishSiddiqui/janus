# Getting started with Janus

Janus is a desktop roleplay chat app. Your characters, chats and settings stay on your computer. To chat you need one thing: a language model to write the replies.

## 1. Connect a model

On first launch the setup guide asks where the AI should come from. Pick one:

| Source | Cost | What you need |
|---|---|---|
| Ollama | Free | [Ollama](https://ollama.com) installed and a model pulled, for example `ollama pull llama3.2`. No account. |
| OpenRouter | Free and paid models | An API key from openrouter.ai/keys. Models ending in `:free` cost nothing. |
| Puter | Free models | A personal token from puter.com/dashboard (Account, Create token). |
| OpenAI-compatible | Varies | LM Studio, KoboldCPP, vLLM, or any server with an OpenAI-style API. Enter its address. |

The guide tests the connection, lists the models it finds, and saves your choice. If the test fails, the message says what went wrong (wrong key, server not running, wrong address).

You can skip the guide and add providers later under **AI Studio, Providers**. Other providers (Anthropic, Gemini, Groq, DeepSeek and more) are added there.

## 2. Start a chat

The last step offers three starter characters: Mara Vell (a lighthouse keeper), Quill (a game master) and Sol (a friendly companion). Choosing one creates it and opens a chat.

More characters:

- **Characters** page: create your own, or import a character card (PNG or JSON, the standard V2 card format).
- Edit any character to change its personality, scenario and greeting.

## 3. Optional extras

- **Scene images:** add an image provider. The AI Horde preset is free and needs no signup. ComfyUI and WanGP work for local generation.
- **Voice:** turn on voice in Settings. Built-in Kokoro runs locally; ElevenLabs, Google Cloud TTS and Fish Audio are available as cloud providers.
- **Memories and lorebooks:** Janus can remember facts from your chats and inject world details when keywords appear.

## Troubleshooting

- **"Could not reach" Ollama:** make sure Ollama is running and the address is `http://localhost:11434`.
- **No models listed:** type the exact model name yourself in the guide, or add it later under Providers.
- **Run the guide again:** it only appears when no chat provider exists. Add providers manually under AI Studio, Providers at any time.
