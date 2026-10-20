# Characters

A character is who you talk to. Everything about them lives in a **character card**: a name, a description, a personality, a scenario, an opening message, and optional extras. Janus reads and writes the standard Character Card V2 format, so cards from other roleplay tools work.

## The gallery

Open **Characters** in the sidebar to see every character as a portrait card.

- **Search** filters by name as you type.
- **Create** opens a form for a new character.
- **Import** reads a character card from a PNG file (see [Importing cards](#importing-cards)).
- Click a card to open its **profile page**. The pencil icon on a card edits it, the bin deletes it (it goes to the [Trash](data-and-privacy.md#trash), not away for good).
- The heart on a card marks a favourite.

## Creating a character

Press **Create** and fill in:

| Field | What it is for |
|---|---|
| **Name** (required) | What the character is called. Used for the chat title and to label their messages. |
| **Description** | Who they are: appearance, background, anything the model should know. |
| **Personality** | Traits, habits, how they speak. |
| **Scenario** | Where and when the story starts, and what is going on. |
| **First message** | The character's opening line. It is shown when you start a chat. |
| **System prompt** | Optional extra instructions for this character only, added to the app-wide prompt. |
| **Tags** | Comma-separated labels such as `Fantasy, Adventure`. The first tag colours the card. |

Two placeholders work in any text field: `{{user}}` becomes your [persona](personas.md) name (or "User" when none is chosen) and `{{char}}` becomes the character's name.

## The profile page

Click a character to see their page. It shows the **Description**, **Personality**, **Scenario** and **First message**, with tabs for the character's lorebook and canon memories, and counts for conversations, messages and canon memories.

- **Start New Chat** opens a conversation with them (using your selected persona, if any).
- **Edit Character** changes any field, and lets you choose their **voice** (see [Voice](voice.md)). The speaker button previews a voice.
- **Recent conversations** lists your chats with them and opens them.

## Importing cards

Character cards are PNG images with the character data embedded in them. Press **Import**, pick a PNG, and Janus:

1. reads the embedded V2 card data,
2. creates the character,
3. uses the PNG itself as the portrait, and
4. imports the card's embedded lorebook (if there is one) as real, active lorebook entries.

A PNG without character data is rejected with a message. JSON-only cards are not imported yet.

## Portraits

A character's portrait comes from the imported PNG. For characters created inside Janus you can upload an image, or generate one if you have an image provider set up (see [Scenes and images](emotions-and-scenes.md)). Characters Janus discovers during a story ([NPCs](cast-and-npcs.md)) can get generated portraits that you approve or reject.

## Characters and accounts

Every account has its own characters. If you use Janus from a [browser](browser-access.md), you see only your own.
