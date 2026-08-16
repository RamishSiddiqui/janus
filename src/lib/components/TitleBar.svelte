<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import JanusMark from './JanusMark.svelte';

  let { onToggleSidebar }: { onToggleSidebar: () => void } = $props();

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let isMaximized = $state(false);

  onMount(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const win = getCurrentWindow();
      isMaximized = await win.isMaximized();
      unlisten = await win.onResized(async () => {
        isMaximized = await win.isMaximized();
      });
    });
    return () => unlisten?.();
  });

  async function minimize() {
    if (!isTauri) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().minimize();
    } catch (err) {
      console.error('Failed to minimize window:', err);
    }
  }
  async function toggleMaximize() {
    if (!isTauri) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().toggleMaximize();
    } catch (err) {
      console.error('Failed to toggle maximize:', err);
    }
  }
  async function close() {
    if (!isTauri) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().close();
    } catch (err) {
      console.error('Failed to close window:', err);
    }
  }
</script>

<div class="titlebar">
  <div class="tb-left">
    <button class="tb-icon-btn" onclick={onToggleSidebar} aria-label="Toggle sidebar" title="Toggle sidebar (Ctrl+B)">
      <Icon name="menu" size={16} color="#8b8ba7" />
    </button>
  </div>

  <div class="tb-center" data-tauri-drag-region>
    <div class="tb-brand" data-tauri-drag-region>
      <JanusMark size={14} />
      <span class="tb-brand-word"><span class="tb-brand-ja">JA</span><span class="tb-brand-nus">NUS</span></span>
    </div>
  </div>

  <div class="tb-right">
    <button class="tb-win-btn" onclick={minimize} aria-label="Minimize">
      <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0" y="4.5" width="10" height="1" fill="currentColor"/></svg>
    </button>
    <button class="tb-win-btn" onclick={toggleMaximize} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
      {#if isMaximized}
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1" stroke-linejoin="round">
          <path d="M2.5 0.5H9.5V7.5"/>
          <rect x="0.5" y="2.5" width="7" height="7"/>
        </svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1" stroke-linejoin="round">
          <rect x="1" y="1" width="8" height="8"/>
        </svg>
      {/if}
    </button>
    <button class="tb-win-btn tb-win-close" onclick={close} aria-label="Close">
      <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.1" stroke-linecap="round">
        <line x1="0.5" y1="0.5" x2="9.5" y2="9.5"/>
        <line x1="9.5" y1="0.5" x2="0.5" y2="9.5"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 34px;
    flex-shrink: 0;
    background: #0a0a13;
    border-bottom: 1px solid rgba(139, 92, 246, 0.08);
    user-select: none;
    -webkit-user-select: none;
  }

  .tb-left {
    display: flex;
    align-items: center;
    padding-left: 8px;
    width: 140px;
    flex-shrink: 0;
  }

  .tb-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    border: none;
    background: transparent;
    cursor: pointer;
    transition: background 150ms ease;
  }
  .tb-icon-btn:hover {
    background: rgba(139, 92, 246, 0.1);
  }

  .tb-center {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    height: 100%;
  }

  .tb-brand {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 8px;
  }
  .tb-brand-word {
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0.28em;
    text-transform: uppercase;
  }
  .tb-brand-ja { color: #9075F2; }
  .tb-brand-nus { color: #CDA15F; }

  .tb-right {
    display: flex;
    align-items: center;
    width: 140px;
    justify-content: flex-end;
    flex-shrink: 0;
  }

  .tb-win-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 34px;
    border: none;
    background: transparent;
    color: #8b8ba7;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
  }
  .tb-win-btn:hover {
    background: rgba(139, 92, 246, 0.12);
    color: #e0e0f0;
  }
  .tb-win-close:hover {
    background: #e81123;
    color: #ffffff;
  }
</style>
