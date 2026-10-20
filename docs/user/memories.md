# Memories

Memories are short, lasting facts about your story: who someone is, what was promised, what was decided. Janus keeps them in a database and puts the relevant ones in front of the model, so a detail from message 5 still matters at message 500.

## Where memories come from

- **You pin them.** In a chat, the memory panel lets you write a fact, instruction or note (**Pin a memory**). Pinned memories are always available to the model.
- **Janus extracts them.** With **Auto-Save Memories** on (Settings, Chat Behavior), Janus reads each exchange and saves only facts that matter beyond the current scene (identities, relationships, decisions, revelations). Most messages produce none.
- **Janus can search them.** With semantic memory on, relevant memories are retrieved by meaning even if the words differ. See [How Janus remembers](how-janus-remembers.md).

Each memory has an **importance** from 1 to 10 (5 is neutral). Higher importance ranks it higher when space is limited. You can edit the text or importance at any time.

## Canon

A memory can be promoted to **canon**: a character-level fact that is true across all of that character's chats, rather than one conversation. Canon is the trunk; conversation memories branch from it.

## Memory scope

Each chat chooses how it uses memory:

| Scope | Meaning |
|---|---|
| **Character** (shared) | The chat reads and writes the character's memories, shared with other chats of that character. |
| **Conversation** (isolated) | The chat keeps its own memories that do not leak into other chats. |
| **None** | Memory is off for this chat. |

## The Memories page

Open **Memories** in the sidebar and choose one or more characters above. Two views show the same data:

- **Graph:** a map with the character in the middle, their conversations as timelines, and each memory as a node. Canon memories are flagged. Lines show links between conversations.
- **Timeline:** the memories laid out by conversation, with a filter to choose which timelines to show.

The header counts **Memories**, **Canon**, **Timelines** and **Links**. Select a memory (or a group) to open the action panel.

### Sharing memories between chats

A **link** carries a memory from one conversation to another. In the action panel, choose:

- **Link type:** **copy** makes a one-time snapshot; **sync** keeps the two in step.
- **Direction:** **one way** (source to target) or **two way**.
- **Sync mode:** **auto** (Janus keeps them in step) or **manual** (you trigger it).

You can **unlink** at any time (you are asked to confirm).

## Branched chats

When you branch a conversation, the new chat appears as a timeline connected to its parent in the graph, and can inherit the memories the parent had at the branch point.
