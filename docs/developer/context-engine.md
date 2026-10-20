# Context engine

`src-tauri/src/context/` holds the logic that decides what the model sees. It is deliberately free of Tauri state, so it can be reasoned about and tested on its own.

## Modules

| Module | Role |
|---|---|
| `prompt_builder.rs` | Assembles the full prompt and returns `ContextStats`. |
| `budget.rs` | `ContextBudget` and `allocate()`: how many tokens each part may use. |
| `window.rs` | `apply_sliding_window()`: keep the newest messages that fit. |
| `tokenizer.rs` | Token counting with `tiktoken-rs` (an approximation for non-OpenAI models). |
| `summary.rs` | Rolling summary of evicted messages. |
| `rag.rs` | Hybrid retrieval: vector similarity plus BM25, fused by rank. |
| `scene_extractor.rs` | Extracts scene state from the latest text. |
| `response_parser.rs` | Splits `[Name]: text` multi-character replies. |
| `npc/` | NPC detection, profile generation and the pipeline ({doc}`npc-pipeline`). |

## Prompt layers

`build_prompt` emits system messages in this order, then history, then the closing instruction:

1. System prompt (the app-wide text from settings) and character card(s). In a group chat every card is included, plus a group scene directive and the "other characters present" note.
2. "About the user" (the persona).
3. Lorebook entries (always-active plus keyword-triggered, capped by `max_lorebook_tokens`).
4. Memories (semantic matches if available, else recency-ordered; capped by `max_memory_tokens`).
5. Emotional state of each speaker.
6. Scene state.
7. Rolling summary ("story so far").
8. Retrieved past messages (RAG), if enabled.
9. The sliding window of recent messages.
10. Post-history instructions (the narrative direction), after the history.

Layers 1 to 6 are the **fixed layers**. Their token count is subtracted from the budget before history is considered.

## Budget

```text
total_usable   = (max_context_tokens - reserved_for_response) x safety_margin
history_budget = total_usable - fixed_layers_tokens
summary_budget = history_budget / 5      (up to 20%)
rag_budget     = history_budget / 10     (up to 10%)
messages_budget = what remains
```

Defaults: 16,384 context tokens, 2,048 reserved for the reply, a 0.90 safety margin, 800 tokens for memories and 1,500 for the lorebook. The context size comes from the user's **Context Window Size** setting. `ContextStats` reports every share (`fixed_tokens`, `history_tokens`, `summary_tokens`, `rag_tokens`, and included/evicted message counts) and is shown by the token counter.

## Sliding window

`apply_sliding_window` walks from the newest message backwards, adding messages until the budget is spent, and returns them oldest-first with the count evicted. Evicted messages feed the rolling summary.

## Rolling summary

When messages are evicted, a background call folds them and the previous summary into one third-person past-tense summary under 500 words. It is stored per conversation (`conversation_summaries`) and injected next turn. Failures are logged and ignored.

## Hybrid retrieval

`rag.rs` retrieves from two stores:

- **Messages**: `message_embeddings` with a vector index and a full-text index.
- **Memories**: `memories` with embeddings and full-text.

Each retrieval runs two rankers, cosine similarity (vector) and BM25 (keywords), and fuses their ranked lists with **Reciprocal Rank Fusion**: `score(id) += 1 / (60 + rank)` for each list the id appears in. RRF combines by rank because cosine (bounded) and BM25 (unbounded) live on incompatible scales. The `top_k` and `min_similarity` settings bound the result.

The embedding model comes from the enabled embedding models (AI Studio), resolved through `enabled_models`, not from the chat provider's own config; an embedding model is often hosted by a different provider than the chat model. Vector size is stored with the index, and changing models requires a rebuild (`rebuild_embedding_index`) since vectors from different models are not comparable. The HNSW index is created at runtime by `ensure_vector_index` once the dimension is known.

## Memory scope

A conversation's `memory_scope` is `character` (shared memories), `conversation` (its own), or `none`. Retrieval and extraction respect it; see {doc}`database` for the memory tree and links.

## Testing it

`tests/vector_search.rs` and `tests/hybrid_search.rs` run retrieval against a real embedded database. The budget and window functions have unit tests beside them.
