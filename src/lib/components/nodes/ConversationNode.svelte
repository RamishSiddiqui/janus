<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();
</script>

<Handle type="target" position={Position.Top} id="top" />

<div class="conv-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-dim: {data.colorBorder ?? 'rgba(139,92,246,0.25)'};">
  <div class="conv-icon-wrap">
    <Icon name="message-circle" size={14} />
  </div>
  <div class="conv-body">
    <span class="conv-title">{data.label}</span>
    {#if data.memoryCount != null}
      <span class="conv-sub">{data.memoryCount} memories</span>
    {/if}
  </div>
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

  .conv-body {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 8px 12px;
    gap: 2px;
    min-width: 0;
  }

  .conv-title {
    font-size: 12px;
    font-weight: 650;
    color: var(--accent);
    line-height: 1.3;
    letter-spacing: -0.2px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .conv-sub {
    font-size: 9px;
    color: color-mix(in srgb, var(--accent) 40%, #5a5a7a);
    font-weight: 500;
  }

  /* ── Light theme ── */
  :global([data-theme="light"]) .conv-node {
    background: linear-gradient(135deg, rgba(255,255,255,0.92), rgba(248,245,252,0.95));
    box-shadow: 0 2px 12px rgba(0,0,0,0.06), 0 0 8px color-mix(in srgb, var(--accent) 12%, transparent);
  }
</style>
