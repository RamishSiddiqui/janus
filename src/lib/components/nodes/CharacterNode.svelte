<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();
</script>

<div class="char-node">
  <div class="char-img-wrap">
    {#if data.avatarUrl}
      <img class="char-img" src={data.avatarUrl} alt={data.label} />
    {:else}
      <div class="char-img fallback">
        <Icon name="user" size={24} />
      </div>
    {/if}
  </div>
  <div class="char-info">
    <span class="char-name">{data.label}</span>
    {#if data.subtitle}
      <span class="char-sub">{data.subtitle}</span>
    {/if}
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .char-node {
    display: flex;
    align-items: stretch;
    background: linear-gradient(135deg, #0e0e1e, #141028);
    border: 1.5px solid rgba(139, 92, 246, 0.3);
    border-radius: 16px;
    overflow: hidden;
    box-shadow:
      0 0 32px rgba(139, 92, 246, 0.12),
      0 8px 24px rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(12px);
    min-width: 200px;
    cursor: grab;
  }

  .char-img-wrap {
    width: 64px;
    flex-shrink: 0;
    background: rgba(139, 92, 246, 0.06);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .char-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .char-img.fallback {
    color: #5a5a7a;
    width: 100%;
    height: 100%;
  }

  .char-info {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 12px 20px 12px 14px;
    gap: 2px;
  }

  .char-name {
    font-size: 15px;
    font-weight: 700;
    color: #e8e0ff;
    letter-spacing: -0.3px;
    white-space: nowrap;
  }

  .char-sub {
    font-size: 10px;
    color: #5a5a7a;
    font-weight: 500;
  }
</style>
