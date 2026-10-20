# Chat pipeline

How a message becomes a reply, and everything that happens afterwards.

## Messages and timelines

Messages form a tree. Each message has a parent, and the conversation stores an `active_message_id`, the tip of the branch being shown. The visible chat is the chain from the root to that tip.

- Sending a message creates a child of the active tip and moves the tip to it.
- Regenerating creates a **sibling** of an assistant message (same parent). The siblings are the "timelines" the user flips between (`get_message_siblings`, `set_active_message`).
- `branch_conversation` copies the chain up to a message into a new conversation and records `parent_conversation_id` and the branch point, so the UI can link back and the memory graph can draw the branch.

## `send_message`

`commands/chat/send.rs`:

1. Resolve the actor, check the conversation belongs to them, and validate any attachment paths (`ensure_attachment_path`).
2. **Save the user message** as a child of the active message.
3. Resolve once per send: the attachment images, the primary character, the voice assigned to the speaker, and the list of cast characters (always including the conversation's primary character first).
4. In the background, **embed the user message** for vector search.
5. Resolve the provider and model, then **build the prompt** with `context::prompt_builder::build_prompt` (see {doc}`context-engine`). This returns the messages to send and `ContextStats`.
6. Start generation. With streaming on, spawn the stream-completion task. With streaming off, make one call and save the result.

## Streaming

`providers::unified::RigProvider` turns the provider's stream into `StreamChunk` values over a channel: `Delta`, `ReasoningDelta`, `Done(full_text)`, `Error`.

`commands/chat/streaming.rs` consumes them:

| Chunk | What happens |
|---|---|
| `Delta` | Append to the partial text (kept so Stop can save it), emit `chat-stream` with `event_type: "delta"`, and if the speaker has a built-in voice, feed the sentence chunker and emit `tts-chunk` events. |
| `ReasoningDelta` | Emit `chat-stream` with `"reasoning"` so the UI shows it in the Thinking block and never as dialogue. |
| `Done` | If the text is empty, retry once. Otherwise parse multi-character segments, save the message, emit `done`, then start the post-turn work below. |
| `Error` | Emit `chat-stream` with `"error"`; the message is marked failed so the UI offers Retry. |

The provider call has a 90-second silence timeout. `cancel_generation` aborts the task and saves what had streamed, with a `cancelled` event.

### Multi-character replies

In a group chat the prompt asks for `[CharName]: text` parts. `context::response_parser` splits the final text into segments attributed to cast members (falling back gracefully to one segment when no markers are found). The final message stores its segments, and `multi-char-response` tells the frontend. While streaming, the frontend's presentation buffer detects markers mid-stream so each speaker gets their own bubble immediately.

## After a reply

Spawned tasks that never block or fail the reply:

| Work | Where | Result |
|---|---|---|
| Embed the assistant message | `context::rag::embed_and_store` | Vector for later retrieval; `embedding_updated`. |
| Rolling summary | `context::summary::generate_rolling_summary` | Runs when the window evicted messages. |
| Scene state extraction | `context::scene_extractor` | Updates location, characters present, ambience; emits `scene_state_changed`. |
| NPC detection | `context::npc::pipeline` | Every fifth message; see {doc}`npc-pipeline`. |
| Auto image | Frontend | If **Auto-Generate Scene Images** is on and the scene changed. |

The **frontend** runs two more calls through `generate_raw` (a stateless one-shot completion): `emotion-updater.ts` (the speaker's new emotional state, saved with `upsert_character_state` and `set_message_emotional_snapshot`) and, if **Auto-Save Memories** is on, `memory-extractor.ts` (atomic facts saved with `create_memory`).

## Retry and regenerate

- `retry_failed_message` re-runs a failed assistant message with the same prompt inputs it had, including its attachments.
- `regenerate_message` builds a prompt from the history before an assistant message and writes a new sibling.

## Attachments

Images are written to `attachments/<uuid>.<ext>` (png, jpg, jpeg, webp, gif) and recorded in the message metadata as `{ relativePath, mimeType }`. When a prompt is built, the files are read back and passed to the provider as image parts. A file that cannot be read is dropped silently so generation still works. Paths must be plain `attachments/<file>` names (no folders) and may not already belong to another account's message.

## Where to change things

| To change | Look at |
|---|---|
| What the model sees | `context/prompt_builder.rs` |
| Token budget shares | `context/budget.rs` |
| Streaming behaviour and events | `commands/chat/streaming.rs` |
| Provider calls | `providers/unified.rs` |
| Frontend streaming state | `stores/chatStream.ts`, `services/presentation-buffer.ts` |
