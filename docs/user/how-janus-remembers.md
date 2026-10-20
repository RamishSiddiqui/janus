# How Janus remembers

Models only see a limited amount of text at once, the **context window**. A long story does not fit, and most apps quietly cut the oldest messages. Janus builds each prompt in layers so the important parts survive.

## What goes into every prompt

From the top, each turn is assembled from:

1. **The system prompt:** the app-wide instructions (editable under [Settings, Prompts](settings.md#prompts)) plus the character's own system prompt.
2. **The character card:** description, personality, scenario, example dialogue, and your persona.
3. **Lorebook entries** that triggered, plus those marked always active (capped).
4. **Memories:** your pinned ones, canon memories, and the ones found to be relevant (capped).
5. **Emotional state:** how each character feels right now ([Emotions and scenes](emotions-and-scenes.md)).
6. **Scene state:** where and when the story is.
7. **The story so far:** a rolling summary of older messages that no longer fit.
8. **Relevant past messages:** passages found by semantic search (if turned on).
9. **Recent messages**, word for word, as many as fit.
10. **Narrative direction:** the post-history instructions, placed after the conversation to shape how the reply ends.

## The budget

Janus counts tokens and splits the available space so no layer can crowd out the rest:

- The window size is **Context Window Size** in [Settings, Context and Memory](settings.md#context-and-memory) (default 16,384). Set it to your model's real limit.
- Room is reserved for the reply, and 10% is kept as a safety margin because providers count tokens slightly differently.
- Memories and lorebook entries each have a hard cap, so a huge lorebook cannot push out the conversation.
- Whatever is left after the fixed layers goes to history: up to about 20% of it for the summary, about 10% for retrieved passages, and the rest for recent messages.

The **token** counter under the chat box shows what is about to be sent.

## Rolling summaries

When messages no longer fit, Janus does not just drop them. After a reply finishes, it asks the model to fold the evicted messages (plus the previous summary) into a short third-person summary of events, relationships, important details and tone. It appears in later prompts as the **story so far**. Turn this off with **Auto-Summarize**.

## Semantic memory (vector search)

With **Semantic Memory** on, Janus turns messages and memories into numeric "embeddings" and finds the passages closest in meaning to what is happening now, using two signals together: meaning (vector similarity) and keywords (BM25), blended so that neither dominates. This is how an old detail comes back when it becomes relevant.

It needs an **embedding model** from a provider that offers them (OpenRouter, Ollama, and others). Enable one in **AI Studio, Embedding Models**, then pick it in Settings.

Settings in **Context and Memory**:

| Setting | Meaning |
|---|---|
| **Embedder Model** | Which embedding model to use. |
| **Top-K Results** | How many passages to retrieve. |
| **Min Similarity** | Ignore matches below this score (0 to 1). |
| **Index Status** | How many messages are indexed (**Indexed**, **Total**, **Coverage**) and the vector size. |
| **Rebuild Index** | Re-embed everything. Needed when you change to a model with a different vector size. |

If you switch embedding models, Janus shows **Dimension Mismatch** or **Model Changed** and asks you to rebuild, because vectors from different models cannot be compared.

## Fixing a bad memory

If the model gets something wrong, change the source: edit or delete the memory, fix the lorebook entry, or edit the message. Janus does not hide its memory in a black box; everything above is visible and editable.
