<script lang="ts">
  import Icon from './Icon.svelte';
  import MemoryGraph from './MemoryGraph.svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';

  let { conversationId, onClose }: { conversationId: string; onClose: () => void } = $props();

  let graphData = $state<MemoryGraphData | null>(null);
  let isLoading = $state(true);

  async function load() {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      graphData = await ipc.getCastMemoryGraph(conversationId);
    } catch (err) {
      console.error('Failed to load cast memory graph:', err);
      graphData = null;
    }
    isLoading = false;
  }

  load();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="cast-graph-overlay"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  role="presentation"
>
  <div class="cast-graph-modal" role="dialog" aria-modal="true" aria-label="Conversation cast graph">
    <div class="cast-graph-hdr">
      <span class="cast-graph-title">Cast Graph</span>
      <span class="cast-graph-subtitle">Memories shared between this conversation's characters</span>
      <button class="cast-graph-close" onclick={onClose} aria-label="Close cast graph">
        <Icon name="x" size={16} color="#8b8ba7" />
      </button>
    </div>

    {#if isLoading}
      <div class="cast-graph-empty"><span>Loading…</span></div>
    {:else if !graphData || (graphData.memories.length === 0 && (graphData.characters?.length ?? 0) === 0)}
      <div class="cast-graph-empty">
        <Icon name="network" size={24} color="var(--fg-muted)" />
        <span>No cast memories to graph yet</span>
      </div>
    {:else}
      <div class="cast-graph-canvas">
        <MemoryGraph data={graphData} onRefresh={load} />
      </div>
    {/if}
  </div>
</div>

<style>
  .cast-graph-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(6,6,15,0.7); backdrop-filter: blur(4px);
    display: flex; align-items: center; justify-content: center;
    animation: overlayIn 180ms ease;
  }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }

  .cast-graph-modal {
    width: min(1100px, 95vw); height: 82vh;
    display: flex; flex-direction: column; gap: 12px;
    padding: 20px 22px; border-radius: 16px;
    background: linear-gradient(175deg, #0e0e22, #0a0a18);
    border: 1px solid rgba(139,92,246,0.15);
    box-shadow: 0 20px 60px rgba(0,0,0,0.5);
    animation: modalIn 220ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes modalIn {
    from { opacity: 0; transform: translateY(12px) scale(0.97); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .cast-graph-hdr { display: flex; align-items: baseline; gap: 10px; flex-shrink: 0; }
  .cast-graph-title {
    font-size: 16px; font-weight: 700; letter-spacing: -0.2px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent;
  }
  .cast-graph-subtitle { font-size: var(--text-sm); color: #5a5a7a; flex: 1; min-width: 0; }
  .cast-graph-close {
    width: 28px; height: 28px; border-radius: 8px; border: 1px solid rgba(139,92,246,0.1);
    background: transparent; cursor: pointer; display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .cast-graph-close:hover { background: rgba(139,92,246,0.08); }

  .cast-graph-empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; flex: 1; color: #5a5a7a; font-size: 13px;
  }

  .cast-graph-canvas { flex: 1; min-height: 0; border-radius: 12px; overflow: hidden; border: 1px solid var(--border-subtle); }
</style>
