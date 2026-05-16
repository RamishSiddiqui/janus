<script lang="ts">
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import Icon from './Icon.svelte';

  let { data }: { data: MemoryGraphData } = $props();

  const PALETTE = ['#c4a1ff', '#00f2ff', '#fb7185', '#fbbf24', '#34d399', '#d580ff'];
  const CANON_COLOR = '#daa520';

  interface TimelineEntry {
    id: string;
    content: string;
    source: string;
    version: number;
    isCanon: boolean;
    conversationTitle: string;
    color: string;
    category: string;
    time: string;
    parentId: string | null;
  }

  let convColorMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.conversations.forEach((c, i) => map.set(c.id, PALETTE[i % PALETTE.length]));
    return map;
  });

  let convTitleMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.conversations.forEach(c => map.set(c.id, c.title));
    return map;
  });

  let entries = $derived.by(() => {
    return data.memories
      .map(m => {
        const catMatch = m.content.match(/^\[(\w+)\]\s*/);
        const category = catMatch ? catMatch[1] : 'fact';
        const content = catMatch ? m.content.slice(catMatch[0].length) : m.content;
        return {
          id: m.id,
          content,
          source: m.source,
          version: m.version,
          isCanon: m.is_canon,
          conversationTitle: m.conversation_id ? (convTitleMap.get(m.conversation_id) ?? 'Unknown') : 'Canon',
          color: m.is_canon ? CANON_COLOR : (m.conversation_id ? (convColorMap.get(m.conversation_id) ?? '#666') : CANON_COLOR),
          category,
          time: m.created_at,
          parentId: m.parent_id,
        } as TimelineEntry;
      })
      .sort((a, b) => a.time.localeCompare(b.time));
  });

  // Filters
  let filterConv: string | null = $state(null);
  let filterCategory: string | null = $state(null);
  let categories = $derived([...new Set(entries.map(e => e.category))]);

  let filtered = $derived.by(() => {
    let result = entries;
    if (filterConv) {
      result = result.filter(e => (filterConv === 'canon' && e.isCanon) || e.conversationTitle === filterConv);
    }
    if (filterCategory) {
      result = result.filter(e => e.category === filterCategory);
    }
    return result;
  });

  function fmt(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) +
             ', ' + d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    } catch { return iso; }
  }
</script>

<div class="tl-container">
  <!-- Filter bar -->
  <div class="tl-filters">
    <div class="filter-chip-group">
      <button
        class="filter-chip"
        class:active={!filterConv}
        onclick={() => filterConv = null}
      >All</button>
      <button
        class="filter-chip canon"
        class:active={filterConv === 'canon'}
        onclick={() => filterConv = filterConv === 'canon' ? null : 'canon'}
      >Canon</button>
      {#each data.conversations as conv, i}
        <button
          class="filter-chip"
          class:active={filterConv === conv.title}
          style="--chip-color: {PALETTE[i % PALETTE.length]}"
          onclick={() => filterConv = filterConv === conv.title ? null : conv.title}
        >
          <span class="chip-dot" style="background: {PALETTE[i % PALETTE.length]}"></span>
          {conv.title.length > 20 ? conv.title.slice(0, 18) + '…' : conv.title}
        </button>
      {/each}
    </div>
    <div class="filter-right">
      {#if categories.length > 1}
        <select class="cat-select" bind:value={filterCategory}>
          <option value={null}>All types</option>
          {#each categories as cat}
            <option value={cat}>{cat}</option>
          {/each}
        </select>
      {/if}
      <span class="entry-count">{filtered.length}</span>
    </div>
  </div>

  <!-- Timeline body -->
  <div class="tl-scroll">
    {#if filtered.length === 0}
      <div class="tl-empty">
        <Icon name="inbox" size={28} />
        <p>No memories match the current filters</p>
      </div>
    {:else}
      <div class="tl-track">
        {#each filtered as entry, i (entry.id)}
          <div class="tl-row" style="--delay: {Math.min(i * 40, 400)}ms;">
            <!-- Track -->
            <div class="row-track">
              <div class="row-dot" style="background: {entry.color}; box-shadow: 0 0 8px {entry.color}44;"></div>
              {#if i < filtered.length - 1}
                <div class="row-line"></div>
              {/if}
            </div>

            <!-- Card -->
            <div class="row-card">
              <div class="card-header">
                <div class="card-origin" style="color: {entry.color};">
                  {#if entry.isCanon}
                    <span class="origin-badge canon-badge">🧠 Canon</span>
                  {:else}
                    <span class="origin-badge" style="background: {entry.color}12; border-color: {entry.color}25;">
                      💬 {entry.conversationTitle}
                    </span>
                  {/if}
                </div>
                <span class="card-time">{fmt(entry.time)}</span>
              </div>

              <p class="card-content">{entry.content}</p>

              <div class="card-tags">
                <span class="tag type">{entry.category}</span>
                <span class="tag source" class:auto={entry.source === 'auto'}>
                  {entry.source === 'auto' ? '🤖 Auto' : '📌 Pinned'}
                </span>
                {#if entry.version > 1}
                  <span class="tag version">v{entry.version}</span>
                {/if}
                {#if entry.parentId}
                  <span class="tag inherited">⛓ Inherited</span>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .tl-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* ── Filter bar ── */
  .tl-filters {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-bottom: 1px solid rgba(139, 92, 246, 0.05);
    flex-shrink: 0;
    gap: 12px;
    overflow-x: auto;
  }

  .tl-filters::-webkit-scrollbar { height: 0; }

  .filter-chip-group {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .filter-chip {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    font-size: 11px;
    font-weight: 600;
    color: #5a5a7a;
    background: rgba(14, 14, 30, 0.5);
    border: 1px solid rgba(139, 92, 246, 0.06);
    border-radius: 8px;
    cursor: pointer;
    transition: all 200ms;
    white-space: nowrap;
    font-family: var(--font-body);
  }

  .filter-chip:hover {
    border-color: rgba(139, 92, 246, 0.15);
    color: #8b8ba7;
  }

  .filter-chip.active {
    background: rgba(139, 92, 246, 0.1);
    border-color: rgba(139, 92, 246, 0.2);
    color: #c4a1ff;
  }

  .filter-chip.canon.active {
    background: rgba(218, 165, 32, 0.1);
    border-color: rgba(218, 165, 32, 0.25);
    color: #daa520;
  }

  .chip-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .filter-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .cat-select {
    background: rgba(14, 14, 30, 0.5);
    border: 1px solid rgba(139, 92, 246, 0.08);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 11px;
    color: #8b8ba7;
    cursor: pointer;
    font-family: var(--font-body);
  }

  .entry-count {
    font-size: 12px;
    font-weight: 700;
    color: #c4a1ff;
    font-family: var(--font-mono);
    min-width: 20px;
    text-align: right;
  }

  /* ── Scroll area ── */
  .tl-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px 40px;
  }

  .tl-scroll::-webkit-scrollbar { width: 3px; }
  .tl-scroll::-webkit-scrollbar-thumb { background: rgba(139, 92, 246, 0.12); border-radius: 3px; }

  .tl-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 10px;
    color: #4a4a6a;
  }

  .tl-empty p {
    font-size: 12px;
    margin: 0;
  }

  /* ── Timeline track ── */
  .tl-track {
    display: flex;
    flex-direction: column;
    max-width: 640px;
    margin: 0 auto;
  }

  .tl-row {
    display: flex;
    gap: 14px;
    animation: fadeSlide 350ms ease both;
    animation-delay: var(--delay);
  }

  @keyframes fadeSlide {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .row-track {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 12px;
    flex-shrink: 0;
    padding-top: 2px;
  }

  .row-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    z-index: 1;
    transition: transform 200ms;
  }

  .tl-row:hover .row-dot {
    transform: scale(1.3);
  }

  .row-line {
    width: 1.5px;
    flex: 1;
    background: linear-gradient(to bottom, rgba(139, 92, 246, 0.1), transparent);
    min-height: 16px;
  }

  /* ── Card ── */
  .row-card {
    flex: 1;
    padding: 12px 16px;
    background: rgba(14, 14, 30, 0.4);
    border: 1px solid rgba(139, 92, 246, 0.05);
    border-radius: 12px;
    margin-bottom: 8px;
    transition: all 200ms;
  }

  .row-card:hover {
    background: rgba(14, 14, 30, 0.6);
    border-color: rgba(139, 92, 246, 0.1);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .card-origin { font-size: 11px; }

  .origin-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 10px;
    border: 1px solid transparent;
  }

  .canon-badge {
    background: rgba(218, 165, 32, 0.1);
    border-color: rgba(218, 165, 32, 0.2);
    color: #daa520;
  }

  .card-time {
    font-size: 10px;
    color: #3a3a5a;
    font-family: var(--font-mono);
  }

  .card-content {
    font-size: 13px;
    color: #c8c8e0;
    line-height: 1.55;
    margin: 0 0 8px;
  }

  .card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .tag {
    font-size: 9px;
    padding: 2px 7px;
    border-radius: 5px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .tag.type {
    background: rgba(139, 92, 246, 0.08);
    color: #8b8ba7;
    text-transform: capitalize;
  }

  .tag.source {
    background: rgba(16, 185, 129, 0.08);
    color: #34d399;
  }

  .tag.source.auto {
    background: rgba(0, 242, 255, 0.08);
    color: #00f2ff;
  }

  .tag.version {
    background: rgba(139, 92, 246, 0.1);
    color: #c4a1ff;
  }

  .tag.inherited {
    background: rgba(218, 165, 32, 0.08);
    color: #daa520;
  }
</style>
