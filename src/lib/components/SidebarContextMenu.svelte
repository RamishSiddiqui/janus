<script lang="ts">
  import Icon from './Icon.svelte';

  let { x, y, onRename, onDelete, onClose }: {
    x: number; y: number;
    onRename: () => void; onDelete: () => void; onClose: () => void;
  } = $props();
</script>

<button class="ctx-bg" onclick={onClose} aria-label="Close menu"></button>
<div class="ctx-menu" role="menu" style="left:{x}px;top:{y}px;">
  <button class="ctx-btn" role="menuitem" onclick={onRename}>
    <Icon name="pencil" size={12} color="#6b6b8a" /><span>Rename</span>
  </button>
  <button class="ctx-btn danger" role="menuitem" onclick={onDelete}>
    <Icon name="trash-2" size={12} color="var(--danger)" /><span>Delete</span>
  </button>
</div>

<style>
  /* ── Context Menu ── */
  .ctx-bg { position: fixed; inset: 0; background: transparent; z-index: 199; border: none; cursor: default; }
  .ctx-menu {
    position: fixed; z-index: 200; padding: 5px; min-width: 150px;
    background: rgba(16,16,34,0.95); backdrop-filter: blur(16px);
    border: 1px solid rgba(139,92,246,0.12); border-radius: 12px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5), 0 0 0 1px rgba(139,92,246,0.05);
  }
  .ctx-btn {
    display: flex; align-items: center; gap: 9px; width: 100%;
    padding: 8px 12px; border-radius: 8px; border: none;
    background: transparent; color: #8b8ba7; font-size: var(--text-sm);
    font-family: var(--font-body); text-align: left; cursor: pointer;
    transition: all 150ms var(--ease-out);
  }
  .ctx-btn:hover { background: rgba(139,92,246,0.1); color: #c8c8e0; }
  .ctx-btn.danger { color: var(--danger); }
  .ctx-btn.danger:hover { background: rgba(244,63,94,0.1); }
</style>
