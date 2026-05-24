<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();

  const isEmpty: boolean = data.isEmpty ?? false;
</script>

<Handle type="target" position={Position.Top} id="top" />

<div
  class="conv-node"
  class:empty={isEmpty}
  style="--accent: {data.color ?? '#c4a1ff'}; --accent-dim: {data.colorBorder ?? 'rgba(139,92,246,0.25)'};"
>
  <div class="conv-icon-wrap">
    <Icon name={isEmpty ? 'message-circle' : 'message-circle'} size={14} />
  </div>
  <div class="conv-body">
    <span class="conv-title">{data.label}</span>
    {#if isEmpty}
      <span class="conv-sub empty-label">
        <span class="empty-dot"></span>
        No memories yet
      </span>
    {:else if data.memoryCount != null}
      <span class="conv-sub">{data.memoryCount} {data.memoryCount === 1 ? 'memory' : 'memories'}</span>
    {/if}
  </div>
  {#if data.isShared}
    <div class="shared-badge" title="Shared conversation">⇌</div>
  {/if}
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .conv-node {
    display: flex;
    align-items: stretch;
    background: linear-gradient(135deg, #0e0e1e, #141028);
    border: 1.5px solid var(--accent);
    border-radius: 12px;
    overflow: hidden;
    box-shadow:
      0 0 12px color-mix(in srgb, var(--accent) 25%, transparent),
      0 0 28px color-mix(in srgb, var(--accent) 10%, transparent);
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    font-family: 'Inter', -apple-system, sans-serif;
    transition: opacity 300ms ease, border-color 300ms ease;
  }

  /* Empty state — muted, dashed border */
  .conv-node.empty {
    opacity: 0.55;
    border-style: dashed;
    border-width: 1px;
    box-shadow: none;
    background: rgba(10, 10, 22, 0.6);
  }
  .conv-node.empty:hover {
    opacity: 0.8;
  }

  .conv-icon-wrap {
    width: 38px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--accent);
    border-right: 1px solid color-mix(in srgb, var(--accent) 15%, transparent);
  }
  .conv-node.empty .conv-icon-wrap {
    background: rgba(255,255,255,0.02);
    color: #4a4a6a;
    border-right-color: rgba(255,255,255,0.05);
  }

  .conv-body {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 8px 12px;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .conv-title {
    font-size: 12px;
    font-weight: 650;
    color: var(--accent);
    line-height: 1.3;
    letter-spacing: -0.2px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .conv-node.empty .conv-title { color: #5a5a7a; }

  .conv-sub {
    font-size: 9px;
    color: color-mix(in srgb, var(--accent) 40%, #5a5a7a);
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .empty-label { color: #3a3a55; font-style: italic; }

  .empty-dot {
    width: 4px; height: 4px; border-radius: 50%;
    background: #3a3a55;
    flex-shrink: 0;
    animation: empty-blink 2.5s ease-in-out infinite;
  }
  @keyframes empty-blink {
    0%,100% { opacity: 0.3; }
    50%      { opacity: 0.8; }
  }

  .shared-badge {
    padding: 0 8px;
    display: flex;
    align-items: center;
    font-size: 12px;
    color: var(--accent);
    opacity: 0.6;
    flex-shrink: 0;
  }

  /* ── Light theme ── */
  :global([data-theme="light"]) .conv-node {
    background: linear-gradient(135deg, rgba(255,255,255,0.92), rgba(248,245,252,0.95));
    box-shadow: 0 2px 12px rgba(0,0,0,0.06), 0 0 8px color-mix(in srgb, var(--accent) 12%, transparent);
  }
  :global([data-theme="light"]) .conv-node.empty {
    background: rgba(255,255,255,0.5);
    box-shadow: none;
  }
</style>
