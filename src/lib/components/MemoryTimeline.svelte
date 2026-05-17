<script lang="ts">
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import Icon from './Icon.svelte';

  let { data }: { data: MemoryGraphData } = $props();

  const PALETTE = ['#c4a1ff', '#00f2ff', '#fb7185', '#fbbf24', '#34d399', '#d580ff'];
  const CANON_COLOR = '#daa520';

  // Category → icon mapping
  const CATEGORY_ICONS: Record<string, string> = {
    trait: '🧬',
    event: '⚡',
    relationship: '💫',
    goal: '🎯',
    discovery: '🔮',
    preference: '💭',
    fact: '📋',
  };

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
        const category = catMatch ? catMatch[1].toLowerCase() : 'fact';
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
      >
        <span class="chip-dot" style="background: {CANON_COLOR};"></span>
        Canon
      </button>
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
        <div class="empty-icon-wrap">
          <Icon name="inbox" size={28} />
        </div>
        <p>No memories match the current filters</p>
      </div>
    {:else}
      <div class="tl-track">
        {#each filtered as entry, i (entry.id)}
          <div
            class="tl-row"
            class:canon={entry.isCanon}
            style="--accent: {entry.color}; --delay: {Math.min(i * 30, 500)}ms;"
          >
            <!-- Spine node + line -->
            <div class="spine">
              <div class="spine-node">
                <span class="node-icon">{CATEGORY_ICONS[entry.category] ?? '📋'}</span>
              </div>
              {#if i < filtered.length - 1}
                <div class="spine-line"></div>
              {/if}
            </div>

            <!-- Card -->
            <div class="tl-card">
              <div class="card-accent"></div>
              <div class="card-inner">
                <!-- Header row -->
                <div class="card-head">
                  <div class="head-left">
                    {#if entry.isCanon}
                      <span class="origin-pill canon-pill">
                        <span class="pill-dot canon-dot"></span>
                        Canon
                      </span>
                    {:else}
                      <span class="origin-pill" style="--pill-color: {entry.color};">
                        <span class="pill-dot" style="background: {entry.color};"></span>
                        {entry.conversationTitle}
                      </span>
                    {/if}
                    <span class="category-label">{entry.category}</span>
                  </div>
                  <span class="card-time">{fmt(entry.time)}</span>
                </div>

                <!-- Content -->
                <p class="card-body">{entry.content}</p>

                <!-- Footer badges -->
                <div class="card-foot">
                  <span class="badge" class:auto={entry.source === 'auto'} class:pinned={entry.source !== 'auto'}>
                    {entry.source === 'auto' ? '⚙ Auto' : '📌 Pinned'}
                  </span>
                  {#if entry.version > 1}
                    <span class="badge version">v{entry.version}</span>
                  {/if}
                  {#if entry.parentId}
                    <span class="badge inherited">⛓ Inherited</span>
                  {/if}
                </div>
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
    font-family: 'Inter', -apple-system, sans-serif;
  }

  /* ══════ Filter Bar ══════ */
  .tl-filters {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 24px;
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
    font-family: 'Inter', -apple-system, sans-serif;
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
    font-family: 'Inter', -apple-system, sans-serif;
  }

  .entry-count {
    font-size: 12px;
    font-weight: 700;
    color: #c4a1ff;
    font-family: 'JetBrains Mono', monospace;
    min-width: 20px;
    text-align: right;
  }

  /* ══════ Scroll Area ══════ */
  .tl-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 28px 32px 60px;
  }

  .tl-scroll::-webkit-scrollbar { width: 3px; }
  .tl-scroll::-webkit-scrollbar-thumb { background: rgba(139, 92, 246, 0.12); border-radius: 3px; }

  .tl-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 14px;
    color: #4a4a6a;
  }

  .empty-icon-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    background: rgba(139, 92, 246, 0.04);
    border: 1px solid rgba(139, 92, 246, 0.06);
  }

  .tl-empty p {
    font-size: 12px;
    margin: 0;
  }

  /* ══════ Timeline Track ══════ */
  .tl-track {
    display: flex;
    flex-direction: column;
    max-width: 680px;
    margin: 0 auto;
  }

  .tl-row {
    display: flex;
    gap: 16px;
    animation: slideIn 400ms cubic-bezier(0.16, 1, 0.3, 1) both;
    animation-delay: var(--delay);
  }

  @keyframes slideIn {
    from { opacity: 0; transform: translateY(12px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ══════ Spine ══════ */
  .spine {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 32px;
    flex-shrink: 0;
    padding-top: 4px;
  }

  .spine-node {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: linear-gradient(135deg, #141028, #0e0e1e);
    border: 1.5px solid var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    z-index: 2;
    box-shadow: 0 0 12px color-mix(in srgb, var(--accent) 15%, transparent);
    transition: all 250ms;
  }

  .tl-row:hover .spine-node {
    box-shadow: 0 0 20px color-mix(in srgb, var(--accent) 30%, transparent);
    transform: scale(1.1);
  }

  .node-icon {
    font-size: 12px;
    line-height: 1;
  }

  .spine-line {
    width: 1.5px;
    flex: 1;
    min-height: 12px;
    background: linear-gradient(
      to bottom,
      color-mix(in srgb, var(--accent) 20%, transparent),
      rgba(139, 92, 246, 0.06)
    );
  }

  /* ══════ Card ══════ */
  .tl-card {
    flex: 1;
    display: flex;
    margin-bottom: 10px;
    background: linear-gradient(135deg, rgba(14, 14, 30, 0.6), rgba(10, 10, 26, 0.4));
    border: 1px solid rgba(139, 92, 246, 0.06);
    border-radius: 14px;
    overflow: hidden;
    transition: all 280ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .tl-card:hover {
    background: linear-gradient(135deg, rgba(18, 18, 38, 0.8), rgba(14, 14, 30, 0.6));
    border-color: color-mix(in srgb, var(--accent) 15%, transparent);
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.25),
      0 0 0 1px color-mix(in srgb, var(--accent) 5%, transparent);
    transform: translateY(-1px);
  }

  .card-accent {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(
      180deg,
      var(--accent),
      color-mix(in srgb, var(--accent) 20%, transparent)
    );
  }

  .card-inner {
    flex: 1;
    padding: 12px 16px;
    min-width: 0;
  }

  /* Card Header */
  .card-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    gap: 8px;
  }

  .head-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .origin-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 650;
    color: var(--pill-color);
    background: color-mix(in srgb, var(--pill-color) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--pill-color) 15%, transparent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 200px;
  }

  .canon-pill {
    --pill-color: #daa520;
    color: #daa520;
    background: rgba(218, 165, 32, 0.1);
    border-color: rgba(218, 165, 32, 0.2);
  }

  .pill-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .canon-dot {
    background: #daa520;
    box-shadow: 0 0 4px rgba(218, 165, 32, 0.4);
  }

  .category-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: #4a4a6a;
    font-weight: 600;
    flex-shrink: 0;
  }

  .card-time {
    font-size: 9px;
    color: #3a3a5a;
    font-family: 'JetBrains Mono', monospace;
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Card Body */
  .card-body {
    font-size: 13px;
    color: #c8c8e0;
    line-height: 1.6;
    margin: 0 0 10px;
    letter-spacing: -0.1px;
  }

  /* Card Footer */
  .card-foot {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .badge {
    font-size: 9px;
    padding: 2px 7px;
    border-radius: 5px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .badge.auto {
    background: rgba(0, 242, 255, 0.06);
    color: #00c4cc;
  }

  .badge.pinned {
    background: rgba(16, 185, 129, 0.06);
    color: #34d399;
  }

  .badge.version {
    background: rgba(139, 92, 246, 0.08);
    color: #a78bfa;
  }

  .badge.inherited {
    background: rgba(218, 165, 32, 0.06);
    color: #d4a017;
  }

  /* ══════ Canon Row Special ══════ */
  .tl-row.canon .tl-card {
    border-color: rgba(218, 165, 32, 0.08);
  }

  .tl-row.canon .tl-card:hover {
    border-color: rgba(218, 165, 32, 0.15);
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.25),
      0 0 0 1px rgba(218, 165, 32, 0.06);
  }
</style>
