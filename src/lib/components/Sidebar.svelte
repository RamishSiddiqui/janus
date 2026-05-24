<script lang="ts">
  import Icon from './Icon.svelte';
  import Skeleton from './Skeleton.svelte';
  import type { NavItem, ConversationPreview } from '$lib/types';
  import { conversations, activeConversationId, loadMessages, deleteConversation, createConversation, loadConversations, isLoadingConversations, hasMoreConversations, loadMoreConversations, totalConversations } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { tick } from 'svelte';
  import type { SearchResult } from '$lib/services/ipc';

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

  // AI Studio accordion — persisted in localStorage
  let aiStudioOpen = $state(
    browser ? (localStorage.getItem('sidebar-ai-studio-open') !== 'false') : true
  );
  function toggleAiStudio() {
    aiStudioOpen = !aiStudioOpen;
    if (browser) localStorage.setItem('sidebar-ai-studio-open', String(aiStudioOpen));
  }

  let filteredConversations = $derived(
    searchQuery ? $conversations.filter(c => c.characterName.toLowerCase().includes(searchQuery.toLowerCase())) : $conversations
  );

  // --- Character Grouping ---
  interface CharacterGroup {
    characterName: string;
    characterId: string | null;
    avatarColor: string;
    avatarUrl: string | null;
    conversations: ConversationPreview[];
    hasActiveConv: boolean;
  }

  // Collect unique characters across all crossover conversations for the stacked header avatar
  interface UniqueChar { id: string; name: string; avatarUrl: string | null; avatarColor: string; }
  let crossoverUniqueChars = $derived.by(() => {
    const seen = new Map<string, UniqueChar>();
    for (const conv of sharedConversations) {
      // Primary character
      if (conv.characterId && !seen.has(conv.characterId)) {
        seen.set(conv.characterId, { id: conv.characterId, name: conv.characterName, avatarUrl: conv.avatarUrl, avatarColor: conv.avatarColor });
      }
      // Additional characters
      for (const p of (conv.additionalCharacters ?? [])) {
        if (!seen.has(p.id)) {
          seen.set(p.id, { id: p.id, name: p.name, avatarUrl: p.avatarUrl, avatarColor: p.avatarColor });
        }
      }
    }
    return [...seen.values()].slice(0, 4);
  });

  // Tracks which groups user has explicitly toggled. 
  // Key = characterName, value = desired state (true=open, false=closed)
  let manualToggles = $state<Map<string, boolean>>(new Map());
  let sharedExpanded = $state(true);

  let ungroupedConversations = $derived.by(() => {
    return filteredConversations.filter(c => !c.characterId);
  });

  // Shared conversations: those with additionalCharacters — shown in dedicated Alliances section
  let sharedConversations = $derived.by(() => {
    return filteredConversations.filter(c => c.characterId && (c.additionalCharacters?.length ?? 0) > 0);
  });
  let hasActiveShared = $derived(sharedConversations.some(c => c.id === $activeConversationId));

  let characterGroups = $derived.by(() => {
    const groups = new Map<string, CharacterGroup>();
    const convs = filteredConversations;

    // Helper to ensure a group exists for a character
    function ensureGroup(name: string, id: string | null, color: string, url: string | null) {
      if (!groups.has(name)) {
        groups.set(name, {
          characterName: name,
          characterId: id,
          avatarColor: color,
          avatarUrl: url,
          conversations: [],
          hasActiveConv: false,
        });
      }
    }

    for (const conv of convs) {
      // Skip conversations without a character (shown ungrouped)
      if (!conv.characterId) continue;
      // Skip multi-character conversations (shown in Alliances section)
      if ((conv.additionalCharacters?.length ?? 0) > 0) continue;

      const primaryKey = conv.characterName || 'Unknown';
      ensureGroup(primaryKey, conv.characterId, conv.avatarColor, conv.avatarUrl);

      const primaryGroup = groups.get(primaryKey)!;
      if (!primaryGroup.conversations.some(c => c.id === conv.id)) {
        primaryGroup.conversations.push(conv);
      }
      if ($activeConversationId === conv.id) {
        primaryGroup.hasActiveConv = true;
      }
    }

    // Sort: active character first, then alphabetical. Skip groups with 0 solo convs.
    return [...groups.values()]
      .filter(g => g.conversations.length > 0)
      .sort((a, b) => {
        if (a.hasActiveConv && !b.hasActiveConv) return -1;
        if (!a.hasActiveConv && b.hasActiveConv) return 1;
        return a.characterName.localeCompare(b.characterName);
      });
  });

  function toggleGroup(name: string) {
    const next = new Map(manualToggles);
    const currentlyExpanded = isGroupExpanded(name, characterGroups.find(g => g.characterName === name));
    next.set(name, !currentlyExpanded);
    manualToggles = next;
  }

  function isGroupExpanded(name: string, group?: CharacterGroup): boolean {
    // If user manually toggled this group, use that state
    if (manualToggles.has(name)) return manualToggles.get(name)!;
    // Default: active character's group starts expanded, others collapsed
    return group?.hasActiveConv ?? false;
  }

  // --- Deep Search State ---
  let searchResults = $state<SearchResult[]>([]);
  let isSearching = $state(false);
  let showSearchResults = $state(false);
  let selectedResultIndex = $state(-1);
  let searchDebounceTimer: ReturnType<typeof setTimeout>;

  // Debounce search input (150ms) for local filtering
  let searchTimeout: ReturnType<typeof setTimeout>;
  function onSearchInput(value: string) {
    searchInput = value;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => { searchQuery = value; }, 150);

    // Auto-trigger deep search after 300ms of typing (if 2+ chars)
    clearTimeout(searchDebounceTimer);
    if (value.trim().length >= 2 && isTauri) {
      searchDebounceTimer = setTimeout(() => triggerDeepSearch(value.trim()), 300);
    } else if (value.trim().length === 0) {
      showSearchResults = false;
      searchResults = [];
    }
  }

  async function triggerDeepSearch(query: string) {
    if (!isTauri || !query) return;
    isSearching = true;
    showSearchResults = true;
    selectedResultIndex = -1;
    try {
      const ipc = await import('$lib/services/ipc');
      searchResults = await ipc.searchMessages(query, 20);
    } catch (err) {
      console.error('Search failed:', err);
      searchResults = [];
    }
    isSearching = false;
  }

  function handleSearchKeydown(e: KeyboardEvent) {
    if (showSearchResults && searchResults.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        selectedResultIndex = Math.min(selectedResultIndex + 1, searchResults.length - 1);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        selectedResultIndex = Math.max(selectedResultIndex - 1, -1);
      } else if (e.key === 'Enter' && selectedResultIndex >= 0) {
        e.preventDefault();
        navigateToResult(searchResults[selectedResultIndex]);
        return;
      }
    }
    if (e.key === 'Enter' && searchInput.trim().length >= 2) {
      e.preventDefault();
      triggerDeepSearch(searchInput.trim());
    }
    if (e.key === 'Escape') {
      showSearchResults = false;
      selectedResultIndex = -1;
    }
  }

  function navigateToResult(result: SearchResult) {
    $activeConversationId = result.conversation_id;
    loadMessages(result.conversation_id);
    onNavigate('/');
    clearSearch();
  }

  function clearSearch() {
    searchInput = '';
    searchQuery = '';
    searchResults = [];
    showSearchResults = false;
    selectedResultIndex = -1;
  }

  function formatSearchTime(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h`;
    const days = Math.floor(hrs / 24);
    if (days < 7) return `${days}d`;
    return `${Math.floor(days / 7)}w`;
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
    <!-- Flat (ungrouped) items first -->
    {#each navItems.filter(i => i.path !== '/settings' && !i.group) as item (item.path)}
      {@const isActive = currentPath === item.path}
      <button class="sb-nav-item" class:active={isActive} onclick={() => onNavigate(item.path)}
        title={collapsed ? item.label : undefined} aria-current={isActive ? 'page' : undefined}>
        {#if isActive}<span class="nav-glow-bar"></span>{/if}
        <span class="nav-icon"><Icon name={item.icon} size={16} color={isActive ? '#c4a1ff' : '#6b6b8a'} /></span>
        {#if !collapsed}<span class="nav-text">{item.label}</span>{/if}
      </button>
    {/each}

    <!-- AI Studio accordion group -->
    {#if !collapsed}
      {@const studioItems = navItems.filter(i => i.group === 'ai-studio')}
      {@const studioActive = studioItems.some(i => currentPath.startsWith(i.path))}
      {@const studioOpen = aiStudioOpen || studioActive}
      <div class="nav-group" class:group-active={studioActive}>
        <button class="nav-group-header" onclick={toggleAiStudio}
          aria-expanded={studioOpen} aria-label="AI Studio section">
          <span class="nav-group-icon">
            <Icon name="cpu" size={15} color={studioActive ? '#c4a1ff' : '#6b6b8a'} />
          </span>
          <span class="nav-group-label" class:active={studioActive}>AI Studio</span>
          <span class="nav-group-chevron" class:open={studioOpen}>
            <Icon name="chevron-right" size={13} color={studioActive ? '#c4a1ff' : '#5a5a7a'} />
          </span>
        </button>
        {#if studioOpen}
          <div class="nav-sub-list">
            <div class="nav-sub-rail"></div>
            {#each studioItems as item (item.path)}
              {@const isActive = currentPath === item.path || currentPath.startsWith(item.path + '/')}
              <button class="nav-sub-item" class:active={isActive}
                onclick={() => onNavigate(item.path)}
                aria-current={isActive ? 'page' : undefined}>
                {#if isActive}<span class="nav-sub-accent"></span>{/if}
                <span class="nav-sub-icon">
                  <Icon name={item.icon} size={14} color={isActive ? '#c4a1ff' : '#5a5a7a'} />
                </span>
                <span class="nav-sub-text">{item.label}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <!-- Collapsed: show sub-items as icon-only -->
      {#each navItems.filter(i => i.group === 'ai-studio') as item (item.path)}
        {@const isActive = currentPath === item.path}
        <button class="sb-nav-item" class:active={isActive} onclick={() => onNavigate(item.path)}
          title={item.label} aria-current={isActive ? 'page' : undefined}>
          {#if isActive}<span class="nav-glow-bar"></span>{/if}
          <span class="nav-icon"><Icon name={item.icon} size={16} color={isActive ? '#c4a1ff' : '#6b6b8a'} /></span>
        </button>
      {/each}
    {/if}
  </nav>

  {#if showConversations && !collapsed}
    <div class="sb-divider"><div class="divider-grad"></div></div>

    <!-- Search -->
    <div class="sb-search" class:focused={searchFocused}>
      <div class="search-icon-wrap"><Icon name="search" size={13} color={searchFocused ? '#c4a1ff' : '#6b6b8a'} /></div>
      <input type="text" placeholder="Search messages..." aria-label="Search messages"
        value={searchInput} oninput={(e) => onSearchInput((e.target as HTMLInputElement).value)}
        onkeydown={handleSearchKeydown}
        onfocus={() => searchFocused = true} onblur={() => searchFocused = false} />
      {#if searchInput}
        <button class="search-clear" onclick={clearSearch} aria-label="Clear search">
          <Icon name="x" size={12} color="#6b6b8a" />
        </button>
      {/if}
      <div class="search-glow"></div>
    </div>

    <!-- Search Results Overlay -->
    {#if showSearchResults}
      <div class="search-results">
        <div class="search-results-header">
          <span class="section-tag">Results</span>
          {#if !isSearching}
            <span class="section-count">{searchResults.length}</span>
          {/if}
        </div>
        <div class="search-results-list">
          {#if isSearching}
            {#each Array(3) as _, i}
              <div class="conv-skeleton"><Skeleton variant="text" width="90%" /><Skeleton variant="text" width="60%" /></div>
            {/each}
          {:else if searchResults.length === 0}
            <div class="conv-empty">
              <span class="conv-empty-icon">🔍</span>
              <span>No messages found</span>
              <span class="conv-empty-sub">Try different keywords</span>
            </div>
          {:else}
            {#each searchResults as result, i (result.message_id)}
              <button class="search-result-card" class:selected={selectedResultIndex === i}
                onclick={() => navigateToResult(result)}
                onmouseenter={() => selectedResultIndex = i}>
                <div class="sr-header">
                  <span class="sr-character">{result.character_name ?? result.conversation_title}</span>
                  <span class="sr-role" class:user={result.role === 'user'}>{result.role === 'user' ? 'You' : 'AI'}</span>
                  <span class="sr-time">{formatSearchTime(result.created_at)}</span>
                </div>
                <div class="sr-snippet">{@html result.snippet}</div>
              </button>
            {/each}
          {/if}
        </div>
      </div>
    {:else}
      <!-- Conversation List (hidden during search) -->
      <!-- Character-Grouped Conversation List (hidden during search) -->
      <div class="sb-section-head">
        <span class="section-tag">Chats</span>
        <span class="section-count">{filteredConversations.length}</span>
      </div>

      <div class="sb-convos">
        {#if $isLoadingConversations && filteredConversations.length === 0}
          {#each Array(4) as _, i}
            <div class="conv-skeleton"><Skeleton variant="circle" width="38px" height="38px" /><div class="skel-lines"><Skeleton variant="text" width="65%" /><Skeleton variant="text" width="85%" /></div></div>
          {/each}
        {:else if characterGroups.length === 0 && ungroupedConversations.length === 0 && sharedConversations.length === 0}
          <div class="conv-empty"><span class="conv-empty-icon">💬</span><span>No conversations yet</span><span class="conv-empty-sub">Start one from the gallery</span></div>
        {:else}
          <!-- Ungrouped conversations (no character) -->
          {#each ungroupedConversations as conv (conv.id)}
            {@const isActive = $activeConversationId === conv.id}
            <button class="cg-conv ungrouped" class:active={isActive}
              onclick={() => { $activeConversationId = conv.id; loadMessages(conv.id); }}
              oncontextmenu={(e) => openContextMenu(e, conv.id)}
              aria-current={isActive ? 'true' : undefined}>
              <div class="cg-conv-accent"></div>
              <span class="cg-conv-title">{conv.preview || conv.characterName || 'New Chat'}</span>
              <span class="cg-conv-time">{conv.time}</span>
            </button>
          {/each}

          <!-- ══ Alliances — Multi-Character Conversations ══ -->
          {#if sharedConversations.length > 0}
            <div class="crossover-section" class:has-active={hasActiveShared}>
              <button class="crossover-header" onclick={() => sharedExpanded = !sharedExpanded} aria-expanded={sharedExpanded}>
                <!-- Stacked Avatar Cluster -->
                <div class="crossover-avatar-stack">
                  {#each crossoverUniqueChars.slice(0, 3) as char, i (char.id)}
                    <div class="stack-ava" style="z-index: {10 - i}; --stack-i: {i}">
                      <div class="stack-ava-ring">
                        <div class="stack-ava-inner" style="background: {char.avatarColor}">
                          {#if char.avatarUrl}
                            <img src={char.avatarUrl} alt={char.name} />
                          {:else}
                            <span class="stack-ava-letter">{char.name.charAt(0)}</span>
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/each}
                  {#if crossoverUniqueChars.length > 3}
                    <div class="stack-ava stack-overflow" style="z-index: 6; --stack-i: 3">
                      <div class="stack-ava-ring overflow">
                        <div class="stack-ava-inner overflow-inner">
                          <span class="stack-overflow-count">+{crossoverUniqueChars.length - 3}</span>
                        </div>
                      </div>
                    </div>
                  {/if}
                  <div class="stack-glow"></div>
                </div>

                <div class="cg-info">
                  <span class="crossover-title">Alliances</span>
                  <span class="cg-count">{sharedConversations.length} {sharedConversations.length === 1 ? 'quest' : 'quests'} · {crossoverUniqueChars.length} heroes</span>
                </div>
                <div class="cg-chevron" class:rotated={sharedExpanded}>
                  <Icon name="chevron-down" size={12} color="#00d4e0" />
                </div>
              </button>

              {#if sharedExpanded}
                <div class="crossover-list">
                  {#each sharedConversations as conv, ci (conv.id)}
                    {@const isActive = $activeConversationId === conv.id}
                    <button class="crossover-conv" class:active={isActive}
                      onclick={() => { $activeConversationId = conv.id; loadMessages(conv.id); }}
                      oncontextmenu={(e) => openContextMenu(e, conv.id)}
                      style="animation-delay: {ci * 50}ms"
                      aria-current={isActive ? 'true' : undefined}>
                      <div class="crossover-accent"></div>
                      
                      <!-- Conversation-level stacked avatars -->
                      <div class="crossover-badges">
                        <div class="crossover-ava primary" style="background:{conv.avatarColor}">
                          {#if conv.avatarUrl}<img src={conv.avatarUrl} alt={conv.characterName} />{/if}
                        </div>
                        {#each (conv.additionalCharacters ?? []).slice(0, 2) as p, pi}
                          <div class="crossover-ava" style="background:{p.avatarColor}; --ava-i: {pi + 1}">
                            {#if p.avatarUrl}<img src={p.avatarUrl} alt={p.name} />{/if}
                          </div>
                        {/each}
                      </div>

                      <div class="crossover-body">
                        <span class="crossover-conv-title">{conv.preview || 'Untitled'}</span>
                        <div class="crossover-member-pills">
                          <span class="member-pill primary-pill">{conv.characterName}</span>
                          {#each (conv.additionalCharacters ?? []) as p}
                            <span class="member-pill-sep">×</span>
                            <span class="member-pill">{p.name}</span>
                          {/each}
                        </div>
                      </div>
                      <span class="cg-conv-time">{conv.time}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}

          <!-- Character groups (solo conversations only) -->
          {#each characterGroups as group, gi (group.characterName)}
            {@const expanded = isGroupExpanded(group.characterName, group)}
            <div class="char-group" class:expanded class:has-active={group.hasActiveConv}>
              <button class="char-group-header" 
                onclick={() => toggleGroup(group.characterName)}
                aria-expanded={expanded}>
                <div class="cg-ava-wrap">
                  <div class="cg-ava" style="background:{group.avatarColor}">
                    {#if group.avatarUrl}
                      <img src={group.avatarUrl} alt={group.characterName} class="cg-ava-img" />
                    {/if}
                  </div>
                  {#if group.hasActiveConv}<span class="cg-active-dot"></span>{/if}
                </div>
                <div class="cg-info">
                  <span class="cg-name">{group.characterName}</span>
                  <span class="cg-count">{group.conversations.length} {group.conversations.length === 1 ? 'chat' : 'chats'}</span>
                </div>
                <div class="cg-chevron" class:rotated={expanded}>
                  <Icon name="chevron-down" size={12} color="#6b6b8a" />
                </div>
              </button>

              {#if expanded}
                <div class="cg-list">
                  {#each group.conversations as conv, ci (conv.id)}
                    {@const isActive = $activeConversationId === conv.id}
                    <button class="cg-conv" class:active={isActive}
                      onclick={() => { $activeConversationId = conv.id; loadMessages(conv.id); }}
                      oncontextmenu={(e) => openContextMenu(e, conv.id)}
                      style="animation-delay: {ci * 30}ms"
                      aria-current={isActive ? 'true' : undefined}>
                      <div class="cg-conv-accent"></div>
                      <div class="cg-conv-dot"></div>
                      {#if renamingId === conv.id}
                        <input class="rename-input" bind:value={renameValue}
                          onblur={() => finishRename(conv.id)}
                          onkeydown={(e) => { if (e.key==='Enter') finishRename(conv.id); if (e.key==='Escape') renamingId=null; }} />
                      {:else}
                        <div class="cg-conv-body">
                          <div class="cg-conv-head">
                            <span class="cg-conv-title">{conv.preview || 'Untitled'}</span>
                            <span class="cg-conv-time">{conv.time}</span>
                          </div>
                        </div>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
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
  {/if}

  <!-- Bottom-pinned Settings -->
  <div class="sb-bottom">
    <div class="sb-divider"><div class="divider-grad"></div></div>
    <button class="sb-nav-item" class:active={currentPath === '/settings'} onclick={() => onNavigate('/settings')}
      title={collapsed ? 'Settings' : undefined} aria-current={currentPath === '/settings' ? 'page' : undefined}>
      {#if currentPath === '/settings'}<span class="nav-glow-bar"></span>{/if}
      <span class="nav-icon"><Icon name="settings" size={16} color={currentPath === '/settings' ? '#c4a1ff' : '#6b6b8a'} /></span>
      {#if !collapsed}<span class="nav-text">Settings</span>{/if}
    </button>
  </div>

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
    overflow: hidden;
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

  /* ── AI Studio Accordion Group ── */
  .nav-group {
    border-radius: 10px;
    border: 1px solid transparent;
    overflow: hidden;
    transition: border-color 200ms, background 200ms;
  }
  .nav-group.group-active {
    background: rgba(139,92,246,0.04);
    border-color: rgba(139,92,246,0.08);
  }
  .nav-group-header {
    display: flex; align-items: center; gap: 10px;
    padding: 9px 12px; width: 100%;
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-body); text-align: left;
    transition: background 160ms;
    border-radius: 10px;
  }
  .nav-group-header:hover { background: rgba(139,92,246,0.06); }
  .nav-group-icon { display: flex; align-items: center; width: 20px; height: 20px; flex-shrink: 0; }
  .nav-group-label {
    flex: 1; font-size: var(--text-md); font-weight: 500; color: #6b6b8a;
    white-space: nowrap; transition: color 160ms;
  }
  .nav-group-label.active { color: #c4a1ff; font-weight: 600; }
  .nav-group-chevron {
    display: flex; flex-shrink: 0;
    transition: transform 220ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .nav-group-chevron.open { transform: rotate(90deg); }

  .nav-sub-list {
    position: relative;
    padding: 2px 0 6px 14px;
    display: flex; flex-direction: column; gap: 1px;
    animation: subListIn 200ms ease both;
  }
  @keyframes subListIn {
    from { opacity: 0; transform: translateY(-6px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .nav-sub-rail {
    position: absolute; left: 22px; top: 4px; bottom: 8px;
    width: 1.5px;
    background: linear-gradient(180deg, rgba(139,92,246,0.2) 0%, rgba(139,92,246,0.04) 100%);
    border-radius: 2px;
  }
  .nav-sub-item {
    display: flex; align-items: center; gap: 9px;
    padding: 7px 10px 7px 20px;
    border-radius: 8px; width: 100%;
    background: transparent; border: 1px solid transparent;
    font-family: var(--font-body); text-align: left; cursor: pointer;
    position: relative; transition: all 150ms;
  }
  .nav-sub-item:hover {
    background: rgba(139,92,246,0.07);
    border-color: rgba(139,92,246,0.06);
  }
  .nav-sub-item.active {
    background: linear-gradient(90deg, rgba(139,92,246,0.14) 0%, rgba(139,92,246,0.04) 100%);
    border-color: rgba(139,92,246,0.12);
  }
  .nav-sub-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 2.5px; height: 16px; border-radius: 0 4px 4px 0;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 10px rgba(139,92,246,0.7);
    animation: barPulse 2.5s ease-in-out infinite;
  }
  .nav-sub-icon { display: flex; align-items: center; width: 18px; height: 18px; flex-shrink: 0; }
  .nav-sub-text {
    font-size: 13px; font-weight: 500; color: #6b6b8a;
    white-space: nowrap; transition: color 150ms;
  }
  .nav-sub-item:hover .nav-sub-text { color: #c8c8e0; }
  .nav-sub-item.active .nav-sub-text { color: #e8e0ff; font-weight: 600; }

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
    display: flex; flex-direction: column; gap: 6px;
    overflow-y: auto; flex: 1 1 0;
    min-height: 80px;
    position: relative; z-index: 1; padding-right: 2px;
  }
  .sb-convos::-webkit-scrollbar { width: 3px; }
  .sb-convos::-webkit-scrollbar-track { background: transparent; }
  .sb-convos::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }
  .sb-convos::-webkit-scrollbar-thumb:hover { background: rgba(139,92,246,0.3); }

  /* ── Character Group ── */
  .char-group {
    border-radius: 14px;
    background: transparent;
    border: 1px solid transparent;
    overflow: hidden;
    transition: all 250ms var(--ease-out);
    flex-shrink: 0;
  }
  .char-group.has-active {
    background: rgba(139,92,246,0.03);
    border-color: rgba(139,92,246,0.06);
  }
  .char-group:hover:not(.has-active) {
    background: rgba(139,92,246,0.02);
  }

  /* ── Group Header ── */
  .char-group-header {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px;
    width: 100%; text-align: left; border: none;
    background: transparent; font-family: var(--font-body);
    cursor: pointer; position: relative;
    border-radius: 14px;
    transition: all 180ms var(--ease-out);
  }
  .char-group-header:hover {
    background: rgba(139,92,246,0.06);
  }
  .char-group-header:active {
    transform: scale(0.98);
  }

  .cg-ava-wrap { position: relative; flex-shrink: 0; }
  .cg-ava {
    width: 36px; height: 36px; min-width: 36px; min-height: 36px;
    border-radius: 10px; overflow: hidden;
    transition: transform 200ms var(--ease-spring), box-shadow 200ms var(--ease-out);
    border: 2px solid rgba(139,92,246,0.08);
  }
  .cg-ava-img {
    width: 100%; height: 100%; object-fit: cover; display: block;
  }
  .char-group.has-active .cg-ava {
    border-color: rgba(139,92,246,0.3);
    box-shadow: 0 0 12px rgba(139,92,246,0.15);
  }
  .char-group-header:hover .cg-ava {
    transform: scale(1.06);
  }

  /* ══ Alliances / Crossovers Section ══ */
  .crossover-section {
    border-radius: 16px;
    background: linear-gradient(165deg, rgba(0,212,224,0.03) 0%, rgba(139,92,246,0.02) 100%);
    border: 1px solid rgba(0,212,224,0.08);
    overflow: hidden;
    transition: all 300ms var(--ease-out);
    position: relative;
    flex-shrink: 0;
  }
  .crossover-section::before {
    content: ''; position: absolute; inset: 0; border-radius: 16px; pointer-events: none;
    background: radial-gradient(ellipse 100% 60% at 20% -20%, rgba(0,212,224,0.08) 0%, transparent 60%);
    opacity: 0; transition: opacity 300ms;
  }
  .crossover-section:hover::before { opacity: 1; }
  .crossover-section.has-active {
    background: linear-gradient(165deg, rgba(0,212,224,0.05) 0%, rgba(139,92,246,0.03) 100%);
    border-color: rgba(0,212,224,0.15);
    box-shadow: 0 4px 24px rgba(0,212,224,0.06), inset 0 1px 0 rgba(0,212,224,0.06);
  }

  .crossover-header {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; width: 100%; text-align: left; border: none;
    background: transparent; font-family: var(--font-body);
    cursor: pointer; border-radius: 16px;
    transition: all 200ms var(--ease-out);
    min-width: 0;
  }
  .crossover-header:hover { background: rgba(0,212,224,0.06); }
  .crossover-header:active { transform: scale(0.985); }

  /* ── Stacked Avatar Cluster (Group Header) ── */
  .crossover-avatar-stack {
    display: flex; align-items: center; flex-shrink: 0;
    position: relative; height: 32px;
    padding-right: 2px;
  }
  .stack-ava {
    position: relative;
    margin-left: calc(var(--stack-i, 0) * -8px);
    transition: transform 280ms cubic-bezier(0.34, 1.56, 0.64, 1),
                margin-left 280ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .crossover-header:hover .stack-ava {
    margin-left: calc(var(--stack-i, 0) * -5px);
    transform: translateY(-1px);
  }

  .stack-ava-ring {
    width: 28px; height: 28px; border-radius: 50%;
    padding: 1.5px;
    background: linear-gradient(135deg, #00d4e0, #8B5CF6);
    box-shadow: 0 2px 6px rgba(0,0,0,0.3), 0 0 0 1px rgba(0,212,224,0.1);
    transition: box-shadow 250ms var(--ease-out);
  }
  .crossover-header:hover .stack-ava-ring {
    box-shadow: 0 4px 12px rgba(0,212,224,0.25), 0 0 0 1px rgba(0,212,224,0.2);
  }
  .stack-ava-ring.overflow {
    background: linear-gradient(135deg, rgba(0,212,224,0.3), rgba(139,92,246,0.3));
  }

  .stack-ava-inner {
    width: 100%; height: 100%; border-radius: 50%;
    overflow: hidden; display: flex; align-items: center; justify-content: center;
  }
  .stack-ava-inner img {
    width: 100%; height: 100%; object-fit: cover; display: block;
  }
  .stack-ava-letter {
    font-size: 11px; font-weight: 700; color: #fff;
    text-shadow: 0 1px 3px rgba(0,0,0,0.3);
  }
  .overflow-inner {
    background: rgba(12,12,30,0.8) !important;
  }
  .stack-overflow-count {
    font-size: 10px; font-weight: 700; color: #00d4e0;
    font-family: var(--font-mono);
  }

  .stack-glow {
    position: absolute; inset: -4px; border-radius: 50%;
    background: radial-gradient(circle, rgba(0,212,224,0.12) 0%, transparent 70%);
    pointer-events: none; opacity: 0.3;
    animation: stackPulse 3.5s ease-in-out infinite;
  }
  @keyframes stackPulse { 0%,100% { opacity: 0.2; transform: scale(1); } 50% { opacity: 0.5; transform: scale(1.04); } }

  .crossover-title {
    font-size: var(--text-md); font-weight: 700; letter-spacing: -0.2px;
    background: linear-gradient(135deg, #00d4e0 20%, #00f2ff 60%, #8B5CF6 100%);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
  }
  .crossover-section.has-active .crossover-title {
    background: linear-gradient(135deg, #00f2ff 0%, #8B5CF6 100%);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
  }

  .crossover-list {
    display: flex; flex-direction: column; gap: 2px;
    padding: 2px 4px 8px;
    animation: listExpand 280ms var(--ease-out) both;
    min-width: 0;
  }

  .crossover-conv {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 8px;
    border-radius: 10px; width: 100%; text-align: left;
    border: none; background: transparent;
    font-family: var(--font-body); cursor: pointer;
    position: relative; overflow: hidden;
    transition: all 180ms var(--ease-out);
    animation: convFadeIn 220ms var(--ease-out) both;
    min-width: 0;
  }
  .crossover-conv:hover {
    background: rgba(0,212,224,0.06);
  }
  .crossover-conv.active {
    background: linear-gradient(90deg, rgba(0,212,224,0.1) 0%, rgba(139,92,246,0.05) 100%);
    box-shadow: inset 0 0 0 1px rgba(0,212,224,0.12);
  }

  /* Cyan-violet accent bar */
  .crossover-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 3px; height: 0; border-radius: 3px;
    background: linear-gradient(180deg, #00f2ff, #8B5CF6);
    transition: height 250ms cubic-bezier(0.34, 1.56, 0.64, 1), box-shadow 250ms var(--ease-out);
  }
  .crossover-conv.active .crossover-accent {
    height: 26px;
    box-shadow: 0 0 12px rgba(0,242,255,0.35), 0 0 4px rgba(139,92,246,0.3);
  }

  /* Conversation-level overlapping avatar badges */
  .crossover-badges {
    display: flex; align-items: center; flex-shrink: 0;
  }
  .crossover-ava {
    width: 24px; height: 24px; border-radius: 50%;
    overflow: hidden; flex-shrink: 0;
    border: 2px solid rgba(12,12,30,0.9);
    margin-left: calc(var(--ava-i, 0) * -8px);
    z-index: calc(10 - var(--ava-i, 0));
    transition: transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
                margin-left 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
                border-color 200ms;
  }
  .crossover-ava.primary { z-index: 10; --ava-i: 0; }
  .crossover-ava img { width: 100%; height: 100%; object-fit: cover; display: block; }

  .crossover-conv:hover .crossover-ava {
    margin-left: calc(var(--ava-i, 0) * -5px);
    transform: scale(1.08);
  }
  .crossover-conv.active .crossover-ava {
    border-color: rgba(0,212,224,0.3);
  }

  .crossover-body {
    flex: 1; min-width: 0;
    display: flex; flex-direction: column; gap: 2px;
    overflow: hidden;
  }
  .crossover-conv-title {
    font-size: var(--text-sm); font-weight: 500; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    transition: color 150ms;
    max-width: 100%;
  }
  .crossover-conv:hover .crossover-conv-title { color: #c8c8e0; }
  .crossover-conv.active .crossover-conv-title { color: #e0f7fa; font-weight: 600; }

  /* Member pills */
  .crossover-member-pills {
    display: flex; align-items: center; gap: 3px;
    overflow: hidden;
    min-width: 0;
  }
  .member-pill {
    font-size: 9px; font-weight: 600; color: #4a7a7e;
    font-family: var(--font-mono); letter-spacing: 0.1px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    transition: color 150ms;
  }
  .member-pill.primary-pill { color: #00a0aa; }
  .member-pill-sep {
    font-size: 8px; color: #3a5a5e; font-weight: 700;
  }
  .crossover-conv:hover .member-pill { color: #6aafb5; }
  .crossover-conv:hover .member-pill.primary-pill { color: #00d4e0; }
  .crossover-conv:hover .member-pill-sep { color: #5a8a8e; }
  .crossover-conv.active .member-pill { color: #00b8c4; }
  .crossover-conv.active .member-pill.primary-pill { color: #00f2ff; }
  .crossover-conv.active .member-pill-sep { color: #00d4e0; }

  .cg-active-dot {
    position: absolute; bottom: -2px; right: -2px;
    width: 10px; height: 10px; border-radius: 50%;
    background: #10B981; border: 2px solid #09091a;
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(16,185,129,0.3); } 50% { box-shadow: 0 0 0 5px rgba(16,185,129,0); } }

  .cg-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; overflow: hidden; }
  .cg-name {
    font-size: var(--text-md); font-weight: 600; color: #c8c8e0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    letter-spacing: -0.1px;
  }
  .char-group.has-active .cg-name { color: #e8e0ff; }

  .cg-count {
    font-size: 10px; font-weight: 500; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 0.3px;
  }

  .cg-chevron {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; flex-shrink: 0;
    transition: transform 250ms var(--ease-spring);
    opacity: 0.5;
  }
  .cg-chevron.rotated { transform: rotate(180deg); }
  .char-group-header:hover .cg-chevron { opacity: 1; }

  /* ── Conversations Sub-list ── */
  .cg-list {
    display: flex; flex-direction: column; gap: 1px;
    padding: 2px 6px 8px 22px;
    position: relative;
    animation: listExpand 250ms var(--ease-out) both;
  }
  @keyframes listExpand {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* Rail line connecting conversations */
  .cg-list::before {
    content: '';
    position: absolute;
    left: 27px; top: 4px; bottom: 10px;
    width: 1.5px;
    background: linear-gradient(180deg, rgba(139,92,246,0.15) 0%, rgba(139,92,246,0.04) 100%);
    border-radius: 2px;
  }

  /* ── Conversation Item (unified layout) ── */
  .cg-conv {
    display: flex; align-items: flex-start;
    padding: 7px 10px 7px 18px;
    border-radius: 8px; width: 100%;
    text-align: left; border: none;
    background: transparent; font-family: var(--font-body);
    cursor: pointer; position: relative;
    transition: all 150ms var(--ease-out);
    animation: convFadeIn 200ms var(--ease-out) both;
  }
  @keyframes convFadeIn {
    from { opacity: 0; transform: translateX(-6px); }
    to { opacity: 1; transform: translateX(0); }
  }
  .cg-conv:hover { background: rgba(139,92,246,0.06); }
  .cg-conv.active { background: rgba(139,92,246,0.1); }

  /* Accent bar — always absolute, never in flow */
  .cg-conv-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 3px; height: 0; border-radius: 3px;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    transition: height 200ms var(--ease-spring), box-shadow 200ms var(--ease-out);
  }
  .cg-conv.active .cg-conv-accent {
    height: 20px;
    box-shadow: 0 0 8px rgba(139,92,246,0.4);
  }

  /* Dot connector — pinned to title line */
  .cg-conv-dot {
    position: absolute;
    left: 3px; top: 14px;
    width: 5px; height: 5px;
    border-radius: 50%;
    background: rgba(139,92,246,0.15);
    transition: all 150ms var(--ease-out);
  }
  .cg-conv:hover .cg-conv-dot {
    background: rgba(139,92,246,0.35);
    transform: scale(1.3);
  }
  .cg-conv.active .cg-conv-dot {
    background: #8B5CF6;
    box-shadow: 0 0 6px rgba(139,92,246,0.5);
  }

  /* Content body */
  .cg-conv-body {
    display: flex; flex-direction: column; gap: 3px;
    flex: 1; min-width: 0;
  }
  .cg-conv-head {
    display: flex; align-items: center; gap: 8px;
  }
  .cg-conv-head .cg-conv-title { flex: 1; }

  .cg-conv-title {
    font-size: var(--text-sm); font-weight: 500; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    flex: 1;
    transition: color 150ms;
  }
  .cg-conv:hover .cg-conv-title { color: #c8c8e0; }
  .cg-conv.active .cg-conv-title { color: #e8e0ff; font-weight: 600; }

  .cg-conv-time {
    font-size: 10px; color: #4a4a6a; font-family: var(--font-mono);
    flex-shrink: 0;
  }

  /* Ungrouped (no character) conversations - flat items */
  .cg-conv.ungrouped {
    padding-left: 12px;
    margin-bottom: 2px;
    flex-shrink: 0;
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

  /* ── Search Results ── */
  .search-results {
    display: flex; flex-direction: column; gap: 4px;
    flex: 1; min-height: 0;
  }
  .search-results-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 14px 4px;
  }
  .search-results-list {
    flex: 1; overflow-y: auto; padding: 0 6px;
    display: flex; flex-direction: column; gap: 4px;
  }

  .search-result-card {
    display: flex; flex-direction: column; gap: 5px;
    padding: 10px 12px; border-radius: 10px;
    background: rgba(14,14,30,0.35);
    border: 1px solid transparent;
    cursor: pointer; text-align: left; width: 100%;
    transition: all 150ms var(--ease-out);
    animation: fadeSlideUp 200ms var(--ease-out) both;
  }
  .search-result-card:hover, .search-result-card.selected {
    background: rgba(139,92,246,0.06);
    border-color: rgba(139,92,246,0.12);
  }
  .search-result-card.selected {
    box-shadow: 0 0 0 1px rgba(139,92,246,0.15), 0 2px 8px rgba(139,92,246,0.08);
  }

  .sr-header {
    display: flex; align-items: center; gap: 6px;
  }
  .sr-character {
    font-size: var(--text-sm); font-weight: 600; color: #c8c8e0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1;
  }
  .sr-role {
    font-size: var(--text-xs); font-weight: 700; padding: 1px 6px;
    border-radius: 4px; letter-spacing: 0.3px; flex-shrink: 0;
    background: rgba(139,92,246,0.12); color: #c4a1ff;
  }
  .sr-role.user {
    background: rgba(16,185,129,0.12); color: #10B981;
  }
  .sr-time {
    font-size: var(--text-xs); color: #4a4a6a;
    font-family: var(--font-mono); flex-shrink: 0;
  }
  .sr-snippet {
    font-size: var(--text-sm); color: #8b8ba7; line-height: 1.5;
    overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .sr-snippet :global(mark) {
    background: rgba(139,92,246,0.25); color: #e8e0ff;
    border-radius: 2px; padding: 0 1px;
  }

  @keyframes fadeSlideUp {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
