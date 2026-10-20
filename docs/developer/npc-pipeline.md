# NPC pipeline

`context/npc/` notices when a new character becomes narratively significant, writes a profile for them, and adds them to the conversation's cast. It is separate from `scene_extractor`, which only tracks mechanical scene state.

## Stages

```text
every 5th message
   │  cadence + dedup gate
   ▼
Stage 1: detector      cheap, narrow LLM call: which names deserve tracking?
   │  (names already in the cast or known candidates are filtered out)
   ▼
two-pass debounce      a candidate must be flagged on two separate passes
   │  (npc_candidates keeps counts)
   ▼
Stage 2: profile_generator   pricier call, once per candidate ever
   │  CharacterCardV2-shaped profile fitted to the story so far
   ▼
create character (origin = "npc") + conversation_characters row (role = "npc")
   ▼
emit npc_created
```

- **Cadence:** `NPC_DETECTION_CADENCE = 5` messages.
- **Stage 1** (`detector.rs`) returns names only. It never writes a profile.
- **Debounce** (`db/npc_candidates.rs`): `bump_and_check_due`, `upsert_candidate`, `get_debounced`, `mark_created`. This prevents one-off mentions from becoming characters.
- **Stage 2** (`profile_generator.rs`) produces the profile. It also holds the "refresh from story" and lorebook-generation prompts.
- Everything after the cadence check returns a `Result` that the caller only logs, so a failure never affects the reply.

## Character flags

An NPC is a normal `characters` row with a few fields that drive the UI:

| Field | Meaning |
|---|---|
| `origin` | `gallery` (a real standalone character) or `npc` (auto-created, scoped to its conversation until promoted). Rows from before the field read back as `gallery`. |
| `portrait_status` | `approved` (the default) or `pending_review` for a generated portrait the user has not approved. |
| `profile_reviewed` | False for a freshly generated NPC profile the user has not opened, or one flagged because it still holds the placeholder text. Drives the needs-attention dot. |
| `data.identity_concealed` | Set in the card data when the story has not revealed who the character really is; portrait prompts then avoid describing a specific face. |

`promote_npc_to_gallery` sets `origin` back to `gallery` so the character can start their own chats. `refresh_character_profile` rewrites the profile from the recent story using the user-editable refresh prompt, which must keep returning JSON with `description`, `personality` and `scenario`.

## Portraits

`generate_npc_portrait` builds an image prompt from the profile and uses the default image provider. With **Auto-Approve NPC Portraits** off, the result waits as `pending_review` until the user calls `approve_npc_portrait`, regenerates, or `reject_npc_portrait`. `upload_character_avatar` sets an approved portrait directly.

## Cast memory graph

`get_cast_memory_graph` returns a `MemoryGraph` with one entry per cast member for the conversation, the same structure the single-character graph uses.

## Ownership

NPCs created in the background inherit their owner from the conversation (`inherit_owner_from_conversation`), since no user request is in flight.
