<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();
</script>

<Handle type="target" position={Position.Top} id="top" />

<div class="conv-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-bg: {data.colorBg ?? 'rgba(139,92,246,0.1)'}; --accent-border: {data.colorBorder ?? 'rgba(139,92,246,0.25)'};">
  <div class="conv-icon">
    <Icon name="message-circle" size={14} />
  </div>
  <div class="conv-body">
    <span class="conv-title">{data.label}</span>
    <div class="conv-meta">
      {#if data.memoryCount != null}
        <span class="meta-pill">{data.memoryCount} memories</span>
      {/if}
    </div>
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .conv-node {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--accent-bg);
    border: 1.5px solid var(--accent-border);
    border-radius: 12px;
    padding: 10px 16px 10px 10px;
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    position: relative;
    isolation: isolate;
  }

  /* Opaque dark base so edges behind the node are hidden */
  .conv-node::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: #0a0a1a;
    z-index: -1;
  }

  .conv-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.2);
    color: var(--accent);
    flex-shrink: 0;
  }

  .conv-body {
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow: hidden;
  }

  .conv-title {
    font-size: 12px;
    font-weight: 650;
    color: var(--accent);
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .conv-meta {
    display: flex;
    gap: 4px;
  }

  .meta-pill {
    font-size: 9px;
    font-weight: 600;
    color: var(--accent);
    opacity: 0.5;
    letter-spacing: 0.2px;
  }
</style>
