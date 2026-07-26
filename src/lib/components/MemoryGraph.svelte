<script lang="ts">
  import { SvelteFlow, Controls, Background, BackgroundVariant, MiniMap } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import type { Node, Edge } from '@xyflow/svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import { shareMemory, unlinkMemory, deleteMemory } from '$lib/services/ipc';
  import { undoableDelete } from '$lib/stores/toast';
  import Dagre from '@dagrejs/dagre';
  import CharacterNode from './nodes/CharacterNode.svelte';
  import ConversationNode from './nodes/ConversationNode.svelte';
  import MemoryNode from './nodes/MemoryNode.svelte';
  import MemoryGroupNode from './nodes/MemoryGroupNode.svelte';
  import { groupMemories } from '$lib/utils/groupMemories';
  import SharingEdge from './edges/SharingEdge.svelte';
  import TreeEdge from './edges/TreeEdge.svelte';
  import MemoryActionPanel from './MemoryActionPanel.svelte';

  interface Props {
    data: MemoryGraphData;
    avatars?: Record<string, string | null>;
    onRefresh: () => void;
  }

  let { data, avatars = {}, onRefresh }: Props = $props();

  /* ── Action Panel state ── */
  let selectedMemory = $state<MemoryGraphData['memories'][0] | null>(null);
  let selectedGroupMemories = $state<MemoryGraphData['memories']>([]);
  let selectedMemoryLinks = $derived(
    selectedMemory
      ? data.links.filter(l => l.source_memory_id === selectedMemory!.id)
      : []
  );

  function handleNodeClick({ event, node }: { event: MouseEvent | TouchEvent; node: Node }) {
    // Respond to memory and memoryGroup nodes
    if (node.type === 'memory') {
      const memId = node.id.replace('mem-', '');
      const mem = data.memories.find(m => m.id === memId);
      if (mem) {
        selectedMemory = mem;
        selectedGroupMemories = [];
      }
    } else if (node.type === 'memoryGroup') {
      const firstMemId = node.id.replace('memgroup-', '');
      const mem = data.memories.find(m => m.id === firstMemId);
      if (mem) {
        selectedMemory = mem;
        // Extract all memory IDs from the group node's data
        const groupData = node.data as { memories: Array<{ memoryId: string }> };
        const groupMemIds = (groupData.memories ?? []).map((gm: { memoryId: string }) => gm.memoryId);
        selectedGroupMemories = data.memories.filter(m => groupMemIds.includes(m.id));
      }
    }
  }

  function handlePanelClose() {
    selectedMemory = null;
    selectedGroupMemories = [];
  }

  async function handlePanelShare(config: {
    sourceMemoryId: string;
    targetConversationId: string;
    linkType: 'copy' | 'sync';
    direction: 'one_way' | 'two_way';
    syncMode: 'auto' | 'manual';
  }) {
    await shareMemory(
      config.sourceMemoryId,
      config.targetConversationId,
      config.linkType,
      config.direction,
      config.syncMode,
    );
    onRefresh();
  }

  async function handlePanelUnlink(linkId: string) {
    await unlinkMemory(linkId);
    onRefresh();
  }

  function handlePanelDelete(memoryId: string) {
    selectedMemory = null;
    // The graph re-fetches on refresh rather than removing the node locally,
    // so the memory stays visible until the undo window elapses and the
    // real delete + refresh happen — closing the panel is the immediate
    // feedback that the action was taken.
    undoableDelete(
      'Memory removed',
      async () => {
        await deleteMemory(memoryId);
        onRefresh();
      },
      () => { /* nothing was hidden optimistically — nothing to restore */ },
    );
  }

  const nodeTypes: any = {
    character: CharacterNode,
    conversation: ConversationNode,
    memory: MemoryNode,
    memoryGroup: MemoryGroupNode,
  };

  const edgeTypes: any = { sharing: SharingEdge, tree: TreeEdge };

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
    bg: 'rgba(218,165,32,0.12)',
    border: 'rgba(218,165,32,0.3)',
    text: '#fbbf24',
    edge: 'rgba(218,165,32,0.4)',
  };

  function pal(i: number) { return PALETTE[i % PALETTE.length]; }

  function avatarUrl(charId: string): string | null {
    const path = avatars[charId];
    if (!path) return null;
    // If already a resolved URL (blob:, http:, data:) use it directly
    if (path.startsWith('blob:') || path.startsWith('http') || path.startsWith('data:')) return path;
    return `/avatars/${path.split('/').pop()}`;
  }

  /* ── Node sizes for dagre ── */
  const NODE_SIZES: Record<string, { w: number; h: number }> = {
    character:    { w: 240, h: 64 },
    conversation: { w: 220, h: 56 },
    memory:       { w: 240, h: 120 },
    memoryGroup:  { w: 240, h: 150 },
  };

  /* ── Dagre auto-layout ── */
  function applyLayout(nodes: Node[], treeEdges: Edge[]): Node[] {
    const g = new Dagre.graphlib.Graph().setDefaultEdgeLabel(() => ({}));
    g.setGraph({
      rankdir: 'TB',     // top-to-bottom
      nodesep: 50,       // horizontal gap between siblings
      ranksep: 70,       // vertical gap between ranks
      marginx: 40,
      marginy: 40,
    });

    for (const node of nodes) {
      const size = NODE_SIZES[node.type ?? 'memory'] ?? NODE_SIZES.memory;
      g.setNode(node.id, { width: size.w, height: size.h });
    }

    for (const edge of treeEdges) {
      g.setEdge(edge.source, edge.target);
    }

    Dagre.layout(g);

    return nodes.map(node => {
      const pos = g.node(node.id);
      const size = NODE_SIZES[node.type ?? 'memory'] ?? NODE_SIZES.memory;
      return {
        ...node,
        position: {
          x: pos.x - size.w / 2,
          y: pos.y - size.h / 2,
        },
        width: size.w,
        style: `width: ${size.w}px;`,
      };
    });
  }

  /* ── Build graph ── */
  function buildGraph(g: MemoryGraphData): { nodes: Node[]; edges: Edge[] } {
    const nodes: Node[] = [];
    const treeEdges: Edge[] = [];
    const extraEdges: Edge[] = [];

    const convColorMap = new Map<string, typeof PALETTE[0]>();
    g.conversations.forEach((c, i) => convColorMap.set(c.id, pal(i)));

    const memCountMap = new Map<string, number>();
    g.memories.forEach(m => {
      if (m.conversation_id) {
        memCountMap.set(m.conversation_id, (memCountMap.get(m.conversation_id) ?? 0) + 1);
      }
    });

    // ── Multi-character support ──
    // Build list of characters: use `characters` array if present, else single root
    const charList = g.characters && g.characters.length > 0
      ? g.characters
      : [{ id: g.character_id, name: g.character_name }];

    const isMultiChar = charList.length > 1;

    // Map character_id → root node ID
    const charRootMap = new Map<string, string>();
    // Build a set of memory IDs owned by each character
    const memOwner = new Map<string, string>(); // memory_id → character_id
    g.memories.forEach(m => {
      if (m.character_id) memOwner.set(m.id, m.character_id);
    });
    // Map conversation_id → set of character_ids that have memories in it
    const convCharacters = new Map<string, Set<string>>();
    g.memories.forEach(m => {
      if (m.conversation_id && m.character_id) {
        if (!convCharacters.has(m.conversation_id)) convCharacters.set(m.conversation_id, new Set());
        convCharacters.get(m.conversation_id)!.add(m.character_id);
      }
    });

    // ── Character root nodes ──
    charList.forEach(ch => {
      const rootId = `char-${ch.id}`;
      charRootMap.set(ch.id, rootId);
      const charMems = g.memories.filter(m => m.character_id === ch.id);
      const charConvs = new Set(charMems.filter(m => m.conversation_id).map(m => m.conversation_id!));
      nodes.push({
        id: rootId,
        type: 'character',
        position: { x: 0, y: 0 },
        data: {
          label: ch.name,
          avatarUrl: avatarUrl(ch.id),
          subtitle: `${charMems.length} memories · ${charConvs.size} timelines`,
        },
      });
    });

    // Helper: get root node for a memory's character
    function rootForMemory(mem: { character_id: string | null }): string {
      if (mem.character_id && charRootMap.has(mem.character_id)) {
        return charRootMap.get(mem.character_id)!;
      }
      return charRootMap.values().next().value!; // fallback to first
    }

    // ── Canon memories ──
    g.memories.filter(m => m.is_canon).forEach(m => {
      const p = CANON;
      const rootId = rootForMemory(m);
      nodes.push({
        id: `mem-${m.id}`,
        type: 'memory',
        position: { x: 0, y: 0 },
        data: {
          content: m.content, source: m.source, version: m.version,
          isCanon: true, parentId: m.parent_id,
          color: p.text, colorBg: p.bg, colorBorder: p.border,
        },
      });
      treeEdges.push({
        id: `e-canon-${m.id}`,
        source: rootId,
        target: `mem-${m.id}`,
        sourceHandle: 'bottom',
        targetHandle: 'top',
        type: 'tree',
        data: { color: p.edge },
      });
    });

    // ── Conversation branches ──
    // Track which conv nodes we've already created (for dedup in multi-char)
    const createdConvs = new Set<string>();
    // Build a quick lookup: conv.id → conv (for parent resolution)
    const convById = new Map(g.conversations.map(c => [c.id, c]));

    g.conversations.forEach((conv) => {
      if (createdConvs.has(conv.id)) return;
      createdConvs.add(conv.id);

      const memCount = memCountMap.get(conv.id) ?? 0;
      const isEmpty = memCount === 0;

      // For empty conversations, use a muted palette entry; for active, use the colour map
      const p = isEmpty
        ? { text: '#4a4a6a', bg: 'rgba(255,255,255,0.03)', border: 'rgba(255,255,255,0.08)', edge: 'rgba(120,100,200,0.45)' }
        : (convColorMap.get(conv.id) ?? PALETTE[0]);

      const sharedBy = convCharacters.get(conv.id);
      const isShared = isMultiChar && sharedBy && sharedBy.size > 1;

      nodes.push({
        id: `conv-${conv.id}`,
        type: 'conversation',
        position: { x: 0, y: 0 },
        data: {
          label: conv.title,
          memoryCount: memCount,
          isEmpty,
          color: p.text, colorBg: p.bg, colorBorder: p.border,
          isShared,
          // Pass branch info for potential UI indicators
          isBranch: !!conv.parent_conversation_id,
        },
      });

      // ── Determine the source node for this conversation's edge ──
      // If this conv was branched from another, connect to the parent conv
      // (if the parent exists in our graph). Otherwise fall back to character root.
      const parentConvId = conv.parent_conversation_id;
      const parentConvExists = parentConvId && convById.has(parentConvId);

      if (parentConvExists) {
        // Branch lineage: connect to parent conversation node
        treeEdges.push({
          id: `e-branch-${conv.id}`,
          source: `conv-${parentConvId}`,
          target: `conv-${conv.id}`,
          sourceHandle: 'bottom',
          targetHandle: 'top',
          type: 'tree',
          data: { color: p.edge, dashed: isEmpty },
        });
      } else if (isMultiChar && sharedBy && sharedBy.size > 0) {
        // Multi-char shared: connect to each character root
        sharedBy.forEach(charId => {
          const rootId = charRootMap.get(charId);
          if (rootId) {
            treeEdges.push({
              id: `e-root-${conv.id}-${charId}`,
              source: rootId,
              target: `conv-${conv.id}`,
              sourceHandle: 'bottom',
              targetHandle: 'top',
              type: 'tree',
              data: { color: p.edge, dashed: isEmpty },
            });
          }
        });
      } else {
        // Default: connect to character root
        const ownerChar = sharedBy?.values().next().value ?? conv.character_id ?? g.character_id;
        const rootId = charRootMap.get(ownerChar) ?? charRootMap.values().next().value!;
        treeEdges.push({
          id: `e-root-${conv.id}`,
          source: rootId,
          target: `conv-${conv.id}`,
          sourceHandle: 'bottom',
          targetHandle: 'top',
          type: 'tree',
          data: { color: p.edge, dashed: isEmpty },
        });
      }
    });

    // ── Build set of node pairs that have sharing links ──
    const sharingPairs = new Set<string>();
    g.links.forEach(link => {
      const src = `mem-${link.source_memory_id}`;
      const tgt = link.linked_memory_id ? `mem-${link.linked_memory_id}` : `conv-${link.target_conversation_id}`;
      sharingPairs.add(`${src}->${tgt}`);
      sharingPairs.add(`${tgt}->${src}`);
    });

    // ── Scoped memories (grouped) ──
    // Map: memory_id → graph node id (either mem-X or memgroup-X)
    const memToNodeId = new Map<string, string>();

    const scopedMems = g.memories.filter(m => !m.is_canon);
    const groups = groupMemories(scopedMems, g.links);

    for (const group of groups) {
      const convId = group.laneId === 'canon' ? null : group.laneId;
      const p = convId ? convColorMap.get(convId) : PALETTE[0];
      if (!p) continue;

      if (!group.isGroup) {
        // Single memory → normal MemoryNode
        const m = group.memories[0].memory;
        const nodeId = `mem-${m.id}`;
        memToNodeId.set(m.id, nodeId);

        nodes.push({
          id: nodeId,
          type: 'memory',
          position: { x: 0, y: 0 },
          data: {
            content: m.content, source: m.source, version: m.version,
            isCanon: false, parentId: m.parent_id,
            color: p.text, colorBg: p.bg, colorBorder: p.border,
          },
        });

        // Scoped memories always connect to their conversation node
        const rootId = rootForMemory(m);
        const parentId = convId ? `conv-${convId}` : rootId;
        const pairKey = `${parentId}->${nodeId}`;

        if (!sharingPairs.has(pairKey)) {
          treeEdges.push({
            id: `e-mem-${m.id}`,
            source: parentId,
            target: nodeId,
            sourceHandle: 'bottom',
            targetHandle: 'top',
            type: 'tree',
            data: { color: p.edge },
          });
        }
      } else {
        // Multi-memory group → MemoryGroupNode
        const firstMem = group.memories[0].memory;
        const groupNodeId = `memgroup-${firstMem.id}`;

        // Register all memories in this group to the group node id
        for (const item of group.memories) {
          memToNodeId.set(item.memory.id, groupNodeId);
        }

        nodes.push({
          id: groupNodeId,
          type: 'memoryGroup',
          position: { x: 0, y: 0 },
          data: {
            memories: group.memories.map(item => ({
              content: item.memory.content,
              source: item.memory.source,
              version: item.memory.version,
              isCanon: item.memory.is_canon,
              parentId: item.memory.parent_id,
              memoryId: item.memory.id,
            })),
            color: p.text, colorBg: p.bg, colorBorder: p.border,
          },
        });

        // Scoped group always connects to its conversation node
        const rootId = rootForMemory(firstMem);
        const parentId = convId ? `conv-${convId}` : rootId;

        const pairKey = `${parentId}->${groupNodeId}`;
        if (!sharingPairs.has(pairKey)) {
          treeEdges.push({
            id: `e-memgroup-${firstMem.id}`,
            source: parentId,
            target: groupNodeId,
            sourceHandle: 'bottom',
            targetHandle: 'top',
            type: 'tree',
            data: { color: p.edge },
          });
        }
      }
    }

    // ── Sharing links (custom animated edge) ──
    g.links.forEach((link) => {
      const isSync = link.link_type === 'sync';
      const isTwoWay = link.direction === 'two_way';
      const lbl = isSync ? 'sync' : 'copy';

      // Resolve source/target to group node if the memory is inside a group
      const sourceNodeId = memToNodeId.get(link.source_memory_id) ?? `mem-${link.source_memory_id}`;
      const targetNodeId = link.linked_memory_id
        ? (memToNodeId.get(link.linked_memory_id) ?? `mem-${link.linked_memory_id}`)
        : `conv-${link.target_conversation_id}`;

      extraEdges.push({
        id: `link-${link.id}`,
        source: sourceNodeId,
        target: targetNodeId,
        type: 'sharing',
        sourceHandle: 'bottom',
        targetHandle: 'top',
        data: {
          linkType: link.link_type,
          direction: link.direction,
          label: lbl,
        },
      });
    });

    // Include ALL edges in dagre layout
    const allLayoutEdges = [...treeEdges, ...extraEdges];
    const layoutNodes = applyLayout(nodes, allLayoutEdges);
    return { nodes: layoutNodes, edges: [...treeEdges, ...extraEdges] };
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
    {nodeTypes}
    fitView
    fitViewOptions={{ padding: 0.25 }}
    minZoom={0.15}
    maxZoom={2.5}
    defaultEdgeOptions={{ type: 'smoothstep' }}
    elevateEdgesOnSelect={false}
    {edgeTypes}
    onnodeclick={handleNodeClick}
  >
    <Controls position="bottom-left" />
    <Background variant={BackgroundVariant.Dots} gap={24} size={0.6} patternColor="rgba(139,92,246,0.06)" />
    <MiniMap
      pannable
      zoomable
      position="bottom-right"
    />
  </SvelteFlow>

  <!-- Memory Action Panel -->
  <MemoryActionPanel
    memory={selectedMemory}
    groupMemories={selectedGroupMemories}
    links={data.links}
    conversations={data.conversations}
    onClose={handlePanelClose}
    onShare={handlePanelShare}
    onUnlink={handlePanelUnlink}
    onDelete={handlePanelDelete}
  />

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
    outline: none !important;
  }

  /* Custom node wrappers — strip SvelteFlow defaults */
  .graph-wrap :global(.svelte-flow__node-character),
  .graph-wrap :global(.svelte-flow__node-conversation),
  .graph-wrap :global(.svelte-flow__node-memory),
  .graph-wrap :global(.svelte-flow__node-memoryGroup) {
    background: transparent !important;
    border: none !important;
    box-shadow: none !important;
    padding: 0 !important;
  }

  .graph-wrap :global(.svelte-flow__node-character) { border-radius: 16px !important; }
  .graph-wrap :global(.svelte-flow__node-conversation) { border-radius: 12px !important; }
  .graph-wrap :global(.svelte-flow__node-memory) { border-radius: 10px !important; }
  .graph-wrap :global(.svelte-flow__node-memoryGroup) { border-radius: 12px !important; }

  .graph-wrap :global(.svelte-flow__node-character.selected),
  .graph-wrap :global(.svelte-flow__node-conversation.selected),
  .graph-wrap :global(.svelte-flow__node-memory.selected),
  .graph-wrap :global(.svelte-flow__node-memoryGroup.selected) {
    background: transparent !important;
    border: none !important;
    box-shadow: 0 0 0 2px rgba(139, 92, 246, 0.4), 0 0 24px rgba(139, 92, 246, 0.15) !important;
    outline: none !important;
  }

  .graph-wrap :global(.svelte-flow__node:hover) {
    box-shadow: 0 0 20px rgba(139, 92, 246, 0.12) !important;
  }

  .graph-wrap :global(.svelte-flow__node.selected) {
    outline: none !important;
    box-shadow: 0 0 0 2px rgba(139, 92, 246, 0.4), 0 0 24px rgba(139, 92, 246, 0.15) !important;
  }

  .graph-wrap :global(.svelte-flow__handle) {
    width: 6px;
    height: 6px;
    background: rgba(139, 92, 246, 0.15);
    border: 1px solid rgba(139, 92, 246, 0.1);
    opacity: 0;
  }

  /* Force bottom handles to center */
  .graph-wrap :global(.svelte-flow__handle-bottom) {
    left: 50% !important;
    transform: translateX(-50%) !important;
  }

  /* Force top handles to center */
  .graph-wrap :global(.svelte-flow__handle-top) {
    left: 50% !important;
    transform: translateX(-50%) !important;
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

  /* ═══════ Light Theme ═══════ */
  :global([data-theme="light"]) .graph-wrap :global(.svelte-flow__controls) {
    background: rgba(255,255,255,0.9) !important;
    border-color: rgba(139,92,246,0.1) !important;
    box-shadow: 0 2px 12px rgba(0,0,0,0.08) !important;
  }

  :global([data-theme="light"]) .graph-wrap :global(.svelte-flow__controls button) {
    color: #5a5a7a !important;
  }

  :global([data-theme="light"]) .graph-wrap :global(.svelte-flow__controls button:hover) {
    background: rgba(139,92,246,0.06) !important;
    color: #7c3aed !important;
  }

  :global([data-theme="light"]) .graph-wrap :global(.svelte-flow__minimap) {
    background: rgba(255,255,255,0.9) !important;
    border-color: rgba(139,92,246,0.1) !important;
    box-shadow: 0 2px 12px rgba(0,0,0,0.08) !important;
  }

  :global([data-theme="light"]) .legend {
    background: rgba(255,255,255,0.9);
    border-color: rgba(139,92,246,0.1);
    box-shadow: 0 2px 12px rgba(0,0,0,0.08);
  }

  :global([data-theme="light"]) .legend-item {
    color: #4a4a6a;
  }
</style>
