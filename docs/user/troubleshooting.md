# Troubleshooting

When something is wrong, **Settings, Logging** shows what Janus was doing. Export the log and include the relevant lines (with any API keys removed) if you report a problem.

## Replies

**"Generation failed" or "The provider stopped responding"**
The provider did not answer, returned an error, or is overloaded. Press **Retry**, or choose another model with the model pill in the chat box. Free models on busy services (a `:free` model on OpenRouter, for example) can stall for a long time; Janus gives up after 90 seconds.

**"No provider configured" or a provider shows Failed**
Add a chat provider under **AI Studio, Providers** and check its status. A failure shows why: a wrong key, a wrong address, a server that is not running, or a timeout.

**The model forgets things**
Raise **Context Window Size** to match the model, turn on **Auto-Summarize**, and consider **Semantic Memory**. See [How Janus remembers](how-janus-remembers.md). Pin the important facts as memories.

**The reply cuts off or the model complains the prompt is too long**
**Context Window Size** is larger than the model allows. Lower it to the model's real limit.

**Replies include odd formatting or the character speaks for you**
Edit the message, or adjust **Default System Prompt** and **Narrative Direction** under Settings, Prompts. **Reset** restores the shipped text.

## Providers

**Ollama: "could not reach"**
Make sure Ollama is running and the address is `http://localhost:11434`. Pull a model first (`ollama pull <name>`).

**No models are listed**
Press **Refresh models** on the Models page. Some servers do not list models; type the exact model name instead.

**A model I enabled disappeared**
The provider stopped listing it, so Janus disabled it and marked it **No longer listed**.

## Images

**"No matching worker online right now"**
AI Horde has no worker for that model at the moment. Wait, or choose a more common model. Registering a free key at aihorde.net improves your priority.

**Image generation does nothing**
Check that an **Image** provider exists, is Connected, and that at least one image model is enabled under **AI Studio, Image/Video Models**.

## Voice

**Nothing speaks**
**Enable Voice Output** is off by default. If it is on and you use Kokoro, check that **Voice Model** shows downloaded. For cloud voices, check the provider's status.

**Voice is slow to start the first time**
The engine takes a few seconds to load; Janus warms it up at startup when voice is on.

## Memory search

**"Dimension Mismatch" or "Model Changed"**
The embedding model differs from the one that built the index. Press **Rebuild Index** in Settings, Context and Memory.

## Browser access

**The page will not open from another device**
Browser access must be **on**, and **Other devices on my network** must be on and applied. Use the computer's address on your network, not `127.0.0.1`. A firewall may need to allow the port. See [Browser access](browser-access.md).

**"Sign in first" appears while using Janus**
The session ended (signed out elsewhere, expired, or the host restarted). Sign in again.

## Updates

**Check now says "No update information is published yet"**
The release does not have its update file yet, or you are offline. Try again later, or download the newest installer from the releases page.

**An update failed or "could not be verified"**
Janus only installs updates signed by the project, so a damaged or tampered download is refused and nothing changes. Check your connection and try again, or install the latest installer manually.

## Starting up

**Janus will not start after an update**
Open `logs/janus.log` in the application data folder (see [Data and privacy](data-and-privacy.md#where-the-data-lives)); the last lines say why. A rolling backup of your data is in `backups/janus_auto_backup.surql`. Please report it as an issue with the log lines.

## Still stuck

Open an issue at the project's GitHub page with what you did, what you expected, what happened, your operating system, and the relevant log lines. For security problems, see the security policy in the repository.
