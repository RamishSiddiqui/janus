# Lorebooks

A lorebook holds facts about the world: places, factions, rules, history. Instead of stuffing everything into the character description, you write short entries, and Janus adds an entry to the prompt only when it is relevant. This saves space and keeps the model focused.

Open a character's lorebook with the **Lorebook** icon in the chat header.

## Entries

Each entry has:

| Field | Meaning |
|---|---|
| **Name** | A label for you. |
| **Keywords** | Comma-separated words or phrases. When one appears in the recent conversation, the entry is added to the prompt. |
| **Content** | The text added when the entry triggers. |
| **Priority** | Higher-priority entries win when space is tight. |
| **Active** | Turn an entry off without deleting it. |
| **Always active** | Ignore keywords and add the entry to every message. Use it for things the model must always know. |

You can **add**, **edit**, **delete**, **search** (filter box), and **reorder** entries (move up and down).

Lorebook text is limited to a share of the prompt (about 1,500 tokens) so it cannot crowd out the conversation. See [How Janus remembers](how-janus-remembers.md).

## Getting entries

- **Write them** with the add button.
- **Import from a card:** a character card PNG can carry its own lorebook. Importing the card imports those entries as real, active ones. Entries shown from a card but not yet imported appear greyed with an import button; they do nothing until imported.
- **Generate from the story:** the **Generate lorebook from story** button asks the model to draft entries from what has happened in the chat. Review them before relying on them.

Lorebooks belong to a character. They are included in a [backup](data-and-privacy.md#backup-and-restore).
