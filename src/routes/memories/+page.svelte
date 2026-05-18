<script lang="ts">
  import { browser } from '$app/environment';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import MemoryGraph from '$lib/components/MemoryGraph.svelte';
  import MemoryTimeline from '$lib/components/MemoryTimeline.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Character selection (multi-select)
  interface CharOption { id: string; name: string; avatarPath: string | null; }
  let characters = $state<CharOption[]>([]);
  let selectedCharIds = $state<string[]>([]);
  let isLoading = $state(true);

  // View toggle
  let activeView = $state<'graph' | 'timeline'>('graph');

  // Graph data
  let graphData = $state<MemoryGraphData | null>(null);
  let isLoadingGraph = $state(false);

  // Stats
  let totalMemories = $derived(graphData?.memories?.length ?? 0);
  let canonCount = $derived(graphData?.memories?.filter(m => m.is_canon).length ?? 0);
  let linkCount = $derived(graphData?.links?.length ?? 0);
  let convCount = $derived(graphData?.conversations?.length ?? 0);

  // Selected characters info
  let selectedChars = $derived(characters.filter(c => selectedCharIds.includes(c.id)));
  let pickerLabel = $derived.by(() => {
    if (selectedChars.length === 0) return 'Select characters';
    if (selectedChars.length === 1) return selectedChars[0].name;
    if (selectedChars.length === 2) return selectedChars.map(c => c.name.split(' ')[0]).join(' & ');
    return `${selectedChars.length} characters`;
  });

  onMount(() => {
    if (!isTauri) {
      isLoading = false;
      return;
    }
    loadCharacters();

    // Auto-refresh graph when user navigates back to this page from chat
    function onVisible() {
      if (document.visibilityState === 'visible' && selectedCharIds.length > 0) {
        loadGraphs(selectedCharIds);
      }
    }
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  });

  // Check URL param for pre-selected character
  $effect(() => {
    const charParam = $page.url.searchParams.get('character');
    if (charParam && characters.length > 0) {
      if (!selectedCharIds.includes(charParam)) {
        selectedCharIds = [charParam];
      }
    }
  });

  // Load merged graph when selection changes
  $effect(() => {
    if (selectedCharIds.length > 0 && isTauri) {
      loadGraphs(selectedCharIds);
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
      if (selectedCharIds.length === 0 && characters.length > 0) {
        selectedCharIds = [characters[0].id];
      }
    } catch (err: any) {
      toastError(`Failed to load characters: ${err.message}`);
    } finally {
      isLoading = false;
    }
  }

  async function loadGraphs(charIds: string[]) {
    isLoadingGraph = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const results = await Promise.all(charIds.map(id => ipc.getMemoryGraph(id)));
      // Merge all graphs into one
      const merged: MemoryGraphData = {
        character_id: charIds[0],
        character_name: results.map(r => r.character_name).join(' & '),
        characters: results.map(r => ({ id: r.character_id, name: r.character_name })),
        memories: results.flatMap(r => r.memories),
        links: results.flatMap(r => r.links),
        conversations: results.flatMap(r => r.conversations),
      };
      // Deduplicate conversations by ID
      const seenConvs = new Set<string>();
      merged.conversations = merged.conversations.filter(c => {
        if (seenConvs.has(c.id)) return false;
        seenConvs.add(c.id);
        return true;
      });
      graphData = merged;
    } catch (err: any) {
      toastError(`Failed to load memory graph: ${err.message}`);
      graphData = null;
    } finally {
      isLoadingGraph = false;
    }
  }

  function handleRefresh() {
    if (selectedCharIds.length > 0) loadGraphs(selectedCharIds);
  }

  // Custom dropdown state
  let dropdownOpen = $state(false);
  let dropdownEl: HTMLDivElement | undefined = $state();

  function toggleChar(id: string) {
    if (selectedCharIds.includes(id)) {
      selectedCharIds = selectedCharIds.filter(x => x !== id);
    } else {
      selectedCharIds = [...selectedCharIds, id];
    }
  }

  function handleClickOutside(e: MouseEvent) {
    if (dropdownEl && !dropdownEl.contains(e.target as Node)) {
      dropdownOpen = false;
    }
  }
</script>

<svelte:window on:mousedown={handleClickOutside} />

<svelte:head>
  <title>Memory Management — Mythic</title>
</svelte:head>

<div class="page">
  <!-- === Top Bar === -->
  <header class="topbar">
    <div class="topbar-left">
      <div class="page-icon">
        <Icon name="brain" size={18} />
      </div>
      <div class="page-title-group">
        <h1>Memory Multiverse</h1>
        <p class="subtitle">Character memories across conversation timelines</p>
      </div>
    </div>

    <div class="topbar-right">
      <!-- Character selector (custom) -->
      <div class="char-picker" bind:this={dropdownEl}>
        <button
          class="picker-trigger"
          onclick={() => dropdownOpen = !dropdownOpen}
          disabled={isLoading || characters.length === 0}
        >
          <!-- Stacked avatars -->
          <div class="avatar-stack">
            {#each selectedChars.slice(0, 3) as sc, i}
              {#if sc.avatarPath}
                <img
                  class="stack-avatar"
                  src="/avatars/{sc.avatarPath.split('/').pop()}"
                  alt={sc.name}
                  style="z-index: {3 - i}; margin-left: {i > 0 ? '-8px' : '0'};"
                />
              {:else}
                <div class="stack-avatar placeholder" style="z-index: {3 - i}; margin-left: {i > 0 ? '-8px' : '0'};">
                  <Icon name="user" size={10} />
                </div>
              {/if}
            {/each}
            {#if selectedChars.length === 0}
              <div class="stack-avatar placeholder">
                <Icon name="users" size={12} />
              </div>
            {/if}
            {#if selectedChars.length > 3}
              <span class="stack-more">+{selectedChars.length - 3}</span>
            {/if}
          </div>
          <span class="picker-label">{pickerLabel}</span>
          <span class="picker-chevron" class:open={dropdownOpen}>
            <Icon name="chevron-down" size={13} />
          </span>
        </button>

        {#if dropdownOpen && characters.length > 0}
          <div class="picker-dropdown">
            {#each characters as char}
              <button
                class="picker-option"
                class:selected={selectedCharIds.includes(char.id)}
                onclick={() => toggleChar(char.id)}
              >
                <div class="option-check" class:checked={selectedCharIds.includes(char.id)}>
                  {#if selectedCharIds.includes(char.id)}
                    <Icon name="check" size={10} />
                  {/if}
                </div>
                {#if char.avatarPath}
                  <img
                    class="option-avatar"
                    src="/avatars/{char.avatarPath.split('/').pop()}"
                    alt={char.name}
                  />
                {:else}
                  <div class="option-avatar placeholder">
                    <Icon name="user" size={10} />
                  </div>
                {/if}
                <span>{char.name}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- View switch -->
      <div class="view-switch">
        <button
          class="switch-btn"
          class:active={activeView === 'graph'}
          onclick={() => activeView = 'graph'}
        >
          <Icon name="network" size={14} />
          <span>Graph</span>
        </button>
        <button
          class="switch-btn"
          class:active={activeView === 'timeline'}
          onclick={() => activeView = 'timeline'}
        >
          <Icon name="clock" size={14} />
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
    {:else if selectedCharIds.length === 0}
      <div class="empty-state">
        <div class="empty-icon">
          <Icon name="users" size={32} />
        </div>
        <h2>No Characters Selected</h2>
        <p>Choose one or more characters above to explore their memory multiverse</p>
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
        <p>Chat with {selectedChars.map(c => c.name).join(' & ') || 'these characters'} to start building their memory multiverse</p>
      </div>
    {:else if graphData && activeView === 'graph'}
      <MemoryGraph data={graphData} avatars={Object.fromEntries(selectedChars.map(c => [c.id, c.avatarPath]))} onRefresh={handleRefresh} />
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
    position: relative;
  }

  .picker-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px 4px 4px;
    background: rgba(14, 14, 30, 0.6);
    border: 1px solid rgba(139, 92, 246, 0.1);
    border-radius: 10px;
    cursor: pointer;
    transition: all 200ms;
    font-family: var(--font-body);
  }

  .picker-trigger:hover,
  .picker-trigger:focus {
    border-color: rgba(139, 92, 246, 0.25);
    background: rgba(14, 14, 30, 0.8);
  }

  .picker-trigger:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .avatar-stack {
    display: flex;
    align-items: center;
    padding-left: 2px;
  }

  .stack-avatar {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    object-fit: cover;
    flex-shrink: 0;
    border: 2px solid rgba(10, 10, 26, 0.9);
    position: relative;
  }

  .stack-avatar.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #8B5CF6, #BF40FF);
    color: #fff;
  }

  .stack-more {
    font-size: 10px;
    font-weight: 700;
    color: #8b8ba7;
    margin-left: 2px;
  }

  .picker-label {
    font-size: 13px;
    font-weight: 600;
    color: #e8e0ff;
    min-width: 100px;
    text-align: left;
  }

  .picker-chevron {
    display: flex;
    color: #5a5a7a;
    transition: transform 200ms;
  }

  .picker-chevron.open {
    transform: rotate(180deg);
  }

  /* ── Dropdown Menu ── */
  .picker-dropdown {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    min-width: 220px;
    max-height: 320px;
    overflow-y: auto;
    background: rgba(12, 12, 28, 0.96);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(139, 92, 246, 0.12);
    border-radius: 12px;
    padding: 4px;
    z-index: 100;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(139, 92, 246, 0.06);
    animation: dropIn 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .picker-dropdown::-webkit-scrollbar { width: 3px; }
  .picker-dropdown::-webkit-scrollbar-thumb {
    background: rgba(139, 92, 246, 0.15);
    border-radius: 3px;
  }

  @keyframes dropIn {
    from { opacity: 0; transform: translateY(-6px) scale(0.97); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .picker-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    font-size: 13px;
    font-weight: 500;
    color: #8b8ba7;
    background: none;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: all 150ms;
    font-family: var(--font-body);
    text-align: left;
  }

  .picker-option:hover {
    background: rgba(139, 92, 246, 0.08);
    color: #e8e0ff;
  }

  .picker-option.selected {
    background: rgba(139, 92, 246, 0.12);
    color: #c4a1ff;
  }

  .option-check {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid rgba(139, 92, 246, 0.2);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: all 150ms;
    background: transparent;
  }

  .option-check.checked {
    background: #8B5CF6;
    border-color: #8B5CF6;
    color: #fff;
  }

  .option-avatar {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .option-avatar.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(139, 92, 246, 0.15);
    color: #8b8ba7;
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
