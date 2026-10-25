# Data and privacy

Janus is local-first. Your characters, chats, memories and settings live on your computer. The only things that leave it are the requests you send to the AI providers you choose to add.

## What leaves your computer

- The text of your prompts (and any attached images) go to your **chat provider** to get a reply. A local provider such as Ollama keeps even that on your machine.
- Scene requests go to your **image or video provider**, and voice requests to a **cloud voice provider** if you use one. The built-in Kokoro voice stays local.
- Memory extraction, summaries, NPC detection and similar background steps also call your chat provider with parts of the conversation.
- The desktop app asks GitHub whether a newer Janus exists (at launch and every few hours). The request carries no personal data and no part of your data; turn it off under **Settings, Updates**.
- Janus has **no telemetry, no analytics and no cloud sync**. Its fonts are bundled with the app, so it makes no request to a font host. Apart from your providers and the update check, nothing contacts the internet.
- API keys are stored in the local database and are sent only to their own provider. Your own providers (keys included) are visible to your account on the Providers page, in the desktop app or in a [browser](browser-access.md); other accounts cannot see them.

**Local Storage Only** (Settings, Data and Privacy) records your intent to keep it that way; it is on by default. It does not switch off the update check, which is its own setting.

## Where the data lives

Janus keeps everything in its application data folder:

| Platform | Location |
|---|---|
| Windows | `%APPDATA%\com.janus.app` |
| macOS | `~/Library/Application Support/com.janus.app` |
| Linux | `~/.local/share/com.janus.app` |

Inside it:

| Item | Contents |
|---|---|
| `mythic_surreal/` | The database: characters, chats, memories, accounts. |
| `avatars/`, `personas/`, `portraits/` | Character and persona images. |
| `scenes/` | Generated scene images and videos. |
| `attachments/` | Images you attached to messages. |
| `backups/` | The automatic backup. |
| `logs/janus.log` | The backend log. |
| `tts/` | The downloaded Kokoro voice model, if used. |

## Backup and restore

- **Automatic:** every time Janus starts it writes a rolling backup of the whole database to `backups/janus_auto_backup.surql`, replacing the previous one. A failure is logged and never stops the app. The file is SurrealDB's portable format, readable by the `surreal import` tool.
- **Manual:** **Settings, Data and Privacy, Export Data** saves your library as a JSON file you choose: conversations with their full message history, characters with their lorebooks, pinned and canon memories, and settings. **Import Data** restores from such a file.

To move Janus to another computer, export on the old one and import on the new one, or copy the application data folder while Janus is closed.

## Trash

Deleting a conversation, character or persona moves it to the **Trash** (sidebar), where you can **restore** it. **Delete Forever** removes one item for good; **Empty Trash** removes everything in it. Trashed items do not appear in the gallery or sidebar.

## Clearing data

**Clear All Conversations** permanently deletes every conversation after a confirmation. Deleting an [account](browser-access.md) deletes its data and files.

## Seed data in development builds

Developer builds of Janus add a few demo characters on first run so there is something to try. Release builds do not.
