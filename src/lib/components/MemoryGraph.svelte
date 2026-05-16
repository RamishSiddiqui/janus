<script lang="ts">
  import { SvelteFlow, Controls, Background, MiniMap } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import type { Node, Edge } from '@xyflow/svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';

  let { data, onRefresh }: { data: MemoryGraphData; onRefresh: () => void } = $props();

  /* ── Palette ── */
  const PALETTE = [
    { bg: 'rgba(139,92,246,0.12)',  border: 'rgba(139,92,246,0.3)',  text: '#c4a1ff', edge: 'rgba(139,92,246,0.4)' },
    { bg: 'rgba(0,242,255,0.10)',   border: 'rgba(0,242,255,0.25)',  text: '#00f2ff', edge: 'rgba(0,242,255,0.35)' },
    { bg: 'rgba(244,63,94,0.10)',   border: 'rgba(244,63,94,0.25)',  text: '#fb7185', edge: 'rgba(244,63,94,0.35)' },
    { bg: 'rgba(245,158,11,0.10)',  border: 'rgba(245,158,11,0.25)', text: '#fbbf24', edge: 'rgba(245,158,11,0.35)' },
    { bg: 'rgba(16,185,129,0.10)',  border: 'rgba(16,185,129,0.25)', text: '#34d399', edge: 'rgba(16,185,129,0.35)' },
    { bg: 'rgba(191,64,255,0.10)',  border: 'rgba(191,64,255,0.25)', text: '#d580ff', edge: 'rgba(191,64,255,0.35)' },
  ];

  const CANON = {
    bg: 'linear-gradient(135deg, rgba(218,165,32,0.18), rgba(218,165,32,0.06))',
    border: 'rgba(218,165,32,0.35)',
    text: '#fbbf24',
    edge: 'rgba(218,165,32,0.5)',
  };

  const ROOT = {
    bg: 'linear-gradient(135deg, rgba(139,92,246,0.2), rgba(191,64,255,0.1))',
    border: 'rgba(139,92,246,0.35)',
    text: '#e8e0ff',
  };

  function pal(i: number) { return PALETTE[i % PALETTE.length]; }

  /* ── Build graph ── */
  function buildGraph(g: MemoryGraphData): { nodes: Node[]; edges: Edge[] } {
    const nodes: Node[] = [];
    const edges: Edge[] = [];
    const convColorMap = new Map<string, typeof PALETTE[0]>();
    g.conversations.forEach((c, i) => convColorMap.set(c.id, pal(i)));

    // Root node
    nodes.push({
      id: `char-${g.character_id}`,
      type: 'default',
      position: { x: 400, y: 0 },
      data: { label: `🧠  ${g.character_name}` },
      style: `background: ${ROOT.bg}; color: ${ROOT.text}; border: 1.5px solid ${ROOT.border}; border-radius: 14px; padding: 14px 24px; font-weight: 700; font-size: 14px; font-family: Inter, sans-serif; box-shadow: 0 0 24px rgba(139,92,246,0.1); backdrop-filter: blur(8px);`,
    });

    // Conversation branches
    const spread = 300;
    const startX = 400 - ((g.conversations.length - 1) * spread) / 2;
    g.conversations.forEach((conv, i) => {
      const x = startX + i * spread;
      const p = convColorMap.get(conv.id)!;
      nodes.push({
        id: `conv-${conv.id}`,
        type: 'default',
        position: { x, y: 140 },
        data: { label: `💬  ${conv.title}` },
        style: `background: ${p.bg}; color: ${p.text}; border: 1.5px solid ${p.border}; border-radius: 12px; padding: 10px 18px; font-size: 12px; font-weight: 600; font-family: Inter, sans-serif;`,
      });
      edges.push({
        id: `e-root-${conv.id}`,
        source: `char-${g.character_id}`,
        target: `conv-${conv.id}`,
        style: `stroke: ${ROOT.border}; stroke-width: 2px;`,
        type: 'smoothstep',
      });
    });

    // Memories
    const convMems = new Map<string | null, typeof g.memories>();
    const canon = g.memories.filter(m => m.is_canon);
    const scoped = g.memories.filter(m => !m.is_canon);
    for (const m of scoped) {
      const k = m.conversation_id;
      if (!convMems.has(k)) convMems.set(k, []);
      convMems.get(k)!.push(m);
    }

    // Canon memories — fan out under root
    canon.forEach((m, i) => {
      const fanAngle = ((i - (canon.length - 1) / 2) / Math.max(canon.length - 1, 1)) * 0.6;
      const x = 400 + Math.sin(fanAngle) * 220;
      const y = -80 - Math.cos(fanAngle) * 60;
      const trunc = m.content.length > 55 ? m.content.slice(0, 52) + '…' : m.content;
      const badge = m.source === 'auto' ? '🤖' : '📌';
      const ver = m.version > 1 ? `  v${m.version}` : '';

      nodes.push({
        id: `mem-${m.id}`,
        type: 'default',
        position: { x, y },
        data: { label: `${badge} ${trunc}${ver}` },
        style: `background: ${CANON.bg}; color: ${CANON.text}; border: 1px solid ${CANON.border}; border-radius: 10px; padding: 8px 14px; font-size: 10px; max-width: 200px; line-height: 1.4; font-family: Inter, sans-serif;`,
      });
      edges.push({
        id: `e-canon-${m.id}`,
        source: `char-${g.character_id}`,
        target: `mem-${m.id}`,
        style: `stroke: ${CANON.edge}; stroke-width: 1.5px;`,
        type: 'smoothstep',
      });
    });

    // Conversation-scoped memories — cascade under each branch
    g.conversations.forEach((conv, ci) => {
      const mems = convMems.get(conv.id) ?? [];
      const p = convColorMap.get(conv.id)!;
      const baseX = startX + ci * spread;

      mems.forEach((m, mi) => {
        const offset = (mi - (mems.length - 1) / 2) * 60;
        const x = baseX + offset;
        const y = 280 + mi * 70;
        const trunc = m.content.length > 50 ? m.content.slice(0, 47) + '…' : m.content;
        const badge = m.source === 'auto' ? '🤖' : '📌';
        const ver = m.version > 1 ? `  v${m.version}` : '';

        nodes.push({
          id: `mem-${m.id}`,
          type: 'default',
          position: { x, y },
          data: { label: `${badge} ${trunc}${ver}` },
          style: `background: ${p.bg}; color: ${p.text}; border: 1px solid ${p.border}; border-radius: 10px; padding: 8px 14px; font-size: 10px; max-width: 190px; line-height: 1.4; font-family: Inter, sans-serif; opacity: 0.92;`,
        });

        const parentId = m.parent_id ? `mem-${m.parent_id}` : `conv-${conv.id}`;
        edges.push({
          id: `e-mem-${m.id}`,
          source: parentId,
          target: `mem-${m.id}`,
          style: `stroke: ${p.edge}; stroke-width: 1px;`,
          type: 'smoothstep',
        });
      });
    });

    // Sharing links (dashed / animated)
    g.links.forEach((link) => {
      const isSync = link.link_type === 'sync';
      const isTwoWay = link.direction === 'two_way';
      const arrow = isTwoWay ? '↔' : '→';
      const lbl = isSync ? `sync ${arrow}` : `copy ${arrow}`;

      edges.push({
        id: `link-${link.id}`,
        source: `mem-${link.source_memory_id}`,
        target: link.linked_memory_id ? `mem-${link.linked_memory_id}` : `conv-${link.target_conversation_id}`,
        style: `stroke: rgba(0,242,255,0.4); stroke-width: 1.5px; stroke-dasharray: ${isSync ? '4 3' : '8 5'};`,
        type: 'smoothstep',
        animated: isSync,
        label: lbl,
        labelStyle: 'font-size: 9px; fill: #5a5a7a; font-family: Inter, sans-serif;',
        labelBgStyle: 'fill: rgba(7,7,26,0.8); rx: 4; ry: 4;',
        labelBgPadding: [4, 6] as [number, number],
      });
    });

    return { nodes, edges };
  }

  let nodes: Node[] = $state.raw([]);
  let edges: Edge[] = $state.raw([]);

  $effect(() => {
    const result = buildGraph(data);
    nodes = result.nodes;
    edges = result.edges;
  });
</script>

<div class="graph-wrap">
  <SvelteFlow
    bind:nodes
    bind:edges
    fitView
    fitViewOptions={{ padding: 0.3 }}
    minZoom={0.15}
    maxZoom={2.5}
    defaultEdgeOptions={{ type: 'smoothstep' }}
  >
    <Controls position="bottom-left" />
    <Background variant="dots" gap={24} size={0.6} color="rgba(139,92,246,0.06)" />
    <MiniMap
      pannable
      zoomable
      position="bottom-right"
    />
  </SvelteFlow>

  <!-- Legend overlay -->
  <div class="legend">
    <div class="legend-item">
      <span class="dot canon"></span>
      Canon
    </div>
    {#each data.conversations as conv, i}
      <div class="legend-item">
        <span class="dot" style="background: {pal(i).text};"></span>
        {conv.title.length > 18 ? conv.title.slice(0, 16) + '…' : conv.title}
      </div>
    {/each}
    <div class="legend-sep"></div>
    <div class="legend-item">
      <span class="line solid"></span>
      Inherit
    </div>
    <div class="legend-item">
      <span class="line dashed"></span>
      Shared
    </div>
  </div>
</div>

<style>
  .graph-wrap {
    width: 100%;
    height: 100%;
    position: relative;
  }

  /* ── SvelteFlow overrides ── */
  .graph-wrap :global(.svelte-flow) {
    background: var(--surface-inverse, #07071a) !important;
  }

  .graph-wrap :global(.svelte-flow__node) {
    cursor: grab;
    transition: box-shadow 200ms ease;
  }

  .graph-wrap :global(.svelte-flow__node:hover) {
    box-shadow: 0 0 20px rgba(139, 92, 246, 0.15) !important;
  }

  .graph-wrap :global(.svelte-flow__node.selected) {
    box-shadow: 0 0 0 2px rgba(139, 92, 246, 0.4), 0 0 24px rgba(139, 92, 246, 0.15) !important;
  }

  .graph-wrap :global(.svelte-flow__handle) {
    width: 6px;
    height: 6px;
    background: rgba(139, 92, 246, 0.3);
    border: 1px solid rgba(139, 92, 246, 0.15);
  }

  .graph-wrap :global(.svelte-flow__edge-path) {
    filter: drop-shadow(0 0 3px rgba(139, 92, 246, 0.1));
  }

  /* Controls */
  .graph-wrap :global(.svelte-flow__controls) {
    background: rgba(10, 10, 26, 0.85) !important;
    backdrop-filter: blur(12px);
    border: 1px solid rgba(139, 92, 246, 0.08) !important;
    border-radius: 10px !important;
    overflow: hidden;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4) !important;
  }

  .graph-wrap :global(.svelte-flow__controls button) {
    background: transparent !important;
    color: #5a5a7a !important;
    border: none !important;
    border-bottom: 1px solid rgba(139, 92, 246, 0.06) !important;
    width: 32px !important;
    height: 32px !important;
  }

  .graph-wrap :global(.svelte-flow__controls button:hover) {
    background: rgba(139, 92, 246, 0.08) !important;
    color: #c4a1ff !important;
  }

  .graph-wrap :global(.svelte-flow__controls button svg) {
    fill: currentColor !important;
  }

  /* MiniMap */
  .graph-wrap :global(.svelte-flow__minimap) {
    background: rgba(10, 10, 26, 0.85) !important;
    backdrop-filter: blur(12px);
    border: 1px solid rgba(139, 92, 246, 0.08) !important;
    border-radius: 10px !important;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4) !important;
    overflow: hidden;
  }

  .graph-wrap :global(.svelte-flow__minimap-mask) {
    fill: rgba(139, 92, 246, 0.06) !important;
    stroke: rgba(139, 92, 246, 0.15) !important;
    stroke-width: 1 !important;
  }

  /* ── Legend ── */
  .legend {
    position: absolute;
    bottom: 14px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 18px;
    background: rgba(10, 10, 26, 0.85);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(139, 92, 246, 0.08);
    border-radius: 12px;
    z-index: 10;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 600;
    color: #5a5a7a;
    white-space: nowrap;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .dot.canon {
    background: #daa520;
    box-shadow: 0 0 6px rgba(218, 165, 32, 0.4);
  }

  .legend-sep {
    width: 1px;
    height: 14px;
    background: rgba(139, 92, 246, 0.1);
  }

  .line {
    width: 16px;
    height: 2px;
    flex-shrink: 0;
  }

  .line.solid {
    background: rgba(139, 92, 246, 0.4);
    border-radius: 1px;
  }

  .line.dashed {
    background: repeating-linear-gradient(
      90deg,
      rgba(0, 242, 255, 0.4) 0px,
      rgba(0, 242, 255, 0.4) 4px,
      transparent 4px,
      transparent 7px
    );
  }
</style>
