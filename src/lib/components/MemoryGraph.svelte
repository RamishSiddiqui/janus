<script lang="ts">
  import { SvelteFlow, Controls, Background, MiniMap } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import type { Node, Edge } from '@xyflow/svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import { writable } from 'svelte/store';

  let { data, onRefresh }: { data: MemoryGraphData; onRefresh: () => void } = $props();

  // Conversation color palette (muted, premium palette)
  const CONV_COLORS = [
    '#2ea67e', '#5865f2', '#e05260', '#f0b232',
    '#9b59b6', '#1abc9c', '#e67e22', '#3498db',
    '#e74c3c', '#2ecc71', '#9b59b6', '#f39c12',
  ];

  const CANON_COLOR = '#daa520';

  function getConvColor(index: number): string {
    return CONV_COLORS[index % CONV_COLORS.length];
  }

  // Build Svelte Flow nodes and edges from graph data
  function buildGraph(g: MemoryGraphData): { nodes: Node[]; edges: Edge[] } {
    const nodes: Node[] = [];
    const edges: Edge[] = [];

    // Map conversation IDs to colors
    const convColorMap = new Map<string, string>();
    g.conversations.forEach((c, i) => {
      convColorMap.set(c.id, getConvColor(i));
    });

    // 1. Character root node (center)
    nodes.push({
      id: `char-${g.character_id}`,
      type: 'default',
      position: { x: 400, y: 50 },
      data: { label: `🧠 ${g.character_name}` },
      style: `background: ${CANON_COLOR}; color: #1a1a1a; border-radius: 16px; padding: 12px 20px; font-weight: 700; font-size: 14px; border: 2px solid #b8860b;`,
    });

    // 2. Conversation branch nodes
    const convSpacing = 280;
    const convStartX = 400 - ((g.conversations.length - 1) * convSpacing) / 2;

    g.conversations.forEach((conv, i) => {
      const x = convStartX + i * convSpacing;
      const color = convColorMap.get(conv.id) ?? '#666';

      nodes.push({
        id: `conv-${conv.id}`,
        type: 'default',
        position: { x, y: 180 },
        data: { label: `💬 ${conv.title} (${conv.memory_count})` },
        style: `background: ${color}22; color: ${color}; border: 2px solid ${color}; border-radius: 12px; padding: 10px 16px; font-size: 13px; font-weight: 600;`,
      });

      // Edge: character → conversation
      edges.push({
        id: `e-char-conv-${conv.id}`,
        source: `char-${g.character_id}`,
        target: `conv-${conv.id}`,
        style: `stroke: ${CANON_COLOR}; stroke-width: 2px;`,
        type: 'smoothstep',
      });
    });

    // 3. Memory nodes
    // Group by conversation, then lay out vertically
    const convMemories = new Map<string | null, typeof g.memories>();
    const canonMemories = g.memories.filter(m => m.is_canon);
    const otherMemories = g.memories.filter(m => !m.is_canon);

    for (const m of otherMemories) {
      const key = m.conversation_id;
      if (!convMemories.has(key)) convMemories.set(key, []);
      convMemories.get(key)!.push(m);
    }

    // Canon memories — arranged under the character node
    canonMemories.forEach((m, i) => {
      const x = 400 - ((canonMemories.length - 1) * 200) / 2 + i * 200;
      const truncated = m.content.length > 60 ? m.content.slice(0, 57) + '...' : m.content;
      const badge = m.source === 'auto' ? '🤖' : '📌';
      const vBadge = m.version > 1 ? ` v${m.version}` : '';

      nodes.push({
        id: `mem-${m.id}`,
        type: 'default',
        position: { x, y: 120 },
        data: { label: `${badge} ${truncated}${vBadge}` },
        style: `background: ${CANON_COLOR}11; color: #daa520; border: 1px solid ${CANON_COLOR}44; border-radius: 8px; padding: 8px 12px; font-size: 11px; max-width: 180px;`,
      });

      edges.push({
        id: `e-char-mem-${m.id}`,
        source: `char-${g.character_id}`,
        target: `mem-${m.id}`,
        style: `stroke: ${CANON_COLOR}88; stroke-width: 1px;`,
        type: 'smoothstep',
      });
    });

    // Conversation memories — arranged under each conversation
    g.conversations.forEach((conv, convIndex) => {
      const mems = convMemories.get(conv.id) ?? [];
      const color = convColorMap.get(conv.id) ?? '#666';
      const baseX = convStartX + convIndex * convSpacing;

      mems.forEach((m, memIndex) => {
        const x = baseX - ((mems.length - 1) * 80) / 2 + memIndex * 80;
        const y = 320 + memIndex * 80;
        const truncated = m.content.length > 50 ? m.content.slice(0, 47) + '...' : m.content;
        const badge = m.source === 'auto' ? '🤖' : '📌';
        const vBadge = m.version > 1 ? ` v${m.version}` : '';

        nodes.push({
          id: `mem-${m.id}`,
          type: 'default',
          position: { x, y },
          data: { label: `${badge} ${truncated}${vBadge}` },
          style: `background: ${color}11; color: ${color}; border: 1px solid ${color}44; border-radius: 8px; padding: 8px 12px; font-size: 11px; max-width: 180px;`,
        });

        // Edge: conversation → memory or parent → memory
        const sourceId = m.parent_id
          ? `mem-${m.parent_id}`
          : `conv-${conv.id}`;

        edges.push({
          id: `e-mem-${m.id}`,
          source: sourceId,
          target: `mem-${m.id}`,
          style: `stroke: ${color}66; stroke-width: 1px;`,
          type: 'smoothstep',
        });
      });
    });

    // 4. Sharing link edges (dashed)
    g.links.forEach((link) => {
      const dashStyle = link.link_type === 'sync'
        ? 'stroke-dasharray: 5 3; animation: dash 1s linear infinite;'
        : 'stroke-dasharray: 8 4;';

      edges.push({
        id: `link-${link.id}`,
        source: `mem-${link.source_memory_id}`,
        target: link.linked_memory_id ? `mem-${link.linked_memory_id}` : `conv-${link.target_conversation_id}`,
        style: `stroke: #888; stroke-width: 1.5px; ${dashStyle}`,
        type: 'smoothstep',
        animated: link.link_type === 'sync',
        label: link.link_type === 'sync'
          ? `🔄 ${link.direction === 'two_way' ? '↔' : '→'} ${link.sync_mode}`
          : `📋 copy`,
      });
    });

    return { nodes, edges };
  }

  let graphResult = $derived(buildGraph(data));
  let nodesStore = writable(graphResult.nodes);
  let edgesStore = writable(graphResult.edges);

  // Update stores when data changes
  $effect(() => {
    const result = buildGraph(data);
    nodesStore.set(result.nodes);
    edgesStore.set(result.edges);
  });
</script>

<div class="graph-wrapper">
  <SvelteFlow
    nodes={nodesStore}
    edges={edgesStore}
    fitView
    minZoom={0.2}
    maxZoom={2}
  >
    <Controls />
    <Background variant="dots" gap={20} size={1} />
    <MiniMap
      style="background: var(--surface-secondary); border: 1px solid var(--border-subtle); border-radius: 8px;"
    />
  </SvelteFlow>

  <!-- Legend -->
  <div class="legend">
    <div class="legend-item">
      <span class="legend-dot" style="background: {CANON_COLOR}"></span>
      Canon
    </div>
    {#each data.conversations as conv, i}
      <div class="legend-item">
        <span class="legend-dot" style="background: {getConvColor(i)}"></span>
        {conv.title}
      </div>
    {/each}
    <div class="legend-item">
      <span class="legend-line solid"></span>
      Inheritance
    </div>
    <div class="legend-item">
      <span class="legend-line dashed"></span>
      Shared
    </div>
  </div>
</div>

<style>
  .graph-wrapper {
    width: 100%;
    height: 100%;
    position: relative;
  }

  .graph-wrapper :global(.svelte-flow) {
    background: var(--surface-primary) !important;
  }

  .graph-wrapper :global(.svelte-flow__node) {
    cursor: pointer;
  }

  .graph-wrapper :global(.svelte-flow__controls) {
    background: var(--surface-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    overflow: hidden;
  }

  .graph-wrapper :global(.svelte-flow__controls button) {
    background: var(--surface-secondary);
    color: var(--text-secondary);
    border: none;
    border-bottom: 1px solid var(--border-subtle);
  }

  .graph-wrapper :global(.svelte-flow__controls button:hover) {
    background: var(--surface-tertiary);
  }

  .legend {
    position: absolute;
    bottom: 16px;
    left: 16px;
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    padding: 10px 16px;
    background: var(--surface-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 10px;
    font-size: 11px;
    color: var(--text-secondary);
    z-index: 10;
    backdrop-filter: blur(8px);
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .legend-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .legend-line {
    width: 20px;
    height: 2px;
    flex-shrink: 0;
  }

  .legend-line.solid {
    background: var(--text-tertiary);
  }

  .legend-line.dashed {
    background: repeating-linear-gradient(
      90deg,
      var(--text-tertiary) 0px,
      var(--text-tertiary) 4px,
      transparent 4px,
      transparent 7px
    );
  }
</style>
