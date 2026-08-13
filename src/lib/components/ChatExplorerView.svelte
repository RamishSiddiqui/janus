<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    icon, title, description = '', onClose, children,
  }: {
    icon: string;
    title: string;
    description?: string;
    onClose: () => void;
    children: Snippet;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="explorer">
  <div class="explorer-header">
    <div class="explorer-title-group">
      <div class="explorer-icon" aria-hidden="true">
        <Icon name={icon} size={18} color="#c4a1ff" />
      </div>
      <div class="explorer-title-text">
        <h2 class="explorer-title">{title}</h2>
        {#if description}<p class="explorer-desc">{description}</p>{/if}
      </div>
    </div>
    <button class="explorer-close" onclick={onClose} aria-label="Close, return to chat">
      <span>Back to Chat</span>
      <Icon name="x" size={14} color="#c4a1ff" />
    </button>
  </div>

  <div class="explorer-body">
    {@render children()}
  </div>
</div>

<style>
  .explorer {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    animation: explorerFadeIn 220ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes explorerFadeIn {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .explorer-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 18px 28px; flex-shrink: 0;
    border-bottom: 1px solid rgba(139,92,246,0.08);
    background: linear-gradient(180deg, rgba(12,12,30,0.6), transparent);
  }
  .explorer-title-group { display: flex; align-items: center; gap: 14px; min-width: 0; }
  .explorer-icon {
    width: 40px; height: 40px; border-radius: 12px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: rgba(139,92,246,0.1); border: 1px solid rgba(139,92,246,0.2);
  }
  .explorer-title-text { min-width: 0; }
  .explorer-title {
    margin: 0; font-size: var(--text-xl); font-weight: 700; color: #e8e0ff;
    letter-spacing: -0.3px;
  }
  .explorer-desc {
    margin: 2px 0 0; font-size: var(--text-sm); color: #6b6b8a;
  }

  .explorer-close {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding: 8px 14px; border-radius: 10px;
    background: rgba(139,92,246,0.08); border: 1px solid rgba(139,92,246,0.18);
    color: #c4a1ff; font-size: 12.5px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms;
  }
  .explorer-close:hover {
    background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.3);
    transform: translateY(-1px);
  }

  .explorer-body {
    flex: 1; min-height: 0; overflow-y: auto; padding: 24px 28px;
    /* Lets the wrapped panels' own cqi-based clamp() sizing scale up
       against this much wider container, instead of the narrow popover
       width they were originally designed against. */
    container-type: inline-size;
    /* Flex column so a child that opts into `flex: 1; min-height: 0`
       (the wide Cast graph canvas) can stretch to fill whatever space is
       left below it, rather than being stuck at a fixed height with dead
       space underneath. Children that don't opt in lay out exactly as
       before — a single flex item with no grow just takes its content
       height, same as block flow. */
    display: flex; flex-direction: column;
  }
  .explorer-body::-webkit-scrollbar { width: 4px; }
  .explorer-body::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }
</style>
