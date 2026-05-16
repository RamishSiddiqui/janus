<script lang="ts">
  import Icon from './Icon.svelte';
  import Skeleton from './Skeleton.svelte';
  import type { NavItem, ConversationPreview } from '$lib/types';
  import { conversations, activeConversationId, loadMessages, deleteConversation, createConversation, loadConversations, isLoadingConversations, hasMoreConversations, loadMoreConversations, totalConversations } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { tick } from 'svelte';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { 
    navItems, currentPath, collapsed = false, onNavigate, onToggleCollapse 
  }: {
    navItems: readonly NavItem[]; currentPath: string; collapsed?: boolean;
    onNavigate: (path: string) => void; onToggleCollapse: () => void;
  } = $props();

  let searchInput = $state('');
  let searchQuery = $state('');
  let searchFocused = $state(false);
  let showConversations = $derived(currentPath === '/');
  let filteredConversations = $derived(
    searchQuery ? $conversations.filter(c => c.characterName.toLowerCase().includes(searchQuery.toLowerCase())) : $conversations
  );

  // Debounce search input (150ms) to reduce re-renders during fast typing
  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(value: string) {
    searchInput = value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => { searchQuery = value; }, 150);
  }

  let ctxMenu: { x: number; y: number; convId: string } | null = $state(null);
  let renamingId: string | null = $state(null);
  let renameValue = $state('');

  function openContextMenu(e: MouseEvent, convId: string) { e.preventDefault(); ctxMenu = { x: e.clientX, y: e.clientY, convId }; }
  function closeContextMenu() { ctxMenu = null; }

  async function handleDelete(convId: string) { closeContextMenu(); await deleteConversation(convId); success('Conversation deleted'); }

  function startRename(convId: string) {
    const conv = $conversations.find(c => c.id === convId);
    renameValue = conv?.characterName || conv?.preview || ''; renamingId = convId; closeContextMenu();
    tick().then(() => {
      const el = document.querySelector('.rename-input') as HTMLInputElement;
      if (el) { el.focus(); el.select(); }
    });
  }

  async function finishRename(convId: string) {
    if (!isTauri || !renameValue.trim()) { renamingId = null; return; }
    try { const ipc = await import('$lib/services/ipc'); await ipc.updateConversation(convId, renameValue.trim()); await loadConversations(); success('Conversation renamed'); }
    catch (err) { toastError('Rename failed'); }
    renamingId = null;
  }

  async function handleNewChat() { if (!isTauri) return; await createConversation('', 'New Chat'); onNavigate('/'); }
</script>

<aside class="sidebar" class:collapsed aria-label="Application sidebar">
  <div class="sb-glow-top" aria-hidden="true"></div>
  <div class="sb-glow-orb" aria-hidden="true"></div>

  <!-- Brand -->
  <div class="sb-brand">
    <div class="brand-mark" role="button" tabindex="0" onclick={() => onNavigate('/')} onkeydown={(e) => e.key === 'Enter' && onNavigate('/')}>
      <div class="brand-icon">
        <Icon name="sparkles" size={18} color="#c4a1ff" />
        <div class="brand-icon-glow"></div>
      </div>
      {#if !collapsed}<span class="brand-name">Mythic</span>{/if}
    </div>
    {#if !collapsed}
      <button class="btn-new" title="New Chat" aria-label="Start new chat" onclick={handleNewChat}>
        <Icon name="plus" size={14} color="#fff" />
      </button>
    {/if}
  </div>

  <!-- Nav -->
  <nav class="sb-nav" aria-label="Main navigation">
    {#each navItems as item (item.path)}
      {@const isActive = currentPath === item.path}
      <button class="sb-nav-item" class:active={isActive} onclick={() => onNavigate(item.path)}
        title={collapsed ? item.label : undefined} aria-current={isActive ? 'page' : undefined}>
        {#if isActive}<span class="nav-glow-bar"></span>{/if}
        <span class="nav-icon"><Icon name={item.icon} size={16} color={isActive ? '#c4a1ff' : '#6b6b8a'} /></span>
        {#if !collapsed}<span class="nav-text">{item.label}</span>{/if}
      </button>
    {/each}
  </nav>

  {#if showConversations && !collapsed}
    <div class="sb-divider"><div class="divider-grad"></div></div>

    <!-- Search -->
    <div class="sb-search" class:focused={searchFocused}>
      <div class="search-icon-wrap"><Icon name="search" size={13} color={searchFocused ? '#c4a1ff' : '#6b6b8a'} /></div>
      <input type="text" placeholder="Search chats..." aria-label="Search conversations"
        value={searchInput} oninput={(e) => onSearchInput((e.target as HTMLInputElement).value)} onfocus={() => searchFocused = true} onblur={() => searchFocused = false} />
      {#if searchInput}
        <button class="search-clear" onclick={() => { searchInput = ''; searchQuery = ''; }} aria-label="Clear search">
          <Icon name="x" size={12} color="#6b6b8a" />
        </button>
      {/if}
      <div class="search-glow"></div>
    </div>

    <div class="sb-section-head">
      <span class="section-tag">Recent</span>
      <span class="section-count">{filteredConversations.length}</span>
    </div>

    <!-- Conversations -->
    <div class="sb-convos">
      {#if $isLoadingConversations && filteredConversations.length === 0}
        {#each Array(4) as _, i}
          <div class="conv-skeleton"><Skeleton variant="circle" width="38px" height="38px" /><div class="skel-lines"><Skeleton variant="text" width="65%" /><Skeleton variant="text" width="85%" /></div></div>
        {/each}
      {:else if filteredConversations.length === 0}
        <div class="conv-empty"><span class="conv-empty-icon">💬</span><span>No conversations yet</span><span class="conv-empty-sub">Start one from the gallery</span></div>
      {:else}
        {#each filteredConversations as conv, i (conv.id)}
          {@const isActive = $activeConversationId === conv.id}
          <button class="conv-card" class:active={isActive}
            onclick={() => { $activeConversationId = conv.id; loadMessages(conv.id); }}
            oncontextmenu={(e) => openContextMenu(e, conv.id)}
            style="animation-delay: {i * 40}ms"
            aria-current={isActive ? 'true' : undefined}>
            <div class="conv-ava-wrap">
              <div class="conv-ava" style="background:{conv.avatarColor}">
                {#if conv.avatarUrl}
                  <img src={conv.avatarUrl} alt={conv.characterName} class="conv-ava-img" />
                {/if}
              </div>
              {#if isActive}<span class="conv-pulse"></span>{/if}
            </div>
            <div class="conv-body">
              {#if renamingId === conv.id}
                <input class="rename-input" bind:value={renameValue}
                  onblur={() => finishRename(conv.id)}
                  onkeydown={(e) => { if (e.key==='Enter') finishRename(conv.id); if (e.key==='Escape') renamingId=null; }} />
              {:else}
                <span class="conv-title">{conv.characterName}</span>
                <span class="conv-sub">{conv.preview}</span>
              {/if}
            </div>
            <span class="conv-meta">{conv.time}</span>
            {#if isActive}<div class="conv-active-glow"></div>{/if}
          </button>
        {/each}

        {#if $hasMoreConversations}
          <button class="load-more-btn" onclick={loadMoreConversations}>
            <Icon name="chevron-down" size={12} color="#8B5CF6" />
            <span>Load more ({$conversations.length} of {$totalConversations})</span>
          </button>
        {/if}
      {/if}
    </div>
  {/if}

  {#if ctxMenu}
    <button class="ctx-bg" onclick={closeContextMenu} aria-label="Close menu"></button>
    <div class="ctx-menu" role="menu" style="left:{ctxMenu.x}px;top:{ctxMenu.y}px;">
      <button class="ctx-btn" role="menuitem" onclick={() => startRename(ctxMenu!.convId)}>
        <Icon name="pencil" size={12} color="#6b6b8a" /><span>Rename</span>
      </button>
      <button class="ctx-btn danger" role="menuitem" onclick={() => handleDelete(ctxMenu!.convId)}>
        <Icon name="trash-2" size={12} color="var(--danger)" /><span>Delete</span>
      </button>
    </div>
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
    border-radius: 12px;
    background: linear-gradient(135deg, rgba(139,92,246,0.22), rgba(0,242,255,0.08));
    border: 1px solid rgba(139,92,246,0.2);
  }
  .brand-icon-glow {
    position: absolute; inset: -4px; border-radius: 14px;
    background: radial-gradient(circle, rgba(139,92,246,0.25) 0%, transparent 70%);
    pointer-events: none; opacity: 0.6;
    animation: brandPulse 3s ease-in-out infinite;
  }
  @keyframes brandPulse { 0%,100% { opacity: 0.4; } 50% { opacity: 0.8; } }

  .brand-name {
    font-size: var(--text-2xl); font-weight: 800; letter-spacing: -0.5px; white-space: nowrap;
    background: linear-gradient(135deg, #fff 10%, #c4a1ff 60%, #bf40ff 100%);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
  }

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

  /* ── Navigation ── */
  .sb-nav {
    display: flex; flex-direction: column; gap: 2px; flex-shrink: 0;
    position: relative; z-index: 1;
  }
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
  .collapsed .sb-nav-item { justify-content: center; padding: 10px; }

  /* ── Divider ── */
  .sb-divider { padding: 6px 6px; flex-shrink: 0; z-index: 1; position: relative; }
  .divider-grad {
    height: 1px;
    background: linear-gradient(90deg, transparent 0%, rgba(139,92,246,0.2) 30%, rgba(0,242,255,0.08) 70%, transparent 100%);
  }

  /* ── Search ── */
  .sb-search {
    display: flex; align-items: center; gap: 8px;
    height: 38px; padding: 0 12px; border-radius: 10px;
    background: rgba(14,14,30,0.7);
    border: 1px solid rgba(139,92,246,0.08);
    flex-shrink: 0; position: relative; z-index: 1;
    transition: all 250ms var(--ease-out);
  }
  .sb-search.focused {
    background: rgba(20,20,40,0.9);
    border-color: rgba(139,92,246,0.35);
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06), 0 4px 20px rgba(139,92,246,0.08);
  }
  .search-glow {
    position: absolute; inset: -1px; border-radius: 11px; pointer-events: none;
    opacity: 0; transition: opacity 300ms;
    background: linear-gradient(135deg, rgba(139,92,246,0.15), rgba(0,242,255,0.05));
    z-index: -1;
  }
  .sb-search.focused .search-glow { opacity: 1; }

  .search-icon-wrap { display: flex; flex-shrink: 0; transition: transform 200ms; }
  .sb-search.focused .search-icon-wrap { transform: scale(1.1); }

  .sb-search input {
    flex: 1; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: 13px; font-family: var(--font-body);
  }
  .sb-search input::placeholder { color: #4a4a6a; }
  .search-clear {
    width: 20px; height: 20px; border-radius: 50%;
    background: rgba(139,92,246,0.1); border: none; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: background 150ms;
  }
  .search-clear:hover { background: rgba(139,92,246,0.2); }

  /* ── Section Head ── */
  .sb-section-head {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 6px 2px; flex-shrink: 0; z-index: 1; position: relative;
  }
  .section-tag {
    font-size: var(--text-xs); font-weight: 700; letter-spacing: 1.8px;
    text-transform: uppercase; color: #5a5a7a;
    font-family: var(--font-mono);
  }
  .section-count {
    font-size: var(--text-xs); font-weight: 600; color: #8B5CF6;
    background: rgba(139,92,246,0.1); padding: 1px 7px;
    border-radius: 99px; font-family: var(--font-mono);
  }

  /* ── Conversation List ── */
  .sb-convos {
    display: flex; flex-direction: column; gap: 3px;
    overflow-y: auto; flex: 1; min-height: 0;
    position: relative; z-index: 1; padding-right: 2px;
  }
  .sb-convos::-webkit-scrollbar { width: 3px; }
  .sb-convos::-webkit-scrollbar-track { background: transparent; }
  .sb-convos::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }
  .sb-convos::-webkit-scrollbar-thumb:hover { background: rgba(139,92,246,0.3); }

  .conv-card {
    display: flex; align-items: center; gap: 11px;
    padding: 10px 12px; border-radius: 12px;
    border: 1px solid transparent; background: transparent;
    text-align: left; width: 100%; font-family: var(--font-body);
    cursor: pointer; position: relative; overflow: hidden;
    animation: cardSlideIn 350ms var(--ease-out) both;
    transition: all 180ms var(--ease-out);
  }
  @keyframes cardSlideIn { from { opacity: 0; transform: translateX(-8px); } to { opacity: 1; transform: none; } }

  .conv-card:hover {
    background: rgba(139,92,246,0.06);
    border-color: rgba(139,92,246,0.08);
    transform: translateX(2px);
  }
  .conv-card.active {
    background: linear-gradient(135deg, rgba(139,92,246,0.1), rgba(191,64,255,0.04));
    border-color: rgba(139,92,246,0.15);
  }
  .conv-active-glow {
    position: absolute; inset: 0; pointer-events: none; border-radius: 12px;
    box-shadow: inset 0 0 20px rgba(139,92,246,0.06);
  }

  .conv-ava-wrap { position: relative; flex-shrink: 0; }
  .conv-ava {
    width: 38px; height: 38px; min-width: 38px; min-height: 38px;
    border-radius: 50%; aspect-ratio: 1; overflow: hidden;
    transition: box-shadow 200ms var(--ease-out), transform 200ms var(--ease-out);
  }
  .conv-ava-img {
    width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 50%;
  }
  .conv-card:hover .conv-ava { transform: scale(1.05); }
  .conv-card.active .conv-ava { box-shadow: 0 0 14px rgba(139,92,246,0.3); }

  .conv-pulse {
    position: absolute; bottom: -1px; right: -1px;
    width: 11px; height: 11px; border-radius: 50%;
    background: #10B981; border: 2px solid #09091a;
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(16,185,129,0.3); } 50% { box-shadow: 0 0 0 5px rgba(16,185,129,0); } }

  .conv-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .conv-title {
    font-size: var(--text-md); font-weight: 600; color: #c8c8e0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .conv-card.active .conv-title { color: #e8e0ff; }
  .conv-card:not(.active) .conv-title { font-weight: 500; color: #8b8ba7; }

  .conv-sub {
    font-size: var(--text-sm); color: #5a5a7a;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .conv-meta {
    font-size: 10px; color: #4a4a6a; font-family: var(--font-mono); flex-shrink: 0;
  }

  /* Empty */
  .conv-empty {
    display: flex; flex-direction: column; align-items: center;
    gap: 6px; padding: 36px 16px; text-align: center;
  }
  .conv-empty-icon { font-size: var(--text-3xl); opacity: 0.4; }
  .conv-empty span { font-size: var(--text-sm); color: #5a5a7a; }
  .conv-empty-sub { font-size: var(--text-sm); color: #3a3a5a; }

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

  .rename-input {
    width: 100%; padding: 3px 8px; border-radius: 6px;
    border: 1px solid rgba(139,92,246,0.4); background: rgba(14,14,30,0.8);
    color: #e0e0f0; font-size: var(--text-sm); font-family: var(--font-body); outline: none;
    box-shadow: 0 0 0 3px rgba(139,92,246,0.08);
  }

  /* Load More */
  .load-more-btn {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    width: 100%; padding: 8px 12px; margin-top: 4px;
    border-radius: 10px; border: 1px solid rgba(139,92,246,0.1);
    background: rgba(139,92,246,0.04); color: #8B5CF6;
    font-size: 11px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms var(--ease-out);
  }
  .load-more-btn:hover {
    background: rgba(139,92,246,0.1);
    border-color: rgba(139,92,246,0.2);
  }
  .load-more-btn:active { transform: scale(0.97); }

  .conv-skeleton { display: flex; align-items: center; gap: 11px; padding: 10px 12px; }
  .skel-lines { flex: 1; display: flex; flex-direction: column; gap: 6px; }

  @media (max-width: 768px) {
    .sidebar { position: fixed; left: 0; top: 0; z-index: 100; transform: translateX(-100%); }
  }
</style>
