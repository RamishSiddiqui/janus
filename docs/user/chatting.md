# Chatting

## Starting and finding chats

- **From the gallery:** open a character and press **Start New Chat**, or press the chat button on a card.
- **From the sidebar:** **Chats** lists every conversation, grouped under the character they are with. The group of your current chat starts open. Group chats with several characters sit together under their own heading.
- **New chat shortcut:** `Ctrl+N` (`Cmd+N` on a Mac) goes back to the chat start page.
- **Rename or delete:** right-click a conversation in the sidebar (or use its menu) and choose **Rename** or **Delete**. Deleted chats go to the [Trash](data-and-privacy.md#trash).
- **Search:** the search box in the sidebar finds words in the text of every message in every chat, and jumps to the one you pick.

## Sending a message

Type in **Write your response...** and press **Enter** to send. **Shift+Enter** adds a new line. Markdown is supported.

Janus formats roleplay text for you: `*actions in asterisks*` appear as italic action text, `**double asterisks**` as bold, and quoted speech stays as dialogue, so you do not need any extra formatting.

Under the box you see a running **token** count for what will be sent.

### The model picker

The pill at the right of the input shows the model that will answer. Click it to search your enabled models and pick another for this chat. Models are enabled under [AI Studio](providers-and-models.md).

### Images

- Press the paperclip to attach images (PNG, JPEG, WebP or GIF, up to 10 MB each, up to 4 per message), or **paste** a screenshot straight into the box.
- A vision-capable model actually sees the image. If the selected model does not accept images, the attach button says so.
- Remove an attachment with its **x** before sending.

### Stop

While a reply is being written, **Send** becomes **Stop**. Stopping keeps what was written so far.

## Replies

Replies stream in word by word (turn this off under [Settings, Chat Behavior](settings.md#chat-behavior)). Models that "think" first show a collapsible **Thinking** section, which you can hide in settings. A small mood tag next to a message shows the character's [emotional state](emotions-and-scenes.md).

If a provider does not answer or returns an error, the message shows **Generation failed** with the reason and a **Retry** button. A provider that stays silent for 90 seconds is treated as failed.

## Changing the story: timelines

Every message action works on a message in the chat:

| Action | What it does |
|---|---|
| **Regenerate** | Writes a new answer to the same message. The old answer is kept as another **timeline** you can flip back to. |
| **Timeline arrows** | Under a message with several answers, step between them. The chat continues from whichever you choose. |
| **Edit** | Change the text of a message, then **Save** or **Cancel**. |
| **Copy** | Copies the text. |
| **Branch from here** | Starts a new conversation that contains the story up to this message. The new chat remembers where it came from ("Branched from ..."), and the header link jumps back. |
| **Delete** | Removes the message. |

Nothing is lost when you regenerate: timelines exist so you can explore "what if" without losing the version you liked.

## The chat header

The icons at the top right of a chat open side views without leaving the chat. Press the same icon again to close it.

- **Lorebook:** world details for this character ([Lorebooks](lorebooks.md)).
- **Cast:** the characters in this conversation, with a dot when something needs your attention ([Cast and NPCs](cast-and-npcs.md)).
- **Persona:** who you are in this chat ([Personas](personas.md)).
- **Scene Gallery:** every image generated in this chat ([Scenes and images](emotions-and-scenes.md)).
- **Context Panel:** the character's details, memories and state beside the chat. It is hidden on narrow screens.

## Group chats

Add more characters to a conversation from the **Cast** view. Each reply can be spoken by several characters; Janus labels each part with who said it. See [Cast and NPCs](cast-and-npcs.md).

## Keyboard shortcuts

See [Shortcuts](shortcuts.md).
