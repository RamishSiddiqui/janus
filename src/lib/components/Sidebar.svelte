<script lang="ts">
  import Icon from './Icon.svelte';
  import JanusMark from './JanusMark.svelte';
  import SidebarNav from './SidebarNav.svelte';
  import SidebarSearch from './SidebarSearch.svelte';
  import SidebarConversationList from './SidebarConversationList.svelte';
  import SidebarContextMenu from './SidebarContextMenu.svelte';
  import type { NavItem } from '$lib/types';
  import { conversations, deleteConversationWithUndo, createConversation } from '$lib/stores/chat';
  import { error as toastError } from '$lib/stores/toast';
  import { selectedPersonaId } from '$lib/stores/personas';
  import { browser } from '$app/environment';
  import { tick } from 'svelte';
  import { get } from 'svelte/store';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    navItems, currentPath, collapsed = false, onNavigate, onToggleCollapse
  }: {
    navItems: readonly NavItem[]; currentPath: string; collapsed?: boolean;
    onNavigate: (path: string) => void; onToggleCollapse: () => void;
  } = $props();

  let showConversations = $derived(currentPath === '/');

  // Debounced search text + overlay visibility — SidebarSearch owns the input/results
  // UI itself, but both values are needed here to gate/filter SidebarConversationList.
  let searchQuery = $state('');
  let showSearchResults = $state(false);

  // Context menu + rename state — shared between SidebarConversationList (renders the
  // rename input, handles the context-menu trigger) and SidebarContextMenu (the popup).
  let ctxMenu: { x: number; y: number; convId: string } | null = $state(null);
  let renamingId: string | null = $state(null);
  let renameValue = $state('');

  function openContextMenu(e: MouseEvent, convId: string) { e.preventDefault(); ctxMenu = { x: e.clientX, y: e.clientY, convId }; }
  function closeContextMenu() { ctxMenu = null; }

  function handleDelete(convId: string) {
    closeContextMenu();
    const conv = $conversations.find(c => c.id === convId);
    const label = conv?.characterName || conv?.preview || 'conversation';
    deleteConversationWithUndo(convId, label);
  }

  function startRename(convId: string) {
    const conv = $conversations.find(c => c.id === convId);
    renameValue = conv?.characterName || conv?.preview || ''; renamingId = convId; closeContextMenu();
    tick().then(() => {
      const el = document.querySelector('.rename-input') as HTMLInputElement;
      if (el) { el.focus(); el.select(); }
    });
  }

  async function handleNewChat() {
    if (!isTauri) return;
    const personaId = get(selectedPersonaId) ?? undefined;
    await createConversation('', 'New Chat', personaId);
    onNavigate('/');
  }
</script>

<aside class="sidebar" class:collapsed aria-label="Application sidebar">
  <div class="sb-glow-top" aria-hidden="true"></div>
  <div class="sb-glow-orb" aria-hidden="true"></div>

  <!-- Brand -->
  <div class="sb-brand">
    <div class="brand-mark" role="button" tabindex="0" onclick={() => onNavigate('/')} onkeydown={(e) => e.key === 'Enter' && onNavigate('/')}>
      <div class="brand-icon">
        <JanusMark size={20} />
        <div class="brand-icon-glow"></div>
      </div>
      {#if !collapsed}<span class="brand-name"><span class="brand-name-ja">JA</span><span class="brand-name-nus">NUS</span></span>{/if}
    </div>
    {#if !collapsed}
      <button class="btn-new" title="New Chat" aria-label="Start new chat" onclick={handleNewChat}>
        <Icon name="plus" size={14} color="#fff" />
      </button>
    {/if}
  </div>

  <SidebarNav {navItems} {currentPath} {collapsed} {onNavigate} />

  {#if showConversations && !collapsed}
    <div class="sb-divider"><div class="divider-grad"></div></div>

    <SidebarSearch {onNavigate} bind:searchQuery bind:showSearchResults />

    {#if !showSearchResults}
      <SidebarConversationList
        conversations={$conversations}
        {searchQuery}
        onOpenContextMenu={openContextMenu}
        bind:renamingId
        bind:renameValue
      />
    {/if}
  {/if}

  <!-- Bottom-pinned Trash + Settings -->
  <div class="sb-bottom">
    <div class="sb-divider"><div class="divider-grad"></div></div>
    <button class="sb-nav-item" class:active={currentPath === '/trash'} onclick={() => onNavigate('/trash')}
      title={collapsed ? 'Trash' : undefined} aria-current={currentPath === '/trash' ? 'page' : undefined}>
      {#if currentPath === '/trash'}<span class="nav-glow-bar"></span>{/if}
      <span class="nav-icon"><Icon name="trash-2" size={16} color={currentPath === '/trash' ? '#c4a1ff' : '#6b6b8a'} /></span>
      {#if !collapsed}<span class="nav-text">Trash</span>{/if}
    </button>
    <button class="sb-nav-item" class:active={currentPath === '/settings'} onclick={() => onNavigate('/settings')}
      title={collapsed ? 'Settings' : undefined} aria-current={currentPath === '/settings' ? 'page' : undefined}>
      {#if currentPath === '/settings'}<span class="nav-glow-bar"></span>{/if}
      <span class="nav-icon"><Icon name="settings" size={16} color={currentPath === '/settings' ? '#c4a1ff' : '#6b6b8a'} /></span>
      {#if !collapsed}<span class="nav-text">Settings</span>{/if}
    </button>
  </div>

  {#if ctxMenu}
    <SidebarContextMenu
      x={ctxMenu.x} y={ctxMenu.y}
      onRename={() => startRename(ctxMenu!.convId)}
      onDelete={() => handleDelete(ctxMenu!.convId)}
      onClose={closeContextMenu}
    />
  {/if}
</aside>

<style>
  /* ═══════════════════════════════════════════
     SIDEBAR — Awwwards-tier dark glassmorphism
     ═══════════════════════════════════════════ */
  .sidebar {
    width: var(--sidebar-width);
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 20px 14px 16px;
    gap: 6px;
    overflow: hidden;
    flex-shrink: 0;
    position: relative;
    background: linear-gradient(175deg, #0c0c1e 0%, #09091a 50%, #07071a 100%);
    border-right: 1px solid rgba(139,92,246,0.08);
    transition: width var(--duration-normal) var(--ease-out);
  }

  /* Ambient glow — top */
  .sb-glow-top {
    position: absolute; inset: 0; height: 260px; pointer-events: none; z-index: 0;
    background: radial-gradient(ellipse 120% 100% at 50% -40%, rgba(139,92,246,0.18) 0%, rgba(191,64,255,0.06) 50%, transparent 80%);
  }
  .sb-glow-orb {
    position: absolute; width: 140px; height: 140px; top: 60px; left: -30px;
    border-radius: 50%; filter: blur(60px); opacity: 0.12; pointer-events: none; z-index: 0;
    background: var(--accent-secondary);
    animation: orbDrift 16s ease-in-out infinite alternate;
  }
  @keyframes orbDrift {
    0% { transform: translate(0,0); }
    100% { transform: translate(20px,30px); }
  }

  .sidebar.collapsed { width: 64px; align-items: center; padding: 20px 8px 16px; }

  /* ── Brand ── */
  .sb-brand {
    display: flex; align-items: center; justify-content: space-between;
    padding: 0 2px 10px; flex-shrink: 0; position: relative; z-index: 1;
  }
  .brand-mark {
    display: flex; align-items: center; gap: 10px; cursor: pointer;
    transition: transform 200ms var(--ease-spring);
  }
  .brand-mark:hover { transform: scale(1.03); }
  .brand-mark:active { transform: scale(0.97); }

  .brand-icon {
    position: relative;
    width: 38px; height: 38px; display: flex; align-items: center; justify-content: center;
  }
  .brand-icon-glow {
    position: absolute; inset: -4px; border-radius: 14px;
    background: radial-gradient(circle, rgba(144,117,242,0.25) 0%, transparent 70%);
    pointer-events: none; opacity: 0.6;
    animation: brandPulse 3s ease-in-out infinite;
  }
  @keyframes brandPulse { 0%,100% { opacity: 0.4; } 50% { opacity: 0.8; } }

  .brand-name {
    font-size: var(--text-2xl); font-weight: 500; letter-spacing: 0.32em; white-space: nowrap;
    text-transform: uppercase;
  }
  .brand-name-ja { color: #9075F2; }
  .brand-name-nus { color: #CDA15F; }

  .btn-new {
    width: 34px; height: 34px; border-radius: 10px; border: none; cursor: pointer;
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    box-shadow: 0 4px 20px rgba(139,92,246,0.35), inset 0 1px 0 rgba(255,255,255,0.1);
    transition: transform 200ms var(--ease-spring), box-shadow 200ms var(--ease-out);
    position: relative; overflow: hidden;
  }
  .btn-new::after {
    content: ''; position: absolute; inset: 0;
    background: linear-gradient(135deg, transparent 40%, rgba(255,255,255,0.15) 100%);
    pointer-events: none;
  }
  .btn-new:hover { transform: translateY(-1px) scale(1.05); box-shadow: 0 6px 28px rgba(139,92,246,0.5), inset 0 1px 0 rgba(255,255,255,0.15); }
  .btn-new:active { transform: scale(0.92); }

  /* ── Bottom-pinned section ── */
  .sb-bottom {
    margin-top: auto;
    flex-shrink: 0;
    position: relative;
    z-index: 1;
    padding-top: 2px;
  }

  /* ── Divider ── */
  .sb-divider { padding: 6px 6px; flex-shrink: 0; z-index: 1; position: relative; }
  .divider-grad {
    height: 1px;
    background: linear-gradient(90deg, transparent 0%, rgba(139,92,246,0.2) 30%, rgba(0,242,255,0.08) 70%, transparent 100%);
  }

  /* ── Settings nav item (duplicated from SidebarNav's .sb-nav-item — Svelte
     scopes CSS per-component, and this is the one nav-styled button that
     lives outside <SidebarNav>, pinned below the scrollable conversation
     list rather than inside it) ── */
  .sb-nav-item {
    display: flex; align-items: center; gap: 11px;
    padding: 10px 14px; border-radius: 10px;
    border: 1px solid transparent; background: transparent;
    color: #8b8ba7; font-size: var(--text-md); font-weight: 500;
    font-family: var(--font-body); width: 100%; text-align: left;
    cursor: pointer; position: relative; overflow: hidden;
    transition: all 180ms var(--ease-out);
  }
  .sb-nav-item:hover {
    background: rgba(139,92,246,0.07);
    border-color: rgba(139,92,246,0.06);
    color: #c8c8e0;
  }
  .sb-nav-item.active {
    background: linear-gradient(90deg, rgba(139,92,246,0.12) 0%, rgba(139,92,246,0.04) 100%);
    border-color: rgba(139,92,246,0.1);
    color: #e8e0ff;
    font-weight: 600;
  }
  .nav-glow-bar {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 3px; height: 22px; border-radius: 0 6px 6px 0;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 14px rgba(139,92,246,0.7), 0 0 4px rgba(191,64,255,0.9);
    animation: barPulse 2.5s ease-in-out infinite;
  }
  @keyframes barPulse { 0%,100% { box-shadow: 0 0 10px rgba(139,92,246,0.5); } 50% { box-shadow: 0 0 18px rgba(139,92,246,0.8), 0 0 6px rgba(191,64,255,0.6); } }
  .nav-icon { display: flex; align-items: center; width: 20px; height: 20px; flex-shrink: 0; }
  .nav-text { white-space: nowrap; }
  .sidebar.collapsed .sb-nav-item { justify-content: center; padding: 10px; }

  @media (max-width: 768px) {
    .sidebar { position: fixed; left: 0; top: 0; z-index: 100; transform: translateX(-100%); }
  }
</style>
