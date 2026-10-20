# Frequently asked questions

**Is Janus free?**
Yes. It is open source (AGPL-3.0) with no subscription. You pay only for AI providers you choose to use, and some are free.

**Do I need an account or an internet connection?**
No account is needed to use Janus on your computer. You need a connection only to reach a cloud AI provider. With Ollama and the built-in Kokoro voice, everything can run offline.

**Which AI should I use?**
For no account and full privacy: Ollama with a model that fits your computer. For quality with no cost: models marked `:free` on OpenRouter, or Puter's free models. For the best writing: a paid model from Anthropic, Google or OpenAI-compatible services.

**Where is my data?**
On your computer, in the application data folder. See [Data and privacy](data-and-privacy.md).

**Can I import characters from SillyTavern or other tools?**
Yes, character card PNGs in the common Character Card V2 format import directly, including their embedded lorebooks. See [Characters](characters.md).

**Can two people use one Janus?**
Yes: turn on [browser access](browser-access.md). Each person gets an account with their own characters, chats, memories and providers.

**Why did a character stop remembering something?**
Probably it fell out of the context window. Pin it as a memory, raise the context size to your model's limit, and turn on summaries and semantic memory. See [How Janus remembers](how-janus-remembers.md).

**What is a timeline?**
Another version of one reply. Regenerating keeps the old answer so you can flip between them. See [Chatting](chatting.md).

**Can I undo a deletion?**
Conversations, characters and personas go to the Trash first and can be restored. Individual messages and memories are deleted immediately.

**Does it send my chats to anyone?**
Only to the AI providers you add, to get replies. There is no telemetry or cloud sync. See [Data and privacy](data-and-privacy.md).

**Is the browser version safe on the internet?**
No. It serves plain HTTP and is meant for a home network you trust. Do not expose its port to the internet.

**Does it work on a phone?**
Through the browser: open Janus from your phone on the same Wi-Fi. There is no separate phone app. See [Browser access](browser-access.md).

**Can I change the look of the AI's instructions?**
Yes, under Settings, Prompts.

**Why does the first launch ask me to set up a model?**
Janus needs a language model to write replies. The setup guide appears once, only when no chat provider exists, and you can skip it.
