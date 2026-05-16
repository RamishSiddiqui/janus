<script lang="ts">
  import { browser } from '$app/environment';
  import { page } from '$app/stores';
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

  // Load characters on mount
  $effect(() => {
    if (!isTauri) return;
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
      // Auto-select first character if none selected
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

<div class="memories-page">
  <!-- Header -->
  <header class="memories-header">
    <div class="header-left">
      <h1>
        <Icon name="brain" size={22} />
        Memory Multiverse
      </h1>
      <span class="subtitle">Manage character memories across conversation timelines</span>
    </div>

    <div class="header-controls">
      <!-- Character selector -->
      <div class="char-selector">
        <label for="char-select">Character</label>
        <select
          id="char-select"
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
      </div>

      <!-- View toggle -->
      <div class="view-toggle">
        <button
          class="toggle-btn"
          class:active={activeView === 'graph'}
          onclick={() => activeView = 'graph'}
          title="Graph View"
        >
          <Icon name="git-branch" size={16} />
          Graph
        </button>
        <button
          class="toggle-btn"
          class:active={activeView === 'timeline'}
          onclick={() => activeView = 'timeline'}
          title="Timeline View"
        >
          <Icon name="clock" size={16} />
          Timeline
        </button>
      </div>

      <!-- Refresh -->
      <button class="refresh-btn" onclick={handleRefresh} title="Refresh" disabled={isLoadingGraph}>
        <Icon name="refresh-cw" size={16} />
      </button>
    </div>
  </header>

  <!-- Canvas area -->
  <div class="canvas-container">
    {#if isLoading}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>Loading characters...</p>
      </div>
    {:else if !selectedCharId}
      <div class="empty-state">
        <Icon name="users" size={48} />
        <h2>No Character Selected</h2>
        <p>Select a character to view their memory multiverse</p>
      </div>
    {:else if isLoadingGraph}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>Loading memory graph...</p>
      </div>
    {:else if graphData && activeView === 'graph'}
      <MemoryGraph data={graphData} onRefresh={handleRefresh} />
    {:else if graphData && activeView === 'timeline'}
      <MemoryTimeline data={graphData} />
    {:else if graphData && graphData.memories.length === 0}
      <div class="empty-state">
        <Icon name="brain" size={48} />
        <h2>No Memories Yet</h2>
        <p>Start chatting with this character to build their memory multiverse</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .memories-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-primary);
    overflow: hidden;
  }

  .memories-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 24px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--surface-secondary);
    flex-shrink: 0;
    gap: 16px;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .header-left h1 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .subtitle {
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .char-selector {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .char-selector label {
    font-size: 12px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .char-selector select {
    background: var(--surface-primary);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 6px 12px;
    font-size: 13px;
    color: var(--text-primary);
    cursor: pointer;
    min-width: 160px;
  }

  .char-selector select:focus {
    outline: none;
    border-color: var(--accent-primary);
  }

  .view-toggle {
    display: flex;
    background: var(--surface-primary);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    overflow: hidden;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .toggle-btn:hover {
    color: var(--text-primary);
    background: rgba(255,255,255,0.04);
  }

  .toggle-btn.active {
    color: var(--accent-primary);
    background: rgba(46, 166, 126, 0.1);
  }

  .refresh-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    border: 1px solid var(--border-subtle);
    background: var(--surface-primary);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .refresh-btn:hover:not(:disabled) {
    color: var(--text-primary);
    border-color: var(--accent-primary);
  }

  .refresh-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .canvas-container {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    color: var(--text-tertiary);
  }

  .empty-state h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0;
  }

  .empty-state p {
    font-size: 14px;
    margin: 0;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--border-subtle);
    border-top-color: var(--accent-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
