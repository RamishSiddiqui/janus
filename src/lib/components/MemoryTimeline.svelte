<script lang="ts">
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import Icon from './Icon.svelte';

  let { data }: { data: MemoryGraphData } = $props();

  const PALETTE = ['#c4a1ff', '#00f2ff', '#fb7185', '#fbbf24', '#34d399', '#d580ff'];
  const CANON_COLOR = '#daa520';

  const CATEGORY_ICONS: Record<string, string> = {
    trait: '🧬', event: '⚡', relationship: '💫', goal: '🎯',
    discovery: '🔮', preference: '💭', fact: '📋',
  };

  // ── Lanes ──
  // Canon lane + one lane per conversation
  interface Lane {
    id: string;         // 'canon' or conversation_id
    label: string;
    color: string;
  }

  let lanes = $derived.by(() => {
    const result: Lane[] = [];
    // Canon lane first
    const hasCanon = data.memories.some(m => m.is_canon);
    if (hasCanon) {
      result.push({ id: 'canon', label: 'Canon', color: CANON_COLOR });
    }
    // Conversation lanes
    data.conversations.forEach((c, i) => {
      result.push({ id: c.id, label: c.title, color: PALETTE[i % PALETTE.length] });
    });
    return result;
  });

  // ── Timeline rows ──
  // Each row is a horizontal slice at a point in time
  interface TimelineRow {
    type: 'memory' | 'link';
    time: string;
    // For memory rows
    laneId?: string;
    memoryId?: string;
    content?: string;
    category?: string;
    source?: string;
    version?: number;
    parentId?: string | null;
    // For link rows
    linkLabel?: string;
    fromLaneId?: string;
    toLaneId?: string;
    linkType?: string;
  }

  // Map memory_id → lane_id
  let memLaneMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.memories.forEach(m => {
      map.set(m.id, m.is_canon ? 'canon' : (m.conversation_id ?? 'canon'));
    });
    return map;
  });

  // Map memory_id → conversation_id (for links)
  let memConvMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.memories.forEach(m => {
      if (m.conversation_id) map.set(m.id, m.conversation_id);
    });
    return map;
  });

  let rows = $derived.by(() => {
    const result: TimelineRow[] = [];

    // Memory rows
    data.memories.forEach(m => {
      const catMatch = m.content.match(/^\[(\w+)\]\s*/);
      const category = catMatch ? catMatch[1].toLowerCase() : 'fact';
      const content = catMatch ? m.content.slice(catMatch[0].length) : m.content;

      result.push({
        type: 'memory',
        time: m.created_at,
        laneId: m.is_canon ? 'canon' : (m.conversation_id ?? 'canon'),
        memoryId: m.id,
        content,
        category,
        source: m.source,
        version: m.version,
        parentId: m.parent_id,
      });
    });

    // Link rows — insert after the source memory's time
    data.links.forEach(link => {
      const srcMem = data.memories.find(m => m.id === link.source_memory_id);
      if (!srcMem) return;

      const fromLane = memLaneMap.get(link.source_memory_id) ?? 'canon';
      const toLane = link.target_conversation_id;
      const label = link.link_type === 'sync'
        ? (link.direction === 'two_way' ? '⇄ sync' : '→ sync')
        : (link.direction === 'two_way' ? '⇄ copy' : '→ copy');

      result.push({
        type: 'link',
        time: srcMem.created_at + '_link', // Sort just after the source memory
        fromLaneId: fromLane,
        toLaneId: toLane,
        linkLabel: label,
        linkType: link.link_type,
      });
    });

    // Sort by time
    result.sort((a, b) => a.time.localeCompare(b.time));
    return result;
  });

  function fmtShort(iso: string): string {
    try {
      const d = new Date(iso.replace('_link', ''));
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    } catch { return ''; }
  }

  function laneIndex(laneId: string): number {
    return lanes.findIndex(l => l.id === laneId);
  }

  function laneColor(laneId: string): string {
    return lanes.find(l => l.id === laneId)?.color ?? '#5a5a7a';
  }
</script>

<div class="tl-container">
  {#if lanes.length === 0}
    <div class="tl-empty">
      <div class="empty-icon-wrap"><Icon name="inbox" size={28} /></div>
      <p>No memories to display</p>
    </div>
  {:else}
    <!-- Lane headers -->
    <div class="lane-header" style="--lane-count: {lanes.length};">
      {#each lanes as lane}
        <div class="lane-col">
          <span class="lane-label" style="color: {lane.color};">{lane.label}</span>
          <div class="lane-underline" style="background: {lane.color};"></div>
        </div>
      {/each}
    </div>

    <!-- Timeline body -->
    <div class="tl-scroll">
      <div class="tl-body" style="--lane-count: {lanes.length};">
        <!-- Vertical lane guides -->
        {#each lanes as lane, li}
          <div
            class="lane-guide"
            style="left: calc({li} * (100% / {lanes.length}) + (100% / {lanes.length}) / 2); --guide-color: {lane.color};"
          ></div>
        {/each}

        <!-- Rows -->
        {#each rows as row, ri (row.memoryId ?? `link-${ri}`)}
          {#if row.type === 'memory'}
            {@const li = laneIndex(row.laneId ?? 'canon')}
            {@const color = laneColor(row.laneId ?? 'canon')}
            <div
              class="mem-row"
              style="
                --accent: {color};
                --delay: {Math.min(ri * 25, 600)}ms;
                --lane-offset: calc({li} * (100% / {lanes.length}));
                --lane-width: calc(100% / {lanes.length});
              "
            >
              <div class="mem-cell" style="margin-left: var(--lane-offset); width: var(--lane-width);">
                <div class="mem-dot" style="background: {color}; box-shadow: 0 0 8px {color}44;"></div>
                <div class="mem-card">
                  <div class="card-accent" style="background: linear-gradient(180deg, {color}, {color}33);"></div>
                  <div class="card-inner">
                    <div class="card-head">
                      <span class="cat-icon">{CATEGORY_ICONS[row.category ?? 'fact'] ?? '📋'}</span>
                      <span class="card-content">{row.content}</span>
                    </div>
                    <div class="card-meta">
                      <span class="meta-time">{fmtShort(row.time)}</span>
                      <span class="meta-badge" class:auto={row.source === 'auto'}>
                        {row.source === 'auto' ? 'auto' : 'pinned'}
                      </span>
                      {#if (row.version ?? 1) > 1}
                        <span class="meta-badge version">v{row.version}</span>
                      {/if}
                      {#if row.parentId}
                        <span class="meta-badge inherited">inherited</span>
                      {/if}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          {:else if row.type === 'link'}
            {@const fromIdx = laneIndex(row.fromLaneId ?? 'canon')}
            {@const toIdx = laneIndex(row.toLaneId ?? 'canon')}
            {@const minIdx = Math.min(fromIdx, toIdx)}
            {@const maxIdx = Math.max(fromIdx, toIdx)}
            {@const isSync = row.linkType === 'sync'}
            <div
              class="link-row"
              style="
                --delay: {Math.min(ri * 25, 600)}ms;
                --link-left: calc({minIdx} * (100% / {lanes.length}) + (100% / {lanes.length}) / 2);
                --link-width: calc({(maxIdx - minIdx)} * (100% / {lanes.length}));
                --link-color: {isSync ? 'rgba(0,242,255,0.5)' : 'rgba(139,92,246,0.4)'};
              "
            >
              <div class="link-line">
                <div class="link-dash"></div>
                <span class="link-label">{row.linkLabel}</span>
                <div class="link-dash"></div>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .tl-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: 'Inter', -apple-system, sans-serif;
  }

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
  .tl-empty p { font-size: 12px; margin: 0; }

  /* ══════ Lane Headers ══════ */
  .lane-header {
    display: grid;
    grid-template-columns: repeat(var(--lane-count), 1fr);
    padding: 14px 32px 0;
    gap: 0;
    flex-shrink: 0;
    border-bottom: 1px solid rgba(139, 92, 246, 0.04);
  }

  .lane-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding-bottom: 10px;
  }

  .lane-label {
    font-size: 11px;
    font-weight: 650;
    letter-spacing: -0.2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    padding: 0 8px;
    text-align: center;
  }

  .lane-underline {
    height: 2px;
    width: 48px;
    border-radius: 2px;
    opacity: 0.6;
  }

  /* ══════ Timeline Body ══════ */
  .tl-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 32px 60px;
  }
  .tl-scroll::-webkit-scrollbar { width: 3px; }
  .tl-scroll::-webkit-scrollbar-thumb { background: rgba(139, 92, 246, 0.12); border-radius: 3px; }

  .tl-body {
    position: relative;
    padding-top: 20px;
  }

  /* ══════ Lane Guides (vertical dotted lines) ══════ */
  .lane-guide {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    transform: translateX(-0.5px);
    background: repeating-linear-gradient(
      to bottom,
      color-mix(in srgb, var(--guide-color) 8%, transparent) 0px,
      color-mix(in srgb, var(--guide-color) 8%, transparent) 4px,
      transparent 4px,
      transparent 12px
    );
    pointer-events: none;
    z-index: 0;
  }

  /* ══════ Memory Row ══════ */
  .mem-row {
    position: relative;
    z-index: 1;
    animation: fadeUp 350ms cubic-bezier(0.16, 1, 0.3, 1) both;
    animation-delay: var(--delay);
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .mem-cell {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 8px;
  }

  .mem-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 6px;
    z-index: 2;
    transition: transform 200ms;
  }

  .mem-row:hover .mem-dot {
    transform: scale(1.4);
  }

  .mem-card {
    display: flex;
    flex: 1;
    background: linear-gradient(135deg, rgba(14, 14, 30, 0.7), rgba(10, 10, 26, 0.5));
    border: 1px solid rgba(139, 92, 246, 0.06);
    border-radius: 10px;
    overflow: hidden;
    margin-bottom: 6px;
    transition: all 250ms cubic-bezier(0.16, 1, 0.3, 1);
    min-width: 0;
  }

  .mem-row:hover .mem-card {
    border-color: color-mix(in srgb, var(--accent) 18%, transparent);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2), 0 0 0 1px color-mix(in srgb, var(--accent) 5%, transparent);
    transform: translateY(-1px);
  }

  .card-accent {
    width: 3px;
    flex-shrink: 0;
  }

  .card-inner {
    flex: 1;
    padding: 8px 10px;
    min-width: 0;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    gap: 5px;
    margin-bottom: 4px;
  }

  .cat-icon {
    font-size: 11px;
    flex-shrink: 0;
    line-height: 1.4;
  }

  .card-content {
    font-size: 12px;
    color: #c8c8e0;
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  .meta-time {
    font-size: 9px;
    color: #3a3a5a;
    font-family: 'JetBrains Mono', monospace;
  }

  .meta-badge {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .meta-badge.auto {
    background: rgba(0, 242, 255, 0.06);
    color: #00c4cc;
  }

  .meta-badge.pinned {
    background: rgba(16, 185, 129, 0.06);
    color: #34d399;
  }

  .meta-badge.version {
    background: rgba(139, 92, 246, 0.08);
    color: #a78bfa;
  }

  .meta-badge.inherited {
    background: rgba(218, 165, 32, 0.06);
    color: #d4a017;
  }

  /* ══════ Link Row ══════ */
  .link-row {
    position: relative;
    z-index: 1;
    padding: 6px 0;
    animation: fadeUp 350ms cubic-bezier(0.16, 1, 0.3, 1) both;
    animation-delay: var(--delay);
  }

  .link-line {
    position: absolute;
    left: var(--link-left);
    width: var(--link-width);
    display: flex;
    align-items: center;
    gap: 6px;
    height: 20px;
  }

  .link-dash {
    flex: 1;
    height: 1px;
    background: var(--link-color);
    mask: repeating-linear-gradient(to right, #000 0px, #000 5px, transparent 5px, transparent 9px);
    -webkit-mask: repeating-linear-gradient(to right, #000 0px, #000 5px, transparent 5px, transparent 9px);
  }

  .link-label {
    font-size: 9px;
    font-weight: 650;
    color: var(--link-color);
    white-space: nowrap;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(7, 7, 26, 0.9);
    border: 1px solid var(--link-color);
    letter-spacing: 0.3px;
  }
</style>
