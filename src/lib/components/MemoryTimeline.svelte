<script lang="ts">
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import Icon from './Icon.svelte';

  let { data }: { data: MemoryGraphData } = $props();

  const CONV_COLORS = [
    '#2ea67e', '#5865f2', '#e05260', '#f0b232',
    '#9b59b6', '#1abc9c', '#e67e22', '#3498db',
  ];

  const CANON_COLOR = '#daa520';

  // Build timeline entries sorted by creation date
  interface TimelineEntry {
    id: string;
    content: string;
    source: string;
    version: number;
    isCanon: boolean;
    conversationId: string | null;
    conversationTitle: string;
    color: string;
    category: string;
    time: string;
    parentId: string | null;
  }

  let convColorMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.conversations.forEach((c, i) => {
      map.set(c.id, CONV_COLORS[i % CONV_COLORS.length]);
    });
    return map;
  });

  let convTitleMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.conversations.forEach(c => {
      map.set(c.id, c.title);
    });
    return map;
  });

  let entries = $derived.by(() => {
    return data.memories
      .map(m => {
        // Extract category from content prefix like "[event] Aria..."
        const catMatch = m.content.match(/^\[(\w+)\]\s*/);
        const category = catMatch ? catMatch[1] : 'fact';
        const content = catMatch ? m.content.slice(catMatch[0].length) : m.content;

        return {
          id: m.id,
          content,
          source: m.source,
          version: m.version,
          isCanon: m.is_canon,
          conversationId: m.conversation_id,
          conversationTitle: m.conversation_id ? (convTitleMap.get(m.conversation_id) ?? 'Unknown') : 'Canon',
          color: m.is_canon ? CANON_COLOR : (m.conversation_id ? (convColorMap.get(m.conversation_id) ?? '#666') : CANON_COLOR),
          category,
          time: m.created_at,
          parentId: m.parent_id,
        } as TimelineEntry;
      })
      .sort((a, b) => a.time.localeCompare(b.time));
  });

  // Filter state
  let filterConv: string | null = $state(null);
  let filterCategory: string | null = $state(null);

  let categories = $derived([...new Set(entries.map(e => e.category))]);

  let filteredEntries = $derived.by(() => {
    let result = entries;
    if (filterConv) {
      result = result.filter(e => e.conversationId === filterConv || (filterConv === 'canon' && e.isCanon));
    }
    if (filterCategory) {
      result = result.filter(e => e.category === filterCategory);
    }
    return result;
  });

  function formatTime(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) +
             ' ' + d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    } catch {
      return iso;
    }
  }
</script>

<div class="timeline-container">
  <!-- Filters -->
  <div class="timeline-filters">
    <div class="filter-group">
      <label>Conversation</label>
      <select bind:value={filterConv}>
        <option value={null}>All</option>
        <option value="canon">Canon Only</option>
        {#each data.conversations as conv}
          <option value={conv.id}>{conv.title}</option>
        {/each}
      </select>
    </div>
    <div class="filter-group">
      <label>Category</label>
      <select bind:value={filterCategory}>
        <option value={null}>All</option>
        {#each categories as cat}
          <option value={cat}>{cat}</option>
        {/each}
      </select>
    </div>
    <span class="entry-count">{filteredEntries.length} memor{filteredEntries.length === 1 ? 'y' : 'ies'}</span>
  </div>

  <!-- Timeline -->
  <div class="timeline-scroll">
    {#if filteredEntries.length === 0}
      <div class="empty">
        <Icon name="clock" size={36} />
        <p>No memories match the current filters</p>
      </div>
    {:else}
      <div class="timeline-track">
        {#each filteredEntries as entry, i (entry.id)}
          <div class="timeline-entry">
            <!-- Dot + vertical line -->
            <div class="entry-track">
              <div class="entry-dot" style="background: {entry.color}; box-shadow: 0 0 8px {entry.color}44;"></div>
              {#if i < filteredEntries.length - 1}
                <div class="entry-line"></div>
              {/if}
            </div>

            <!-- Content card -->
            <div class="entry-card" style="border-left: 3px solid {entry.color};">
              <div class="entry-header">
                <span class="entry-conv" style="color: {entry.color};">
                  {entry.isCanon ? '🧠 Canon' : `💬 ${entry.conversationTitle}`}
                </span>
                <span class="entry-time">{formatTime(entry.time)}</span>
              </div>
              <div class="entry-content">{entry.content}</div>
              <div class="entry-meta">
                <span class="badge cat">{entry.category}</span>
                <span class="badge source">{entry.source === 'auto' ? '🤖 Auto' : '📌 Pinned'}</span>
                {#if entry.version > 1}
                  <span class="badge version">v{entry.version}</span>
                {/if}
                {#if entry.parentId}
                  <span class="badge linked">🔗 Inherited</span>
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
  .timeline-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .timeline-filters {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 24px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--surface-secondary);
    flex-shrink: 0;
  }

  .filter-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .filter-group label {
    font-size: 11px;
    color: var(--text-tertiary);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .filter-group select {
    background: var(--surface-primary);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
  }

  .entry-count {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .timeline-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    color: var(--text-tertiary);
  }

  .empty p {
    font-size: 14px;
    margin: 0;
  }

  .timeline-track {
    display: flex;
    flex-direction: column;
    max-width: 700px;
    margin: 0 auto;
  }

  .timeline-entry {
    display: flex;
    gap: 16px;
    min-height: 60px;
  }

  .entry-track {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 16px;
    flex-shrink: 0;
  }

  .entry-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
    z-index: 1;
  }

  .entry-line {
    width: 2px;
    flex: 1;
    background: var(--border-subtle);
    min-height: 20px;
  }

  .entry-card {
    flex: 1;
    padding: 12px 16px;
    background: var(--surface-secondary);
    border-radius: 8px;
    margin-bottom: 12px;
    transition: background 0.2s ease;
  }

  .entry-card:hover {
    background: var(--surface-tertiary);
  }

  .entry-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .entry-conv {
    font-size: 12px;
    font-weight: 600;
  }

  .entry-time {
    font-size: 11px;
    color: var(--text-tertiary);
  }

  .entry-content {
    font-size: 13px;
    color: var(--text-primary);
    line-height: 1.5;
    margin-bottom: 8px;
  }

  .entry-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .badge {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 4px;
    font-weight: 500;
  }

  .badge.cat {
    background: rgba(255,255,255,0.06);
    color: var(--text-secondary);
    text-transform: capitalize;
  }

  .badge.source {
    background: rgba(46, 166, 126, 0.1);
    color: #2ea67e;
  }

  .badge.version {
    background: rgba(88, 101, 242, 0.1);
    color: #5865f2;
  }

  .badge.linked {
    background: rgba(240, 178, 50, 0.1);
    color: #f0b232;
  }
</style>
