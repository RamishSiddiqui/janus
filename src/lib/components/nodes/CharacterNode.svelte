<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { data } = $props();
</script>

<div class="char-node">
  <div class="char-avatar-wrap">
    {#if data.avatarUrl}
      <img class="char-avatar" src={data.avatarUrl} alt={data.label} />
    {:else}
      <div class="char-avatar fallback">
        <Icon name="user" size={20} />
      </div>
    {/if}
    <div class="avatar-ring"></div>
  </div>
  <div class="char-info">
    <span class="char-name">{data.label}</span>
    {#if data.subtitle}
      <span class="char-sub">{data.subtitle}</span>
    {/if}
  </div>
  <div class="char-glow"></div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .char-node {
    display: flex;
    align-items: center;
    gap: 12px;
    background: linear-gradient(145deg, #0d0d1e 0%, #130e28 60%, #0d0d1e 100%);
    border: 1.5px solid rgba(139, 92, 246, 0.25);
    border-radius: 16px;
    padding: 10px 18px 10px 10px;
    overflow: hidden;
    cursor: grab;
    position: relative;
    min-width: 200px;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  .char-node:hover {
    border-color: rgba(139, 92, 246, 0.5);
    box-shadow: 0 0 28px rgba(139, 92, 246, 0.15), 0 4px 16px rgba(0,0,0,0.3);
  }

  /* Top accent line */
  .char-node::before {
    content: '';
    position: absolute;
    top: 0;
    left: 20%;
    right: 20%;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.5), transparent);
  }

  .char-glow {
    position: absolute;
    top: -40%;
    left: -20%;
    width: 70%;
    height: 120%;
    background: radial-gradient(ellipse, rgba(139,92,246,0.06) 0%, transparent 70%);
    pointer-events: none;
  }

  .char-avatar-wrap {
    position: relative;
    flex-shrink: 0;
    width: 44px;
    height: 44px;
  }

  .char-avatar {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    object-fit: cover;
    display: block;
    position: relative;
    z-index: 1;
  }

  .char-avatar.fallback {
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(139, 92, 246, 0.1);
    color: #6a6a8a;
    border: 1px solid rgba(139,92,246,0.15);
  }

  /* Decorative ring behind avatar */
  .avatar-ring {
    position: absolute;
    inset: -2px;
    border-radius: 13px;
    border: 1.5px solid rgba(139, 92, 246, 0.2);
    animation: ringPulse 4s ease-in-out infinite;
  }

  @keyframes ringPulse {
    0%, 100% { opacity: 0.3; }
    50% { opacity: 0.7; }
  }

  .char-info {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    min-width: 0;
  }

  .char-name {
    font-size: 14px;
    font-weight: 700;
    color: #ede7ff;
    letter-spacing: -0.3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .char-sub {
    font-size: 10px;
    color: #6a6a8a;
    font-weight: 500;
    letter-spacing: 0.1px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
