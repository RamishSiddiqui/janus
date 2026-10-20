# Events

Long-running work reports progress through events. The list with payload sources is in the {doc}`/reference/events`.

## Why not `app.emit`

Tauri's `app.emit` is app-wide: every open window receives it. With several accounts that would send one account's streamed reply (or the audio of it) to another's screen. So every emit goes through `events::EmitScoped::emit_scoped`:

```rust
app.emit_scoped("chat-stream", payload)?;
```

## How routing works

`events.rs`:

1. `emit_scoped` puts a `BusEvent { name, payload, owner }` on an ordered mpsc queue. One task drains it, so a stream's chunks stay in order.
2. The owner is found from the ids in the payload (`conversation_id` / `conversationId`, a message id, and so on) through a small cache (`OwnerCache`) over the ownership queries. An event whose owner cannot be found has no owner.
3. The event is sent to:
   - **the desktop window** (Tauri `emit`) only if it belongs to the account the desktop is signed in as, or belongs to nobody, or no accounts exist yet;
   - **browsers** through a `tokio::sync::broadcast` channel (`AppState.event_bus`).
4. Each browser's `GET /api/events` stream receives only events where `BusEvent::visible_to(user)` is true: events owned by that user. An ownerless event reaches browsers only if its name is in `PUBLIC_EVENTS` (currently `embedding_updated`, which only says "refresh what you already show"). The bus holds 512 events.

A slow browser that lags the channel skips old events and continues with newer ones.

## Adding an event

1. Emit with `emit_scoped` from the Rust side.
2. Make sure the payload lets the router find an owner (it carries a conversation id or similar), or, for something safe for everyone, add the name to `PUBLIC_EVENTS`.
3. Listen with `listen('name', ...)` in the frontend. The bridge makes the same call work in a browser.
4. Run the reference generator so the event appears in the docs.

## Payload shapes

| Event | Payload |
|---|---|
| `chat-stream` | `{ event_type: 'delta' \| 'reasoning' \| 'done' \| 'error' \| 'cancelled', content, message_id }` (see `StreamEvent` in `ipc.ts`). |
| `multi-char-response` | The message split into speaker segments. |
| `tts-chunk` / `tts-stream-end` | Audio for one sentence with a sequence number; the end marker for a message. |
| `tts-download-progress` | Download progress of the voice model. |
| `scene_state_changed` | The conversation's new scene state. |
| `ai_horde_progress`, `wangp_progress` | Image/video job progress. |
| `npc_created` | A new NPC joined the cast. |
| `embedding_index_progress`, `embedding_updated` | Index rebuild progress and refresh hints. |
