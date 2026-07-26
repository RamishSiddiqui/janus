<script lang="ts">
  import Icon from './Icon.svelte';
  import Skeleton from './Skeleton.svelte';
  import { activeConversationId, loadMessages } from '$lib/stores/chat';
  import { browser } from '$app/environment';
  import type { SearchResult } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    onNavigate,
    searchQuery = $bindable(''),
    showSearchResults = $bindable(false),
  }: {
    onNavigate: (path: string) => void;
    /** Debounced (150ms) search text — also used by SidebarConversationList to locally filter the list. */
    searchQuery?: string;
    /** Whether the deep-search results overlay is showing (parent hides the conversation list while true). */
    showSearchResults?: boolean;
  } = $props();

  let searchInput = $state('');
  let searchFocused = $state(false);

  // --- Deep Search State ---
  let searchResults = $state<SearchResult[]>([]);
  let isSearching = $state(false);
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
</script>

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
{/if}

<style>
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

  .conv-empty {
    display: flex; flex-direction: column; align-items: center;
    gap: 6px; padding: 36px 16px; text-align: center;
  }
  .conv-empty-icon { font-size: var(--text-3xl); opacity: 0.4; }
  .conv-empty span { font-size: var(--text-sm); color: #5a5a7a; }
  .conv-empty-sub { font-size: var(--text-sm); color: #3a3a5a; }

  .conv-skeleton { display: flex; align-items: center; gap: 11px; padding: 10px 12px; }

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
