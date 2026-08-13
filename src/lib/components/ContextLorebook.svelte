<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { parseCharacterData } from '$lib/utils/character';
  import type { LorebookEntry } from '$lib/types';
  import {
    sceneGenerations, getSceneGenerationState, trackGenerationByKey,
  } from '$lib/stores/sceneGeneration';

  const generateKey = (characterId: string) => `lorebook-generate-${characterId}`;

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    characterId = null,
    conversationId = null,
    wide = false,
  }: {
    characterId?: string | null;
    conversationId?: string | null;
    /** Renders entries in a responsive grid instead of a single stacked
     *  column — used when this panel fills the full chat area (see
     *  ChatExplorerView.svelte) rather than a narrow header popover. */
    wide?: boolean;
  } = $props();

  // Lorebook entries — loaded from backend
  let lorebookEntries: LorebookEntry[] = $state([]);
  let isLoadingLore = $state(false);

  // Add entry form
  let showAddLore = $state(false);
  let newLoreName = $state('');
  let newLoreKeys = $state('');
  let newLoreContent = $state('');
  let newLoreAlwaysActive = $state(false);
  let isSavingLore = $state(false);

  // Edit entry (inline, replaces the row) — there was previously no way to
  // change an entry's fields after creation at all, only toggle on/off or
  // delete-and-recreate.
  let editingId: string | null = $state(null);
  let editName = $state('');
  let editKeys = $state('');
  let editContent = $state('');
  let editAlwaysActive = $state(false);
  let editPriority = $state(10);
  let isSavingEdit = $state(false);

  let isImportingBook = $state(false);

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
          priority: e.priority,
          insertionOrder: e.insertion_order,
        }));
      } else {
        // Fallback: parse from character card's embedded lorebook. These
        // are display-only — not real lorebook_entries rows, so they can't
        // be edited/toggled/deleted/reordered until actually imported (see
        // importFromCard below). Marked isVirtual so the template can grey
        // out those controls and surface the Import CTA instead.
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
            priority: entry.priority ?? 10,
            insertionOrder: entry.insertion_order ?? 100,
            isVirtual: true,
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
    if (!entry || entry.isVirtual) return;

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
    const entry = lorebookEntries.find(e => e.id === entryId);
    if (entry?.isVirtual) return;
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
        newLoreAlwaysActive,
      );
      lorebookEntries = [...lorebookEntries, {
        id: created.id,
        title: created.name || keys[0] || 'Untitled',
        keys: created.keys,
        content: created.content,
        isActive: true,
        alwaysActive: created.always_active,
        priority: created.priority,
        insertionOrder: created.insertion_order,
      }];
      newLoreName = '';
      newLoreKeys = '';
      newLoreContent = '';
      newLoreAlwaysActive = false;
      showAddLore = false;
      success('Lorebook entry added');
    } catch {
      toastError('Failed to add entry');
    }
    isSavingLore = false;
  }

  function startEdit(entry: LorebookEntry) {
    editingId = entry.id;
    editName = entry.title;
    editKeys = entry.keys.join(', ');
    editContent = entry.content;
    editAlwaysActive = entry.alwaysActive;
    editPriority = entry.priority;
  }

  function cancelEdit() {
    editingId = null;
  }

  async function saveEdit(entry: LorebookEntry) {
    if (!isTauri || !editName.trim() || !editContent.trim()) return;
    isSavingEdit = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const keys = editKeys.split(',').map(k => k.trim()).filter(Boolean);
      const updated = await ipc.updateLorebookEntry(
        entry.id, editName.trim(), keys, editContent.trim(),
        editAlwaysActive, editPriority, entry.insertionOrder,
      );
      lorebookEntries = lorebookEntries.map(e => e.id === entry.id ? {
        ...e,
        title: updated.name || keys[0] || 'Untitled',
        keys: updated.keys,
        content: updated.content,
        alwaysActive: updated.always_active,
        priority: updated.priority,
        insertionOrder: updated.insertion_order,
      } : e);
      editingId = null;
      success('Lorebook entry updated');
    } catch {
      toastError('Failed to update entry');
    }
    isSavingEdit = false;
  }

  /** Swaps insertion_order with the adjacent entry in display order — a
   *  simple up/down reorder rather than full drag-and-drop, but it's the
   *  first reordering UI this feature has had at all (insertion_order
   *  existed in the schema and was used to sort entries, but nothing ever
   *  let the user actually change it). Only meaningful when not filtering —
   *  the template hides these controls while a search is active. */
  async function moveEntry(entry: LorebookEntry, direction: 'up' | 'down') {
    if (!isTauri) return;
    const idx = lorebookEntries.findIndex(e => e.id === entry.id);
    const targetIdx = direction === 'up' ? idx - 1 : idx + 1;
    if (idx < 0 || targetIdx < 0 || targetIdx >= lorebookEntries.length) return;
    const target = lorebookEntries[targetIdx];
    if (target.isVirtual || entry.isVirtual) return;

    try {
      const ipc = await import('$lib/services/ipc');
      const [newOrderA, newOrderB] = [target.insertionOrder, entry.insertionOrder];
      await Promise.all([
        ipc.updateLorebookEntry(entry.id, entry.title, entry.keys, entry.content, entry.alwaysActive, entry.priority, newOrderA),
        ipc.updateLorebookEntry(target.id, target.title, target.keys, target.content, target.alwaysActive, target.priority, newOrderB),
      ]);
      const next = [...lorebookEntries];
      next[idx] = { ...entry, insertionOrder: newOrderA };
      next[targetIdx] = { ...target, insertionOrder: newOrderB };
      [next[idx], next[targetIdx]] = [next[targetIdx], next[idx]];
      lorebookEntries = next;
    } catch {
      toastError('Failed to reorder entries');
    }
  }

  /** Imports the character card's embedded lorebook (if any) as real,
   *  persisted entries — for characters imported before this app actually
   *  did that automatically. Safe to click even with nothing to import:
   *  the backend just returns an empty list. */
  async function importFromCard() {
    if (!isTauri || !characterId) return;
    isImportingBook = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const imported = await ipc.importCharacterBookEntries(characterId);
      if (imported.length === 0) {
        toastError('No embedded lorebook found on this character card');
      } else {
        success(`Imported ${imported.length} lorebook ${imported.length === 1 ? 'entry' : 'entries'} from the character card`);
        await loadLorebook(characterId);
      }
    } catch {
      toastError('Failed to import character card lorebook');
    }
    isImportingBook = false;
  }

  /** Generates new lorebook entries via the LLM, grounded in this
   *  character's profile, known story facts, and recent dialogue — so
   *  introducing a new character doesn't require hand-writing their
   *  lorebook every time. Skips facets already covered by existing entries. */
  async function generateFromStory() {
    if (!isTauri || !characterId || !conversationId) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const generated = await trackGenerationByKey(generateKey(characterId), () =>
        ipc.generateCharacterLorebook(characterId!, conversationId!)
      );
      if (generated.length === 0) {
        toastError('Not enough story context yet to generate lorebook entries');
      } else {
        success(`Generated ${generated.length} lorebook ${generated.length === 1 ? 'entry' : 'entries'} from the story`);
        await loadLorebook(characterId);
      }
    } catch {
      toastError('Failed to generate lorebook from story');
    }
  }
</script>

<section class="ctx-section" aria-labelledby="lorebook-title">
  <div class="ctx-section-header">
    <span class="ctx-section-title" id="lorebook-title">LOREBOOK</span>
    <div class="lore-header-actions">
      <span class="ctx-section-meta">{loreSearch ? `${filteredLorebook.length}/${lorebookEntries.length}` : `${lorebookEntries.length} entries`}</span>
      {#if characterId && conversationId && isTauri}
        {@const generateState = getSceneGenerationState($sceneGenerations, generateKey(characterId))}
        <button
          class="lore-add-btn lore-generate-btn"
          title="Generate new lorebook entries from this character's profile and how they've appeared in the story"
          aria-label="Generate lorebook from story"
          onclick={generateFromStory}
          disabled={generateState.isLoading}
        >
          <Icon name="sparkles" size={12} color="var(--accent-primary)" />
        </button>
      {/if}
      {#if characterId && isTauri}
        <button
          class="lore-add-btn"
          title="Import lorebook embedded in this character's card"
          aria-label="Import lorebook from character card"
          onclick={importFromCard}
          disabled={isImportingBook}
        >
          <Icon name="download" size={12} color="var(--accent-primary)" />
        </button>
      {/if}
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

  {#if lorebookEntries.some(e => e.isVirtual)}
    <div class="lore-virtual-banner">
      <Icon name="info" size={12} color="#F59E0B" />
      <span>These entries are shown from the character card but aren't active in chat yet — click <Icon name="download" size={10} color="#F59E0B" /> above to import them for real.</span>
    </div>
  {/if}

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
      <label class="lore-checkbox-row">
        <input type="checkbox" bind:checked={newLoreAlwaysActive} />
        <span>Always active (ignore keywords, inject every message)</span>
      </label>
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
    <div class="cards-grid" class:wide>
    {#each filteredLorebook as entry, i (entry.id)}
      {#if editingId === entry.id}
        <div class="lore-form lore-edit-form">
          <input class="lore-input" placeholder="Entry name" bind:value={editName} aria-label="Edit entry name" />
          <input class="lore-input" placeholder="Keywords (comma separated)" bind:value={editKeys} aria-label="Edit trigger keywords" />
          <textarea class="lore-textarea" placeholder="Content to inject when keywords match..." bind:value={editContent} rows="3" aria-label="Edit entry content"></textarea>
          <label class="lore-checkbox-row">
            <input type="checkbox" bind:checked={editAlwaysActive} />
            <span>Always active (ignore keywords, inject every message)</span>
          </label>
          <label class="lore-priority-row">
            <span>Priority</span>
            <input type="number" class="lore-priority-input" bind:value={editPriority} aria-label="Priority" />
          </label>
          <div class="lore-edit-actions">
            <button class="lore-cancel-btn" onclick={cancelEdit}>Cancel</button>
            <button
              class="lore-save-btn"
              onclick={() => saveEdit(entry)}
              disabled={isSavingEdit || !editName.trim() || !editContent.trim()}
            >
              {isSavingEdit ? 'Saving...' : 'Save'}
            </button>
          </div>
        </div>
      {:else}
        <div class="lore-entry" class:inactive={!entry.isActive} class:virtual={entry.isVirtual}>
          <button
            class="lore-toggle"
            onclick={() => toggleEntry(entry.id)}
            disabled={entry.isVirtual}
            title={entry.isVirtual ? 'Import this entry first to enable/disable it' : entry.isActive ? 'Disable entry' : 'Enable entry'}
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
            {#if entry.alwaysActive}
              <span class="lore-always-badge">ALWAYS</span>
            {/if}
            {#if entry.keys.length > 0}
              <span class="lore-keys">{entry.keys.slice(0, 3).join(', ')}</span>
            {/if}
          </div>
          {#if entry.isActive}
            <span class="lore-dot active" aria-label="Active"></span>
          {/if}
          {#if !entry.isVirtual && !loreSearch}
            <div class="lore-reorder">
              <button class="lore-move-btn" onclick={() => moveEntry(entry, 'up')} disabled={i === 0} title="Move up" aria-label={`Move ${entry.title} up`}>
                <Icon name="chevron-up" size={10} color="var(--fg-muted)" />
              </button>
              <button class="lore-move-btn" onclick={() => moveEntry(entry, 'down')} disabled={i === filteredLorebook.length - 1} title="Move down" aria-label={`Move ${entry.title} down`}>
                <Icon name="chevron-down" size={10} color="var(--fg-muted)" />
              </button>
            </div>
          {/if}
          {#if !entry.isVirtual}
            <button
              class="lore-edit-btn"
              onclick={() => startEdit(entry)}
              title="Edit entry"
              aria-label={`Edit ${entry.title}`}
            >
              <Icon name="pencil" size={10} color="var(--fg-muted)" />
            </button>
          {/if}
          <button
            class="lore-delete"
            onclick={() => deleteEntry(entry.id)}
            disabled={entry.isVirtual}
            title={entry.isVirtual ? 'Import this entry first to delete it' : 'Delete entry'}
            aria-label={`Delete ${entry.title}`}
          >
            <Icon name="x" size={10} color="var(--fg-muted)" />
          </button>
        </div>
      {/if}
    {/each}
    </div>
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
  .lore-add-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }

  /* "Generate from Story" is an AI-driven action, not a plain utility one —
     same glow treatment as ContextNpcPanel's "Refresh from Story". */
  .lore-generate-btn:hover:not(:disabled) {
    border-color: rgba(139,92,246,0.5);
    background: rgba(139,92,246,0.1);
    box-shadow: 0 0 10px rgba(139,92,246,0.2);
  }

  .lore-virtual-banner {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 8px 10px; border-radius: 8px;
    background: rgba(245,158,11,0.08); border: 1px solid rgba(245,158,11,0.2);
    font-size: 10px; color: #F59E0B; line-height: 1.5;
  }
  .lore-virtual-banner :global(svg) { flex-shrink: 0; margin-top: 2px; }

  .lore-search {
    display: flex; align-items: center; gap: 6px;
    height: clamp(28px, 8cqi, 36px); padding: 0 clamp(8px, 2.4cqi, 14px); border-radius: 8px;
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
    padding: clamp(10px, 3cqi, 16px); border-radius: 12px;
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

  .lore-checkbox-row {
    display: flex; align-items: center; gap: 6px;
    font-size: 10px; color: #8b8ba7; cursor: pointer; user-select: none;
  }
  .lore-checkbox-row input { accent-color: #8B5CF6; cursor: pointer; }

  .lore-priority-row {
    display: flex; align-items: center; gap: 8px;
    font-size: 10px; color: #8b8ba7;
  }
  .lore-priority-input {
    width: 56px; padding: 5px 8px; border-radius: 6px;
    background: rgba(9,9,26,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 11px; font-family: var(--font-body);
    outline: none; transition: border-color 150ms;
  }
  .lore-priority-input:focus { border-color: rgba(139,92,246,0.35); }

  .lore-edit-form { grid-column: 1 / -1; }
  .lore-edit-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .lore-cancel-btn {
    padding: 6px 14px; border-radius: 8px;
    background: transparent; border: 1px solid rgba(139,92,246,0.12);
    color: #8b8ba7; font-size: 11px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms;
  }
  .lore-cancel-btn:hover { background: rgba(139,92,246,0.06); color: #c8c8e0; }

  /* `display: contents` by default keeps existing stacked-column behavior
     in the narrow popover; `.wide` (full chat-area explorer view) switches
     to an actual grid of entries. */
  .cards-grid { display: contents; }
  .cards-grid.wide {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 10px;
    align-items: start;
  }

  .lore-entry {
    display: flex; align-items: center; gap: 8px;
    padding: clamp(8px, 2.2cqi, 12px) clamp(10px, 3cqi, 16px); border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: all 150ms;
  }
  .lore-entry:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.1); }
  .lore-entry.inactive { opacity: 0.4; }
  .lore-entry.virtual { border-style: dashed; border-color: rgba(245,158,11,0.25); }

  .lore-toggle { background: none; border: none; padding: 2px; cursor: pointer; flex-shrink: 0; display: flex; }
  .lore-toggle:disabled { cursor: default; opacity: 0.5; }
  .lore-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .lore-text { font-size: var(--text-sm); color: #8b8ba7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-always-badge {
    align-self: flex-start; padding: 1px 5px; border-radius: 4px;
    font-size: 8px; font-weight: 700; letter-spacing: 0.4px;
    background: rgba(139,92,246,0.14); color: #c4a1ff;
  }
  .lore-keys { font-size: 9px; color: #4a4a6a; font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-dot { width: clamp(5px, 1.4cqi, 8px); height: clamp(5px, 1.4cqi, 8px); border-radius: 50%; flex-shrink: 0; }
  .lore-dot.active { background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.3); }

  .lore-reorder { display: flex; flex-direction: column; flex-shrink: 0; opacity: 0; transition: opacity 150ms; }
  .lore-entry:hover .lore-reorder { opacity: 0.6; }
  .lore-move-btn { background: none; border: none; padding: 0; cursor: pointer; display: flex; line-height: 0; }
  .lore-move-btn:disabled { opacity: 0.25; cursor: default; }
  .lore-move-btn:not(:disabled):hover { opacity: 1; }

  .lore-edit-btn {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex;
  }
  .lore-entry:hover .lore-edit-btn { opacity: 0.5; }
  .lore-edit-btn:hover { opacity: 1 !important; }

  .lore-delete {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex;
  }
  .lore-entry:hover .lore-delete { opacity: 0.5; }
  .lore-delete:hover:not(:disabled) { opacity: 1 !important; }
  .lore-delete:disabled { cursor: default; }

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
