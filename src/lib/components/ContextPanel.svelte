<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import SceneDisplay from './SceneDisplay.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { LorebookEntry } from '$lib/types';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    characterId = null,
    characterName,
    characterTagline,
    avatarUrl = null,
    tags = [],
    conversationId = null,
    onClose,
  }: {
    characterId?: string | null;
    characterName: string;
    characterTagline: string;
    avatarUrl?: string | null;
    tags?: { label: string; color: string }[];
    conversationId?: string | null;
    onClose: () => void;
  } = $props();

  // Lorebook entries — loaded from backend
  let lorebookEntries: LorebookEntry[] = $state([]);
  let isLoadingLore = $state(false);

  // Add entry form
  let showAddLore = $state(false);
  let newLoreName = $state('');
  let newLoreKeys = $state('');
  let newLoreContent = $state('');
  let isSavingLore = $state(false);

  // Memories — persisted via backend
  interface MemoryItem { id: string; content: string; source: string; created_at: string; }
  let memories: MemoryItem[] = $state([]);
  let isLoadingMemories = $state(false);
  let showAddMemory = $state(false);
  let newMemoryText = $state('');
  let isSavingMemory = $state(false);

  // Load lorebook entries when character changes
  $effect(() => {
    if (characterId && isTauri) {
      loadLorebook(characterId);
    } else {
      lorebookEntries = [];
    }
  });

  // Load memories when character changes
  $effect(() => {
    if (characterId && isTauri) {
      loadMemories(characterId);
    } else {
      memories = [];
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
        const data = JSON.parse(char.data);

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

  async function loadMemories(charId: string) {
    isLoadingMemories = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const result = await ipc.listMemories(charId);
      memories = result.map(m => ({
        id: m.id,
        content: m.content,
        source: m.source,
        created_at: m.created_at,
      }));
    } catch (err) {
      console.error('Failed to load memories:', err);
      memories = [];
    }
    isLoadingMemories = false;
  }

  async function addMemory() {
    if (!isTauri || !newMemoryText.trim()) return;
    isSavingMemory = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const created = await ipc.createMemory(
        newMemoryText.trim(),
        characterId ?? undefined,
        conversationId ?? undefined,
        'user',
      );
      memories = [{
        id: created.id,
        content: created.content,
        source: created.source,
        created_at: created.created_at,
      }, ...memories];
      newMemoryText = '';
      showAddMemory = false;
      success('Memory pinned');
    } catch {
      toastError('Failed to save memory');
    }
    isSavingMemory = false;
  }

  async function deleteMemoryEntry(memoryId: string) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteMemory(memoryId);
      memories = memories.filter(m => m.id !== memoryId);
      success('Memory removed');
    } catch {
      toastError('Failed to delete memory');
    }
  }

  function getRelativeTime(dateStr: string): string {
    const d = new Date(dateStr + 'Z');
    const diff = Date.now() - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  function getTagStyle(tag: { label: string; color: string }): string {
    return `background: ${tag.color}1F; color: ${tag.color};`;
  }
</script>

<aside class="context-panel animate-slide-in-right" aria-label="Character context">
  <!-- Header -->
  <div class="ctx-header">
    <span class="ctx-title" id="ctx-character-title">CHARACTER</span>
    <button class="ctx-close" onclick={onClose} aria-label="Close context panel">
      <Icon name="x" size={16} color="var(--fg-muted)" />
    </button>
  </div>

  <!-- Character Card -->
  <div class="char-card" aria-labelledby="ctx-character-title">
    <div class="char-avatar-lg" aria-hidden="true">
      {#if avatarUrl}
        <img src={avatarUrl} alt={characterName} class="ctx-avatar-img" />
      {/if}
    </div>
    <span class="char-name-lg">{characterName}</span>
    <span class="char-tagline">{characterTagline}</span>
    <div class="char-tags">
      {#if tags.length > 0}
        {#each tags as tag (tag.label)}
          <span class="tag" style={getTagStyle(tag)}>{tag.label}</span>
        {/each}
      {:else}
        <span class="tag tag-violet">Fantasy</span>
        <span class="tag tag-pink">Mystery</span>
        <span class="tag tag-cyan">Magic</span>
      {/if}
    </div>
  </div>

  <div class="ctx-divider" role="separator"></div>

  <!-- Scene Display -->
  <SceneDisplay />

  <div class="ctx-divider" role="separator"></div>

  <!-- Lorebook -->
  <section class="ctx-section" aria-labelledby="lorebook-title">
    <div class="ctx-section-header">
      <span class="ctx-section-title" id="lorebook-title">LOREBOOK</span>
      <div class="lore-header-actions">
        <span class="ctx-section-meta">{lorebookEntries.length} entries</span>
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
    {:else}
      {#each lorebookEntries as entry (entry.id)}
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

  <div class="ctx-divider" role="separator"></div>

  <!-- Memories -->
  <section class="ctx-section" aria-labelledby="memories-title">
    <div class="ctx-section-header">
      <span class="ctx-section-title" id="memories-title">MEMORIES</span>
      <div class="lore-header-actions">
        <span class="ctx-section-meta">{memories.length} pinned</span>
        <button
          class="lore-add-btn"
          title="Pin a memory"
          aria-label="Pin a memory"
          onclick={() => showAddMemory = !showAddMemory}
        >
          <Icon name={showAddMemory ? 'x' : 'plus'} size={12} color="var(--accent-primary)" />
        </button>
      </div>
    </div>

    {#if showAddMemory}
      <div class="lore-form">
        <textarea
          class="lore-textarea"
          placeholder="Pin a fact, instruction, or context note..."
          bind:value={newMemoryText}
          rows="2"
          aria-label="Memory content"
        ></textarea>
        <button
          class="lore-save-btn"
          onclick={addMemory}
          disabled={isSavingMemory || !newMemoryText.trim()}
        >
          {isSavingMemory ? 'Saving...' : 'Pin Memory'}
        </button>
      </div>
    {/if}

    {#if isLoadingMemories}
      <div class="lore-loading">
        <span class="loading-dot"></span>
        <span class="loading-dot d2"></span>
        <span class="loading-dot d3"></span>
      </div>
    {:else if memories.length === 0}
      <div class="lore-empty">
        <Icon name="pin" size={16} color="var(--fg-muted)" />
        <span>No memories pinned</span>
      </div>
    {:else}
      {#each memories as memory (memory.id)}
        <div class="memory-entry">
          <span class="memory-text">{memory.content}</span>
          <div class="memory-footer">
            <span class="memory-meta">{getRelativeTime(memory.created_at)}</span>
            <span class="memory-source">{memory.source === 'user' ? '📌 Pinned' : '🤖 Auto'}</span>
            <button
              class="memory-delete"
              onclick={() => deleteMemoryEntry(memory.id)}
              title="Remove memory"
              aria-label="Remove memory"
            >
              <Icon name="x" size={10} color="var(--fg-muted)" />
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </section>
</aside>

<style>
  .context-panel {
    width: var(--context-panel-width); height: 100%;
    background: linear-gradient(175deg, #0c0c1e, #09091a 50%, #07071a);
    border-left: 1px solid rgba(139,92,246,0.08);
    padding: 18px 16px; display: flex; flex-direction: column;
    gap: 16px; overflow-y: auto; flex-shrink: 0;
    animation: ctxSlideIn 350ms cubic-bezier(0.34,1.56,0.64,1) both;
  }
  @keyframes ctxSlideIn {
    from { opacity: 0; transform: translateX(20px); }
    to { opacity: 1; transform: translateX(0); }
  }
  .context-panel::-webkit-scrollbar { width: 3px; }
  .context-panel::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }

  .ctx-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-title {
    font-size: 10px; font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.8px;
  }
  .ctx-close {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: background 150ms;
  }
  .ctx-close:hover { background: rgba(139,92,246,0.08); }

  .char-card {
    display: flex; flex-direction: column; align-items: center;
    gap: 14px; padding: 24px 16px 20px; border-radius: 16px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.1);
    position: relative;
  }
  .char-card::before {
    content: ''; position: absolute; top: -40px; left: 50%; transform: translateX(-50%);
    width: 120px; height: 120px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.15), transparent 70%);
    pointer-events: none;
  }

  .char-avatar-lg {
    width: 82px; height: 82px; min-width: 82px; min-height: 82px;
    border-radius: 50%; aspect-ratio: 1;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    overflow: hidden; position: relative; flex-shrink: 0;
    box-shadow: 0 0 20px rgba(139,92,246,0.25);
  }
  .ctx-avatar-img { width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 50%; }
  .char-name-lg { font-size: 18px; font-weight: 700; color: #e8e0ff; }
  .char-tagline { font-size: 12px; color: #6b6b8a; text-align: center; line-height: 1.5; }

  .char-tags { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .tag {
    padding: 4px 10px; border-radius: 99px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.3px;
  }
  .tag-violet { background: rgba(139,92,246,0.12); color: #c4a1ff; }
  .tag-pink { background: rgba(191,64,255,0.12); color: #d580ff; }
  .tag-cyan { background: rgba(0,242,255,0.12); color: #00f2ff; }

  .ctx-divider {
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.12), transparent);
  }

  .ctx-section { display: flex; flex-direction: column; gap: 10px; }
  .ctx-section-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-section-title {
    font-size: 10px; font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.5px;
  }
  .ctx-section-meta { font-size: 10px; color: #4a4a6a; font-family: var(--font-mono); }

  .lore-header-actions { display: flex; align-items: center; gap: 8px; }
  .lore-add-btn {
    background: none; border: 1px solid rgba(139,92,246,0.12);
    border-radius: 8px; padding: 4px; display: flex; cursor: pointer;
    transition: all 150ms;
  }
  .lore-add-btn:hover { border-color: rgba(139,92,246,0.3); background: rgba(139,92,246,0.06); }

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
  .lore-text { font-size: 12px; color: #8b8ba7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-keys { font-size: 9px; color: #4a4a6a; font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lore-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .lore-dot.active { background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.3); }

  .lore-delete {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex;
  }
  .lore-entry:hover .lore-delete { opacity: 0.5; }
  .lore-delete:hover { opacity: 1 !important; }

  .lore-empty { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: #4a4a6a; font-size: 11px; }
  .lore-loading { display: flex; gap: 4px; padding: 14px; justify-content: center; }
  .loading-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: #5a5a7a; animation: dotPulse 1.2s ease-in-out infinite;
  }
  .loading-dot.d2 { animation-delay: 150ms; }
  .loading-dot.d3 { animation-delay: 300ms; }
  @keyframes dotPulse { 0%,100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1); } }

  .memory-entry {
    display: flex; flex-direction: column; gap: 4px;
    padding: 10px 12px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: all 150ms;
  }
  .memory-entry:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.1); }
  .memory-text { font-size: 11px; color: #8b8ba7; line-height: 1.5; }
  .memory-footer {
    display: flex; align-items: center; gap: 8px; margin-top: 2px;
  }
  .memory-meta { font-size: 9px; color: #4a4a6a; font-family: var(--font-mono); }
  .memory-source { font-size: 9px; color: #5a5a7a; }
  .memory-delete {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex; margin-left: auto;
  }
  .memory-entry:hover .memory-delete { opacity: 0.5; }
  .memory-delete:hover { opacity: 1 !important; }

  @media (max-width: 1024px) { .context-panel { display: none; } }
</style>
