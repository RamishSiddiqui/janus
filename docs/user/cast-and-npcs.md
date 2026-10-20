# Cast and NPCs

A conversation can have more than one character. The **cast** is the list of characters in a chat. Open it with the **Cast** icon in the chat header.

## Adding and managing the cast

From the Cast view you can:

- **Add** a character from your gallery to the conversation, or **remove** one.
- Turn a member **active** or inactive without removing them.
- Set how **talkative** each one is, which controls how often they speak up in group replies.
- Open a member's details: **Description**, **Personality**, **Scenario**, and a **Talk** button that focuses the next reply on them.

When several characters can answer, the model writes the reply as labelled parts, one per speaker (for example `[Aria]: ...`). Janus splits the reply, shows each part under the right speaker, and tracks each speaker's own emotional state and voice.

## Automatic NPC detection

As the story goes on, the model often introduces new people: an innkeeper, a rival, a guard. Janus watches for this in the background and, when a new name becomes significant, creates a character for them and adds them to the cast as an **NPC**.

How it works, in plain terms:

1. Every few messages (every fifth), a quick check looks for names that deserve tracking. It does not write a profile.
2. A name has to be flagged in **two separate passes** before it counts. This avoids creating characters for one-off mentions.
3. Once confirmed, Janus writes a profile (description, personality, scenario) that fits the story so far, and adds them.
4. If an image provider is configured and **Auto-Generate NPC Portraits** is on, a portrait is generated too.

You can skip the wait for a name you know is important with **Confirm**, which adds them now.

### Reviewing NPCs

NPCs carry badges in the Cast view:

| Badge | Meaning |
|---|---|
| **Unconfirmed** | Spoke once and is not yet a recurring character. |
| **Concealed** | The story has not revealed who they really are. You can rename them when it does. |
| **Outdated** | Still carries the placeholder profile written when first spotted. Use **Refresh from Story** to rewrite it from what has happened since. |
| **Promoted** | You promoted them to the gallery. |

A dot on the Cast icon means at least one NPC needs attention.

Per NPC you can **Refresh from Story**, **Promote to Gallery** (makes them a normal character you can start new chats with), **View in Gallery**, **Remove from cast**, and manage the portrait: **Approve**, **Regenerate**, **Reject**, or **Upload** your own. With **Auto-Approve NPC Portraits** on, generated portraits are used immediately instead of waiting for your review.

### Cast memory graph

The **Graph** and **Timeline** buttons in the Cast view show the memories shared across the whole cast, the same way the [Memories page](memories.md) shows one character's.

## Settings that affect this

Under **Settings, Image Generation**: **Auto-Generate NPC Portraits** and **Auto-Approve NPC Portraits**. See [Settings](settings.md#image-generation).
