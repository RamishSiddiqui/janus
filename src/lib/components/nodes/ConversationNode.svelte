<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();
</script>

<Handle type="target" position={Position.Top} id="top" />

<div class="conv-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-bg: {data.colorBg ?? 'rgba(139,92,246,0.1)'}; --accent-border: {data.colorBorder ?? 'rgba(139,92,246,0.25)'};">
  <div class="conv-left">
    <div class="conv-icon-ring">
      <Icon name="message-circle" size={13} />
    </div>
    <div class="conv-pulse"></div>
  </div>
  <div class="conv-body">
    <span class="conv-title">{data.label}</span>
    {#if data.memoryCount != null}
      <div class="conv-meta">
        <span class="meta-dot"></span>
        <span class="meta-text">{data.memoryCount} memories</span>
      </div>
    {/if}
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .conv-node {
    display: flex;
    align-items: center;
    gap: 10px;
    background: linear-gradient(135deg, rgba(10,10,26,0.95), rgba(18,16,38,0.95));
    border: 1px solid var(--accent-border);
    border-radius: 14px;
    padding: 10px 14px 10px 10px;
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    position: relative;
    overflow: hidden;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  .conv-node:hover {
    border-color: var(--accent);
    box-shadow: 0 0 20px color-mix(in srgb, var(--accent) 20%, transparent);
  }

  /* Subtle accent gradient glow at top edge */
  .conv-node::before {
    content: '';
    position: absolute;
    top: 0;
    left: 10%;
    right: 10%;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    opacity: 0.5;
  }

  .conv-left {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .conv-icon-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
    color: var(--accent);
    position: relative;
    z-index: 1;
  }

  /* Breathing pulse behind icon */
  .conv-pulse {
    position: absolute;
    inset: -3px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    animation: convPulse 3s ease-in-out infinite;
    z-index: 0;
  }

  @keyframes convPulse {
    0%, 100% { opacity: 0; transform: scale(0.9); }
    50% { opacity: 1; transform: scale(1.05); }
  }

  .conv-body {
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow: hidden;
    min-width: 0;
  }

  .conv-title {
    font-size: 12px;
    font-weight: 650;
    color: var(--accent);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    letter-spacing: -0.2px;
  }

  .conv-meta {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .meta-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.4;
  }

  .meta-text {
    font-size: 9px;
    font-weight: 500;
    color: var(--accent);
    opacity: 0.4;
    letter-spacing: 0.3px;
  }
</style>
