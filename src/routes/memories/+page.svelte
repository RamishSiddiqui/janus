<script lang="ts">
  import { browser } from '$app/environment';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import MemoryGraph from '$lib/components/MemoryGraph.svelte';
  import MemoryTimeline from '$lib/components/MemoryTimeline.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Character selection
  interface CharOption { id: string; name: string; avatarPath: string | null; }
  let characters: CharOption[] = $state([]);
  let selectedCharId: string | null = $state(null);
  let isLoading = $state(true);

  // View toggle
  let activeView: 'graph' | 'timeline' = $state('graph');

  // Graph data
  let graphData: import('$lib/services/ipc').MemoryGraph | null = $state(null);
  let isLoadingGraph = $state(false);

  // Stats
  let totalMemories = $derived(graphData?.memories?.length ?? 0);
  let canonCount = $derived(graphData?.memories?.filter(m => m.is_canon).length ?? 0);
  let linkCount = $derived(graphData?.links?.length ?? 0);
  let convCount = $derived(graphData?.conversations?.length ?? 0);

  // Currently selected character info
  let selectedChar = $derived(characters.find(c => c.id === selectedCharId));

  onMount(() => {
    if (!isTauri) {
      isLoading = false;
      return;
    }
    loadCharacters();
  });

  // Check URL param for pre-selected character
  $effect(() => {
    const charParam = $page.url.searchParams.get('character');
    if (charParam && characters.length > 0) {
      selectedCharId = charParam;
    }
  });

  // Load graph when character changes
  $effect(() => {
    if (selectedCharId && isTauri) {
      loadGraph(selectedCharId);
    } else {
      graphData = null;
    }
  });

  async function loadCharacters() {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const chars = await ipc.listCharacters();
      characters = chars.map(c => {
        let parsedData: any = {};
        try { parsedData = JSON.parse(c.data); } catch {}
        return {
          id: c.id,
          name: parsedData?.name ?? c.name,
          avatarPath: c.avatar_path,
        };
      });
      if (!selectedCharId && characters.length > 0) {
        selectedCharId = characters[0].id;
      }
    } catch (err: any) {
      toastError(`Failed to load characters: ${err.message}`);
    } finally {
      isLoading = false;
    }
  }

  async function loadGraph(charId: string) {
    isLoadingGraph = true;
    try {
      const ipc = await import('$lib/services/ipc');
      graphData = await ipc.getMemoryGraph(charId);
    } catch (err: any) {
      toastError(`Failed to load memory graph: ${err.message}`);
      graphData = null;
    } finally {
      isLoadingGraph = false;
    }
  }

  function handleRefresh() {
    if (selectedCharId) loadGraph(selectedCharId);
  }
</script>

<svelte:head>
  <title>Memory Management — Mythic</title>
</svelte:head>

<div class="page">
  <!-- === Top Bar === -->
  <header class="topbar">
    <div class="topbar-left">
      <div class="page-icon">
        <Icon name="git-branch" size={18} />
      </div>
      <div class="page-title-group">
        <h1>Memory Multiverse</h1>
        <p class="subtitle">Character memories across conversation timelines</p>
      </div>
    </div>

    <div class="topbar-right">
      <!-- Character selector (custom) -->
      <div class="char-picker">
        {#if selectedChar?.avatarPath}
          <img
            class="char-avatar"
            src="/avatars/{selectedChar.avatarPath.split('/').pop()}"
            alt={selectedChar.name}
          />
        {:else}
          <div class="char-avatar placeholder">
            <Icon name="user" size={14} />
          </div>
        {/if}
        <select
          class="char-select"
          bind:value={selectedCharId}
          disabled={isLoading || characters.length === 0}
        >
          {#if characters.length === 0}
            <option value={null}>No characters</option>
          {:else}
            {#each characters as char}
              <option value={char.id}>{char.name}</option>
            {/each}
          {/if}
        </select>
        <Icon name="chevron-down" size={14} />
      </div>

      <!-- View switch -->
      <div class="view-switch">
        <button
          class="switch-btn"
          class:active={activeView === 'graph'}
          onclick={() => activeView = 'graph'}
        >
          <Icon name="git-branch" size={14} />
          <span>Graph</span>
        </button>
        <button
          class="switch-btn"
          class:active={activeView === 'timeline'}
          onclick={() => activeView = 'timeline'}
        >
          <Icon name="list" size={14} />
          <span>Timeline</span>
        </button>
        <div class="switch-indicator" class:right={activeView === 'timeline'}></div>
      </div>

      <!-- Refresh -->
      <button class="icon-btn" onclick={handleRefresh} disabled={isLoadingGraph} title="Refresh graph">
        <Icon name="refresh-cw" size={15} />
      </button>
    </div>
  </header>

  <!-- === Stats strip (only when data loaded) === -->
  {#if graphData && totalMemories > 0}
    <div class="stats-strip">
      <div class="stat">
        <span class="stat-value">{totalMemories}</span>
        <span class="stat-label">Memories</span>
      </div>
      <div class="stat-divider"></div>
      <div class="stat">
        <span class="stat-value canon-glow">{canonCount}</span>
        <span class="stat-label">Canon</span>
      </div>
      <div class="stat-divider"></div>
      <div class="stat">
        <span class="stat-value">{convCount}</span>
        <span class="stat-label">Timelines</span>
      </div>
      <div class="stat-divider"></div>
      <div class="stat">
        <span class="stat-value link-glow">{linkCount}</span>
        <span class="stat-label">Links</span>
      </div>
    </div>
  {/if}

  <!-- === Canvas === -->
  <div class="canvas">
    {#if isLoading}
      <div class="empty-state">
        <div class="loading-ring"></div>
        <p>Initializing...</p>
      </div>
    {:else if !selectedCharId}
      <div class="empty-state">
        <div class="empty-icon">
          <Icon name="users" size={32} />
        </div>
        <h2>No Character Selected</h2>
        <p>Choose a character above to explore their memory multiverse</p>
      </div>
    {:else if isLoadingGraph}
      <div class="empty-state">
        <div class="loading-ring"></div>
        <p>Building graph...</p>
      </div>
    {:else if graphData && graphData.memories.length === 0}
      <div class="empty-state">
        <div class="empty-icon pulse">
          <Icon name="zap" size={32} />
        </div>
        <h2>No Memories Yet</h2>
        <p>Chat with {selectedChar?.name ?? 'this character'} to start building their memory multiverse</p>
      </div>
    {:else if graphData && activeView === 'graph'}
      <MemoryGraph data={graphData} onRefresh={handleRefresh} />
    {:else if graphData && activeView === 'timeline'}
      <MemoryTimeline data={graphData} />
    {/if}
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-inverse);
    overflow: hidden;
  }

  /* ── Top Bar ── */
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: rgba(10, 10, 26, 0.85);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid rgba(139, 92, 246, 0.06);
    flex-shrink: 0;
    z-index: 20;
  }

  .topbar-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .page-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: linear-gradient(135deg, rgba(139, 92, 246, 0.15), rgba(191, 64, 255, 0.08));
    border: 1px solid rgba(139, 92, 246, 0.12);
    color: #c4a1ff;
  }

  .page-title-group h1 {
    font-size: 15px;
    font-weight: 700;
    color: #e8e0ff;
    margin: 0;
    letter-spacing: -0.3px;
  }

  .subtitle {
    font-size: 11px;
    color: #5a5a7a;
    margin: 2px 0 0;
  }

  .topbar-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* ── Character Picker ── */
  .char-picker {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px 4px 4px;
    background: rgba(14, 14, 30, 0.6);
    border: 1px solid rgba(139, 92, 246, 0.1);
    border-radius: 10px;
    cursor: pointer;
    position: relative;
    transition: border-color 200ms;
  }
  .char-picker:hover,
  .char-picker:focus-within {
    border-color: rgba(139, 92, 246, 0.25);
  }

  .char-avatar {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .char-avatar.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #8B5CF6, #BF40FF);
    color: #fff;
  }

  .char-select {
    background: none;
    border: none;
    outline: none;
    font-size: 13px;
    font-weight: 600;
    color: #e8e0ff;
    cursor: pointer;
    min-width: 120px;
    appearance: none;
    -webkit-appearance: none;
    font-family: var(--font-body);
  }

  .char-picker :global(svg:last-child) {
    color: #5a5a7a;
    flex-shrink: 0;
  }

  /* ── View Switch (segmented control) ── */
  .view-switch {
    display: flex;
    position: relative;
    background: rgba(14, 14, 30, 0.5);
    border: 1px solid rgba(139, 92, 246, 0.08);
    border-radius: 10px;
    padding: 3px;
  }

  .switch-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 14px;
    font-size: 12px;
    font-weight: 600;
    color: #5a5a7a;
    background: none;
    border: none;
    cursor: pointer;
    position: relative;
    z-index: 1;
    transition: color 250ms;
    font-family: var(--font-body);
  }

  .switch-btn.active { color: #e8e0ff; }

  .switch-btn span { white-space: nowrap; }

  .switch-indicator {
    position: absolute;
    top: 3px;
    left: 3px;
    width: calc(50% - 3px);
    height: calc(100% - 6px);
    background: rgba(139, 92, 246, 0.12);
    border-radius: 8px;
    transition: transform 300ms cubic-bezier(0.4, 0, 0.2, 1);
    border: 1px solid rgba(139, 92, 246, 0.15);
  }

  .switch-indicator.right {
    transform: translateX(100%);
  }

  /* ── Icon Button ── */
  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: 1px solid rgba(139, 92, 246, 0.08);
    background: rgba(14, 14, 30, 0.5);
    color: #5a5a7a;
    cursor: pointer;
    transition: all 200ms;
    font-family: var(--font-body);
  }

  .icon-btn:hover:not(:disabled) {
    color: #c4a1ff;
    border-color: rgba(139, 92, 246, 0.2);
    background: rgba(139, 92, 246, 0.06);
  }

  .icon-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  /* ── Stats Strip ── */
  .stats-strip {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 8px 20px;
    background: rgba(14, 14, 30, 0.4);
    border-bottom: 1px solid rgba(139, 92, 246, 0.04);
    flex-shrink: 0;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 16px;
  }

  .stat-value {
    font-size: 14px;
    font-weight: 700;
    color: #c4a1ff;
    font-family: var(--font-mono);
  }

  .stat-value.canon-glow {
    color: #daa520;
    text-shadow: 0 0 8px rgba(218, 165, 32, 0.3);
  }

  .stat-value.link-glow {
    color: #00f2ff;
    text-shadow: 0 0 8px rgba(0, 242, 255, 0.25);
  }

  .stat-label {
    font-size: 10px;
    color: #4a4a6a;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    font-weight: 600;
  }

  .stat-divider {
    width: 1px;
    height: 16px;
    background: rgba(139, 92, 246, 0.08);
  }

  /* ── Canvas ── */
  .canvas {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  /* ── Empty States ── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 14px;
  }

  .empty-state h2 {
    font-size: 16px;
    font-weight: 700;
    color: #8b8ba7;
    margin: 0;
    letter-spacing: -0.3px;
  }

  .empty-state p {
    font-size: 12px;
    color: #4a4a6a;
    margin: 0;
  }

  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 64px;
    height: 64px;
    border-radius: 20px;
    background: rgba(139, 92, 246, 0.06);
    border: 1px solid rgba(139, 92, 246, 0.08);
    color: #5a5a7a;
  }

  .empty-icon.pulse {
    animation: gentlePulse 3s ease-in-out infinite;
  }

  @keyframes gentlePulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(139, 92, 246, 0); }
    50% { box-shadow: 0 0 24px 4px rgba(139, 92, 246, 0.08); }
  }

  .loading-ring {
    width: 28px;
    height: 28px;
    border: 2px solid rgba(139, 92, 246, 0.1);
    border-top-color: #8B5CF6;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
