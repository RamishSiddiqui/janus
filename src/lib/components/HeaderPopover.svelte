<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    icon, label, isOpen, hasAttention = false, onToggle, children,
  }: {
    icon: string;
    label: string;
    isOpen: boolean;
    hasAttention?: boolean;
    onToggle: () => void;
    children: Snippet;
  } = $props();
</script>

<div class="hdr-pop-wrap">
  <button
    class="hdr-pop-trigger"
    class:active={isOpen}
    title={label}
    aria-label={label}
    aria-pressed={isOpen}
    onclick={onToggle}
  >
    <Icon name={icon} size={15} color={isOpen ? '#c4a1ff' : '#6b6b8a'} />
    {#if hasAttention}<span class="hdr-pop-dot" aria-label="Needs attention"></span>{/if}
  </button>

  <!-- Always mounted (not {#if}) so the wrapped panel's own effects/listeners
       (data loading, event subscriptions) keep running while closed — the
       NPC Cast attention dot specifically depends on this. -->
  <div class="hdr-pop-panel" class:open={isOpen}>
    {@render children()}
  </div>
</div>

<style>
  .hdr-pop-wrap { position: relative; }

  /* Matches ChatHeader.svelte's .ch-btn exactly (Svelte's scoped styles
     don't cross component boundaries, so this can't just reuse that class). */
  .hdr-pop-trigger {
    width: 34px; height: 34px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.08); background: transparent;
    display: flex; align-items: center; justify-content: center; cursor: pointer;
    transition: all 180ms var(--ease-out);
    position: relative;
  }
  .hdr-pop-trigger:hover {
    background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.15);
    transform: translateY(-1px);
  }
  .hdr-pop-trigger.active {
    background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.25);
    box-shadow: 0 0 12px rgba(139,92,246,0.15);
  }

  .hdr-pop-dot {
    position: absolute; top: 4px; right: 4px;
    width: 6px; height: 6px; border-radius: 50%;
    background: #F59E0B;
    animation: hdrPopPulse 1.1s ease-in-out infinite;
  }
  @keyframes hdrPopPulse {
    0%, 100% { opacity: 0.3; transform: scale(0.75); box-shadow: 0 0 0 0 rgba(245,158,11,0); }
    50%      { opacity: 1;   transform: scale(1);    box-shadow: 0 0 6px 2px rgba(245,158,11,0.35); }
  }

  .hdr-pop-panel {
    display: none;
    position: absolute; top: calc(100% + 10px); right: 0;
    width: 400px; max-height: 70vh;
    flex-direction: column;
    overflow-y: auto;
    padding: 14px;
    background: rgba(8, 6, 20, 0.97);
    backdrop-filter: blur(28px) saturate(160%);
    border: 1px solid rgba(191, 64, 255, 0.15);
    border-radius: 14px;
    box-shadow:
      0 0 0 1px rgba(191, 64, 255, 0.04),
      0 4px 24px rgba(0, 0, 0, 0.4),
      0 24px 64px rgba(0, 0, 0, 0.7),
      inset 0 1px 0 rgba(255, 255, 255, 0.04);
    z-index: 100;
    /* Lets the wrapped section's cqi-based clamp() sizing resolve against
       this popover's own width, matching how it resolved against the
       context panel's <aside> before the move. */
    container-type: inline-size;
  }
  .hdr-pop-panel.open {
    display: flex;
    animation: dropDown 200ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes dropDown {
    from { opacity: 0; transform: translateY(-10px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  .hdr-pop-panel::-webkit-scrollbar { width: 3px; }
  .hdr-pop-panel::-webkit-scrollbar-track { background: transparent; }
  .hdr-pop-panel::-webkit-scrollbar-thumb {
    background: rgba(191, 64, 255, 0.2);
    border-radius: 3px;
  }
</style>
