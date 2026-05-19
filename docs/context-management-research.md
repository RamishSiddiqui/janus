# Context Management for Long Conversations — Deep Research

## The Problem

Mythic currently sends **the entire conversation chain** to the LLM on every message. For Aria's conversation (22 messages = 26 prompt messages including system layers), this means:
- **Slow responses** — OpenRouter took ~69 seconds because of the massive prompt
- **Exponential cost growth** — token usage scales O(n²) with conversation length
- **Context overflow** — models have finite windows (8K–128K tokens); long conversations will simply break
- **Degraded quality** — "Lost in the middle" effect causes models to ignore mid-context information

---

## Technique Comparison Matrix

| # | Technique | Complexity | Token Efficiency | Memory Quality | Latency | Used By |
|---|-----------|------------|-----------------|----------------|---------|---------|
| 1 | **Naive Full History** | ★☆☆☆☆ | ★☆☆☆☆ | ★★★★★ | ★☆☆☆☆ | Mythic (current) |
| 2 | **Sliding Window** | ★★☆☆☆ | ★★★★☆ | ★★☆☆☆ | ★★★★★ | Basic chatbots |
| 3 | **Rolling Summary** | ★★★☆☆ | ★★★★☆ | ★★★☆☆ | ★★★★☆ | SillyTavern Summarize |
| 4 | **Chunk Summary (Map-Reduce)** | ★★★☆☆ | ★★★☆☆ | ★★★★☆ | ★★★☆☆ | ChatGPT, Document RAG |
| 5 | **Vector RAG (Smart Context)** | ★★★★☆ | ★★★★★ | ★★★★☆ | ★★★★☆ | SillyTavern Vector Storage |
| 6 | **Tiered Memory (OS-Inspired)** | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★☆ | MemGPT/Letta |
| 7 | **Temporal Knowledge Graph** | ★★★★★ | ★★★★★ | ★★★★★ | ★★★☆☆ | Zep/Graphiti, Mem0 |

---

## Detailed Breakdown

### Tier 1: Simple Approaches

---

#### 1. Naive Full History (Current Mythic)
```
[System] → [Character] → [Lorebook] → [MSG 1] → [MSG 2] → ... → [MSG N] → [Memories] → [PHI]
```

> **⚠ CAUTION:** Token usage grows quadratically: Turn 100 sends 100 messages, Turn 200 sends 200. A 500-message RP burns ~2.5M tokens just for context.

- **Pros:** Zero information loss, perfect coherence
- **Cons:** Unbounded cost, will break at context limit, degrades model attention on long contexts

---

#### 2. Sliding Window
```
[System] → [Character] → [Lorebook] → [MSG N-50] → ... → [MSG N] → [Memories] → [PHI]
```

Keep only the last N messages (e.g., 50). Everything older is simply discarded.

- **Pros:** Trivial to implement, constant token cost, fast
- **Cons:** **Catastrophic forgetting** — earlier plot points, character introductions, and emotional beats vanish completely. Terrible for RP where "she gave me a pendant in chapter 1" matters in chapter 20.
- **Who uses it:** Basic chatbots, support systems where history doesn't matter

---

### Tier 2: Production Approaches

---

#### 3. Rolling Summary (SillyTavern's Summarize Extension)
```
[System] → [Character] → [SUMMARY of msgs 1..N-30] → [MSG N-30] → ... → [MSG N] → [PHI]
```

Periodically use the LLM itself to compress older messages into a running summary. Keep recent messages verbatim.

- **How it works:**
  - Every K messages (e.g., every 10), ask the LLM: *"Summarize the following conversation, preserving key events, character developments, and emotional beats"*
  - Replace old messages with the summary
  - New summary = `summarize(old_summary + new_messages_to_compress)`
- **Pros:** Bounded context size, retains narrative flow, relatively simple
- **Cons:**
  - **Information drift** — each re-summarization loses fidelity (summarizing a summary of a summary...)
  - Requires an extra LLM call every K messages (adds latency + cost)
  - Summary quality depends heavily on the summarizing model
- **Who uses it:** SillyTavern (Summarize extension), many ChatGPT wrappers

> **💡 TIP:** SillyTavern's `MessageSummarize` extension improves on this by summarizing individual messages rather than re-summarizing the cumulative summary — avoids drift.

---

#### 4. Chunk Summary (Map-Reduce)
```
[System] → [Character] → [Summary Ch.1] → [Summary Ch.2] → [MSG N-20] → ... → [MSG N] → [PHI]
```

Split conversation into "chapters" (fixed-size chunks). Summarize each chunk independently. Inject relevant chapter summaries.

- **How it works:**
  - Chunk messages into blocks of e.g. 20 messages each
  - Summarize each chunk independently (parallelizable)
  - Include the most recent chunk verbatim + relevant older summaries
- **Pros:** Parallelizable, preserves per-chapter detail, natural "chapter" structure for RP
- **Cons:** Loses cross-chapter connections (e.g., "the pendant from Chapter 1" might not appear in Chapter 3's summary)
- **Who uses it:** ChatGPT's memory, document analysis pipelines

---

#### 5. Vector RAG (SillyTavern's Smart Context / Vector Storage)
```
[System] → [Character] → [Retrieved: relevant past msgs] → [MSG N-20] → ... → [MSG N] → [PHI]
```

Store all messages as vector embeddings. On each turn, semantically search for relevant past messages and inject them.

- **How it works:**
  - Every message gets embedded (converted to a vector representation)
  - When sending a prompt, embed the current message
  - Cosine-similarity search against all past messages
  - Inject top-K most relevant results into the prompt
- **Pros:** Only retrieves what's actually relevant (not everything), handles very long histories efficiently, no information loss in storage
- **Cons:**
  - Requires an embedding model (local or API)
  - Relevance is based on semantic similarity, not narrative importance — may miss plot-critical moments that are semantically distant
  - Doesn't preserve chronological flow of retrieved memories
- **Who uses it:** SillyTavern Vector Storage, many production RAG systems

---

### Tier 3: Bleeding Edge

---

#### 6. MemGPT / Letta — Virtual Context Management (OS-Inspired)
```
┌─────────────────────────────────────┐
│           CONTEXT WINDOW            │
│  [System] [Core Memory: editable]   │
│  [Recent conversation: last ~20]    │
│  [Retrieved archival memories]      │
│  [PHI]                              │
└─────────────────────────────────────┘
         ↕ self-directed paging ↕
┌─────────────────────────────────────┐
│       RECALL STORAGE (SQLite)       │
│  All conversation history           │
│  Searchable by text/time            │
└─────────────────────────────────────┘
         ↕
┌─────────────────────────────────────┐
│      ARCHIVAL STORAGE (Vector DB)   │
│  Long-term facts, documents        │
│  Searchable by embedding           │
└─────────────────────────────────────┘
```

The LLM **manages its own memory** through tool calls. It can read, write, search, and evict memories autonomously.

- **Key Innovation:** The model is given explicit tools like:
  - `core_memory_replace(key, old_val, new_val)` — edit persistent facts
  - `archival_memory_search(query)` — search long-term storage
  - `conversation_search(query, date_range)` — find past conversations
- **Pros:** Most flexible system possible, model controls what it remembers, true "infinite" memory
- **Cons:** Heavy reliance on tool-use capability, complex implementation, each turn requires multiple LLM calls (inner monologue → memory ops → response), latency

---

#### 7. Temporal Knowledge Graph (Zep/Graphiti, Mem0)
```
┌─────────────────────────────────────┐
│           CONTEXT WINDOW            │
│  [System] [Character]               │
│  [Entity graph snapshot for turn]   │
│  [Recent conversation: last ~20]    │
│  [PHI]                              │
└─────────────────────────────────────┘
         ↕ query ↕
┌─────────────────────────────────────┐
│     TEMPORAL KNOWLEDGE GRAPH        │
│  Entities: Aria, User, Pendant...   │
│  Relationships with timestamps      │
│  "User gave Aria a pendant" (T=45)  │
│  "Aria's trust increased" (T=46)    │
│  Superseded facts tracked           │
└─────────────────────────────────────┘
```

Extract entities and relationships from conversations into a knowledge graph where **time is a first-class dimension**.

- **Key Innovation:** Bi-temporal tracking — knows both when something happened in-story and when the system learned it
- **Pros:** Extremely precise recall, can answer "what did Aria think of the user in chapter 1 vs chapter 5", handles contradictions gracefully
- **Cons:** Requires graph database infrastructure, complex entity extraction, highest implementation complexity

---

## How Industry Leaders Handle It

### SillyTavern (RP-focused)
| Layer | Implementation |
|-------|---------------|
| **Always in context** | System prompt, character card, Author's Note, lorebook entries |
| **Recent history** | Last N messages (configurable, token-budget-based) |
| **Summarized history** | Summarize extension compresses older turns |
| **Semantic recall** | Vector Storage extension retrieves relevant past messages |
| **User control** | `/hide`, `/cut` commands to manually manage context |

### AI Dungeon (Narrative-focused)
| Layer | Implementation |
|-------|---------------|
| **Front Memory** | Critical persistent details — ALWAYS in context |
| **Last Action** | Most recent user action — ALWAYS kept |
| **Memory Bank** | Vector-retrieved summaries from past adventures |
| **Auto Summarization** | Periodic compression of older story turns |
| **Priority Truncation** | When context overflows: trim Story Cards → Memory Bank → Old History |

### Claude/Anthropic (API-level)
| Layer | Implementation |
|-------|---------------|
| **Server-side compaction** | API auto-summarizes when approaching token threshold |
| **Tool result clearing** | Strips verbose tool outputs while keeping call metadata |
| **Multi-agent isolation** | Each sub-agent has its own smaller context |
| **External notes** | Agent writes to `NOTES.md`, reads on demand |

---

## Recommendation for Mythic

Given that Mythic already has a memory extraction pipeline and a memory graph, the optimal approach is a **Hybrid Tiered Architecture** that leverages what's already built.

### Proposed Architecture: 3-Tier Context Stack

```
┌──────────────────────────────────────────────┐
│               PROMPT WINDOW                   │
│                                               │
│  1. [System Prompt + Character Card]          │  ← Always present
│  2. [Lorebook entries]                        │  ← Always present
│  3. [Rolling Summary of older history]        │  ← NEW: Compressed
│  4. [Last N verbatim messages]                │  ← Sliding window
│  5. [Remembered Facts from memory graph]      │  ← Already exists
│  6. [Emotional State]                         │  ← Already exists
│  7. [Post-History Instructions]               │  ← Already exists
└──────────────────────────────────────────────┘
```

### Implementation Strategy

| Phase | What | Effort | Impact |
|-------|------|--------|--------|
| **Phase 1** | Sliding window + token budget | Low | High — immediate fix |
| **Phase 2** | Rolling summary for evicted messages | Medium | High — preserves narrative |
| **Phase 3** | Vector RAG for relevant past recall | High | Medium — precision retrieval |

### Phase 1: Token-Budgeted Sliding Window (Do Now)
- Set a **token budget** for conversation history (e.g., 4096 tokens)
- Walk the chain backwards from the current message
- Include messages until the budget is exhausted
- This alone solves the immediate latency/cost problem
- The existing memory system already preserves key facts from evicted messages

### Phase 2: Rolling Summary (Do Next)
- When messages are evicted from the sliding window, generate a summary
- Store the summary in a `conversation_summaries` table
- Inject the summary as a system message before the recent history
- Update the summary every N evictions (not every turn — batch for efficiency)

### Phase 3: Semantic Retrieval (Future)
- Embed all messages as vectors
- On each turn, retrieve the top-K most semantically relevant past messages
- Inject them alongside the summary for precision recall of specific events
- This is the "pendant from chapter 1" problem solver

---

## Key Design Decisions

> **⚠ WARNING:** **Do NOT summarize on every turn.** This is the #1 mistake. Each summary LLM call adds 2–5 seconds of latency. Batch summarization (every 10 turns or when messages fall off the window) is the way to go.

> **💡 TIP:** **Token budgeting > message counting.** A 50-message window might be 2K tokens or 20K tokens depending on message length. Budget by tokens, not messages.

> **ℹ NOTE:** **Mythic's existing memory pipeline is the secret weapon.** Most apps need to build memory extraction from scratch. Mythic already extracts and stores key facts. The sliding window + memories is a very strong baseline before adding summarization.
