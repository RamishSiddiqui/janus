<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { parseCharacterData } from '$lib/utils/character';
  import type { LorebookEntry } from '$lib/types';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { characterId = null }: { characterId?: string | null } = $props();

  // Lorebook entries — loaded from backend
  let lorebookEntries: LorebookEntry[] = $state([]);
  let isLoadingLore = $state(false);

  // Add entry form
  let showAddLore = $state(false);
  let newLoreName = $state('');
  let newLoreKeys = $state('');
  let newLoreContent = $state('');
  let isSavingLore = $state(false);

  // Lorebook search/filter
  let loreSearch = $state('');
  let loreSearchFocused = $state(false);
  const filteredLorebook = $derived.by(() => {
    const q = loreSearch.trim().toLowerCase();
    if (!q) return lorebookEntries;
    return lorebookEntries.filter(e =>
      e.title.toLowerCase().includes(q) ||
      e.keys.some(k => k.toLowerCase().includes(q)) ||
      e.content?.toLowerCase().includes(q)
    );
  });

  // Load lorebook entries when character changes
  $effect(() => {
    if (characterId && isTauri) {
      loadLorebook(characterId);
    } else {
      lorebookEntries = [];
    }
  });

  async function loadLorebook(charId: string) {
    isLoadingLore = true;
    try {
      const ipc = await import('$lib/services/ipc');

      // First try DB entries
      const dbEntries = await ipc.listLorebookEntries(charId);

      if (dbEntries.length > 0) {
        lorebookEntries = dbEntries.map(e => ({
          id: e.id,
          title: e.name || e.keys[0] || 'Untitled',
          keys: e.keys,
          content: e.content,
          isActive: e.enabled,
          alwaysActive: e.always_active,
        }));
      } else {
        // Fallback: parse from character card's embedded lorebook
        const char = await ipc.getCharacter(charId);
        const data = parseCharacterData(char.data);

        if (data.character_book?.entries?.length) {
          lorebookEntries = data.character_book.entries.map((entry: any, i: number) => ({
            id: entry.name || `lore-${i}`,
            title: entry.name || entry.keys?.[0] || `Entry ${i + 1}`,
            keys: entry.keys || [],
            content: entry.content || '',
            isActive: entry.enabled !== false,
            alwaysActive: entry.constant === true,
          }));
        } else {
          lorebookEntries = [];
        }
      }
    } catch (err) {
      console.error('Failed to load lorebook:', err);
      lorebookEntries = [];
    }
    isLoadingLore = false;
  }

  async function toggleEntry(entryId: string) {
    if (!isTauri) return;
    const entry = lorebookEntries.find(e => e.id === entryId);
    if (!entry) return;

    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.toggleLorebookEntry(entryId, !entry.isActive);
      lorebookEntries = lorebookEntries.map(e =>
        e.id === entryId ? { ...e, isActive: !e.isActive } : e
      );
    } catch {
      toastError('Failed to toggle entry');
    }
  }

  async function deleteEntry(entryId: string) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteLorebookEntry(entryId);
      const name = lorebookEntries.find(e => e.id === entryId)?.title ?? 'Entry';
      lorebookEntries = lorebookEntries.filter(e => e.id !== entryId);
      success(`Removed ${name}`);
    } catch {
      toastError('Failed to delete entry');
    }
  }

  async function addEntry() {
    if (!isTauri || !newLoreName.trim() || !newLoreContent.trim()) return;
    isSavingLore = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const keys = newLoreKeys.split(',').map(k => k.trim()).filter(Boolean);
      const created = await ipc.createLorebookEntry(
        characterId ?? null,
        newLoreName.trim(),
        keys,
        newLoreContent.trim(),
        false,
      );
      lorebookEntries = [...lorebookEntries, {
        id: created.id,
        title: created.name || keys[0] || 'Untitled',
        keys: created.keys,
        content: created.content,
        isActive: true,
        alwaysActive: created.always_active,
      }];
      newLoreName = '';
      newLoreKeys = '';
      newLoreContent = '';
      showAddLore = false;
      success('Lorebook entry added');
    } catch {
      toastError('Failed to add entry');
    }
    isSavingLore = false;
  }
</script>

<section class="ctx-section" aria-labelledby="lorebook-title">
  <div class="ctx-section-header">
    <span class="ctx-section-title" id="lorebook-title">LOREBOOK</span>
    <div class="lore-header-actions">
      <span class="ctx-section-meta">{loreSearch ? `${filteredLorebook.length}/${lorebookEntries.length}` : `${lorebookEntries.length} entries`}</span>
      <button
        class="lore-add-btn"
        title="Add lorebook entry"
        aria-label="Add lorebook entry"
        onclick={() => showAddLore = !showAddLore}
      >
        <Icon name={showAddLore ? 'x' : 'plus'} size={12} color="var(--accent-primary)" />
      </button>
    </div>
  </div>

  <!-- Search/Filter -->
  {#if lorebookEntries.length > 2}
    <div class="lore-search" class:focused={loreSearchFocused}>
      <Icon name="search" size={11} color={loreSearchFocused ? '#c4a1ff' : '#6b6b8a'} />
      <input
        type="text"
        placeholder="Filter entries..."
        bind:value={loreSearch}
        onfocus={() => loreSearchFocused = true}
        onblur={() => loreSearchFocused = false}
        aria-label="Search lorebook entries"
      />
      {#if loreSearch}
        <button class="lore-search-clear" onclick={() => loreSearch = ''} aria-label="Clear search">
          <Icon name="x" size={10} color="#6b6b8a" />
        </button>
      {/if}
    </div>
  {/if}

  <!-- Add Entry Form -->
  {#if showAddLore}
    <div class="lore-form">
      <input
        class="lore-input"
        placeholder="Entry name"
        bind:value={newLoreName}
        aria-label="Entry name"
      />
      <input
        class="lore-input"
        placeholder="Keywords (comma separated)"
        bind:value={newLoreKeys}
        aria-label="Trigger keywords"
      />
      <textarea
        class="lore-textarea"
        placeholder="Content to inject when keywords match..."
        bind:value={newLoreContent}
        rows="3"
        aria-label="Entry content"
      ></textarea>
      <button
        class="lore-save-btn"
        onclick={addEntry}
        disabled={isSavingLore || !newLoreName.trim() || !newLoreContent.trim()}
      >
        {isSavingLore ? 'Adding...' : 'Add Entry'}
      </button>
    </div>
  {/if}

  {#if isLoadingLore}
    <div class="lore-loading">
      <span class="loading-dot"></span>
      <span class="loading-dot d2"></span>
      <span class="loading-dot d3"></span>
    </div>
  {:else if lorebookEntries.length === 0}
    <div class="lore-empty">
      <Icon name="book-open" size={16} color="var(--fg-muted)" />
      <span>No lorebook entries</span>
    </div>
  {:else if filteredLorebook.length === 0}
    <div class="lore-empty">
      <Icon name="search" size={16} color="var(--fg-muted)" />
      <span>No matches for "{loreSearch}"</span>
    </div>
  {:else}
    {#each filteredLorebook as entry (entry.id)}
      <div class="lore-entry" class:inactive={!entry.isActive}>
        <button
          class="lore-toggle"
          onclick={() => toggleEntry(entry.id)}
          title={entry.isActive ? 'Disable entry' : 'Enable entry'}
          aria-label={entry.isActive ? `Disable ${entry.title}` : `Enable ${entry.title}`}
        >
          <Icon
            name="book-open"
            size={12}
            color={entry.isActive ? 'var(--accent-primary)' : 'var(--fg-muted)'}
          />
        </button>
        <div class="lore-info">
          <span class="lore-text">{entry.title}</span>
          {#if entry.keys.length > 0}
            <span class="lore-keys">{entry.keys.slice(0, 3).join(', ')}</span>
          {/if}
        </div>
        {#if entry.isActive}
          <span class="lore-dot active" aria-label="Active"></span>
        {/if}
        <button
          class="lore-delete"
          onclick={() => deleteEntry(entry.id)}
          title="Delete entry"
          aria-label={`Delete ${entry.title}`}
        >
          <Icon name="x" size={10} color="var(--fg-muted)" />
        </button>
      </div>
    {/each}
  {/if}
</section>

<style>
  .ctx-section { display: flex; flex-direction: column; gap: 10px; }
  .ctx-section-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-section-title {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.5px;
  }
  .ctx-section-meta { font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono); }

  .lore-header-actions { display: flex; align-items: center; gap: 8px; }
  .lore-add-btn {
    background: none; border: 1px solid rgba(139,92,246,0.12);
    border-radius: 8px; padding: 4px; display: flex; cursor: pointer;
    transition: all 150ms;
  }
  .lore-add-btn:hover { border-color: rgba(139,92,246,0.3); background: rgba(139,92,246,0.06); }

  .lore-search {
    display: flex; align-items: center; gap: 6px;
    height: 30px; padding: 0 10px; border-radius: 8px;
    background: rgba(9,9,26,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    transition: all 200ms;
  }
  .lore-search.focused {
    border-color: rgba(139,92,246,0.25);
    background: rgba(14,14,30,0.7);
    box-shadow: 0 0 0 3px rgba(139,92,246,0.04);
  }
  .lore-search input {
    flex: 1; background: none; border: none; outline: none;
    color: #c8c8e0; font-size: 11px; font-family: var(--font-body);
    min-width: 0;
  }
  .lore-search input::placeholder { color: #4a4a6a; }
  .lore-search-clear {
    background: none; border: none; padding: 2px; cursor: pointer;
    display: flex; opacity: 0.5; transition: opacity 150ms;
  }
  .lore-search-clear:hover { opacity: 1; }

  .lore-form {
    display: flex; flex-direction: column; gap: 6px;
    padding: 12px; border-radius: 12px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.1);
  }
  .lore-input, .lore-textarea {
    padding: 8px 10px; border-radius: 8px;
    background: rgba(9,9,26,0.6); border: 1px solid rgba(139,92,246,0.08);
    font-size: 11px; font-family: var(--font-body); color: #e0e0f0;
    outline: none; transition: border-color 150ms;
  }
  .lore-input:focus, .lore-textarea:focus { border-color: rgba(139,92,246,0.35); }
  .lore-textarea { resize: vertical; }

  .lore-save-btn {
    align-self: flex-end; padding: 6px 14px; border-radius: 8px;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); border: none;
    color: #fff; font-size: 11px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: opacity 150ms, box-shadow 150ms;
    box-shadow: 0 2px 10px rgba(139,92,246,0.2);
  }
  .lore-save-btn:hover:not(:disabled) { box-shadow: 0 4px 16px rgba(139,92,246,0.35); }
  .lore-save-btn:disabled { opacity: 0.35; cursor: default; }

  .lore-entry {
    display: flex; align-items: center; gap: 8px;
    padding: 9px 12px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: all 150ms;
  }
  .lore-entry:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.1); }
  .lore-entry.inactive { opacity: 0.4; }

  .lore-toggle { background: none; border: none; padding: 2px; cursor: pointer; flex-shrink: 0; display: flex; }
  .lore-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .lore-text { font-size: var(--text-sm); color: #8b8ba7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-keys { font-size: 9px; color: #4a4a6a; font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .lore-dot.active { background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.3); }

  .lore-delete {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex;
  }
  .lore-entry:hover .lore-delete { opacity: 0.5; }
  .lore-delete:hover { opacity: 1 !important; }

  .lore-empty { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: #4a4a6a; font-size: var(--text-sm); }
  .lore-loading { display: flex; gap: 4px; padding: 14px; justify-content: center; }
  .loading-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: #5a5a7a; animation: dotPulse 1.2s ease-in-out infinite;
  }
  .loading-dot.d2 { animation-delay: 150ms; }
  .loading-dot.d3 { animation-delay: 300ms; }
  @keyframes dotPulse { 0%,100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1); } }
</style>
