# Settings

Open **Settings** at the bottom of the sidebar. The section chips along the top switch between panels. Most settings are saved on the device you are using (a desktop window and a phone each keep their own); the full list with defaults is in the {doc}`settings reference </reference/settings>`.

A small readout at the top shows Janus's own **memory** and **CPU** use, refreshed every few seconds, with amber and red bands when it climbs. A brief CPU spike while voice is speaking is normal.

(appearance)=
## Appearance

| Setting | Meaning |
|---|---|
| **Theme** | **Dark**, **Light**, or **System** (follow your operating system). |
| **Font Size** | **Small**, **Medium** or **Large**. Scales text across the app. |

(chat-behavior)=
## Chat Behavior

| Setting | Meaning | Default |
|---|---|---|
| **Streaming Responses** | Show replies word by word as they are written. Off shows the whole reply at once. | On |
| **Show Model Thinking** | Show the collapsible "Thinking" section for models that reason first. Off hides it. | On |
| **Auto-Generate Scene Images** | Make a scene image automatically when the scene changes (needs an image provider). | On |
| **Auto-Save Memories** | Extract lasting facts from the conversation into [memories](memories.md). | Off |

(context-and-memory)=
## Context and Memory

See [How Janus remembers](how-janus-remembers.md) for what these do.

| Setting | Meaning | Default |
|---|---|---|
| **Context Window Size** | The most tokens the model can see. Set it to your model's real limit. | 16,384 |
| **Auto-Summarize** | Fold older messages into a rolling summary. | On |
| **Semantic Memory** | Find past messages and memories by meaning. | Off |
| **Embedder Model** | The embedding model used for search. Manage models in AI Studio, Embedding Models. | `openai/text-embedding-3-small` |
| **Top-K Results** | How many passages to retrieve. | 5 |
| **Min Similarity** | Lowest match score to accept (0 to 1). | 0.7 |
| **Index Status** | Indexed, total and coverage counts, and the stored vector size. | |
| **Rebuild Index** | Re-embed everything (needed after changing to a different embedding model). | |

(voice)=
## Voice

See [Voice](voice.md).

| Setting | Meaning | Default |
|---|---|---|
| **Enable Voice Output** | Speak replies aloud as they stream. | Off |
| **Voice provider** | The built-in **Kokoro** engine, or one of your cloud voice providers. | Kokoro |
| **Voice Model** | Status and download of the Kokoro model (about 90 MB, one time). | |
| **Default voice** | Used for any character without a voice of their own. | None |

(image-generation)=
## Image Generation

| Setting | Meaning | Default |
|---|---|---|
| **Allow Mature Content** | Sent to AI Horde so ordinary descriptions are not falsely blocked by the strict filter. | On |
| **Auto-Generate NPC Portraits** | Generate a portrait for each newly detected NPC (needs an image provider). | On |
| **Auto-Approve NPC Portraits** | Use generated NPC portraits immediately instead of waiting for your approval. | Off |
| **Presets** | Saved bundles of generation settings. Create with **Add Preset**; **Set Default**, edit or **Delete** existing ones. | |

A preset has: **Name**, **Model** (optional), **Sampler**, **CFG Scale**, **Steps**, **Karras** (smoother noise schedule), **Style** (optional; overrides sampler, model and size), **Negative Prompt** (ignored if a style is set), **Face Fix**, **Upscaler**, **CLIP Skip** (blank means the model's default) and **Hi-Res Fix** (re-processes at higher resolution for better detail at roughly double the time and cost).

(prompts)=
## Prompts

The instructions added to every generation.

| Setting | Meaning |
|---|---|
| **Default System Prompt** | The app-wide instructions that frame the roleplay. **Reset** restores the shipped text. |
| **Narrative Direction** | Instructions placed after the conversation to shape how replies end: hooks, scene changes, pacing. |
| **Character Profile Refresh** | The instructions used by "Refresh from Story". It must keep asking for JSON with `description`, `personality` and `scenario` only; editing that away breaks refreshes. |

When Janus updates its shipped prompts in a new version, your saved copies of these three are refreshed to the new text.

(data-and-privacy)=
## Data and Privacy

See [Data and privacy](data-and-privacy.md).

| Control | Meaning |
|---|---|
| **Local Storage Only** | Keeps Janus in privacy mode. Turning it off asks for confirmation. |
| **Export Data** | Saves your library to a file you choose. |
| **Import Data** | Restores from such a file. |
| **Clear All Conversations** | Permanently deletes every conversation (asks to confirm). |

(logging)=
## Logging

Backend and frontend activity, for diagnosing problems. **Backend** lines are read from Janus's log file, newest first, loading older lines as you scroll up. **Frontend** lines are collected in the window. Both can be searched, and the logs can be exported to a file. **Clear frontend logs** empties the on-screen list.

(updates)=
## Updates

Only in the desktop app. Janus checks GitHub for a newer release a few seconds after it starts and every few hours while it stays open. When there is one, a card appears in the corner with the new version, **What's new** (the release notes), **Update now** and **Later**. **Update now** downloads the update, checks its signature, installs it and restarts Janus; your data is untouched. **Later** hides the card for that version (it is still offered here).

| Control | Meaning |
|---|---|
| **Check now** | Look for an update right away and report the result. |
| **Update now** | Appears when a newer version is available. |
| **Check for updates automatically** | The launch and every-few-hours check. The check only asks GitHub for the latest release and sends nothing about you or your data. On by default; turn it off to update by hand from the releases page. |

Versions older than the first one with this feature cannot update themselves: install that version once from the releases page, and later ones arrive in the app.

(network-access)=
## Network Access

Only in the desktop app, and only for the admin. See [Browser access](browser-access.md).

| Control | Meaning |
|---|---|
| **Browser access** | Serve Janus to browsers. |
| **Other devices on my network** | Listen on your local network instead of this computer only. |
| **Port** | 1024 or higher. |
| **Who can create accounts** | **Open** or **Admin adds**. |
| **Accounts** | List, add (with a temporary passphrase) and delete accounts; **Use this window as them** to act as an account without its passphrase, and **Return** to the admin. |
