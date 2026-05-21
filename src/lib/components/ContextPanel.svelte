<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import SceneDisplay from './SceneDisplay.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { parseCharacterData } from '$lib/utils/character';
  import type { LorebookEntry } from '$lib/types';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    characterId = null,
    characterName,
    characterTagline,
    avatarUrl = null,
    tags = [],
    additionalCharacters = [],
    conversationId = null,
    onClose,
  }: {
    characterId?: string | null;
    characterName: string;
    characterTagline: string;
    avatarUrl?: string | null;
    tags?: { label: string; color: string }[];
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
    conversationId?: string | null;
    onClose: () => void;
  } = $props();

  // Carousel state for multi-character conversations
  let activeCardIndex = $state(0);
  interface CharCard { id: string | null; name: string; tagline: string; avatarUrl: string | null; avatarColor: string; tags: { label: string; color: string }[] }
  let allCards = $derived.by((): CharCard[] => {
    const primary: CharCard = { id: characterId, name: characterName, tagline: characterTagline, avatarUrl, avatarColor: '#8B5CF6', tags: tags || [] };
    if (!additionalCharacters || additionalCharacters.length === 0) return [primary];
    return [primary, ...additionalCharacters.map(c => ({ id: c.id, name: c.name, tagline: c.description || '', avatarUrl: c.avatarUrl, avatarColor: c.avatarColor, tags: [] }))];
  });
  let isMultiChar = $derived(allCards.length > 1);
  function nextCard() { activeCardIndex = (activeCardIndex + 1) % allCards.length; }
  function prevCard() { activeCardIndex = (activeCardIndex - 1 + allCards.length) % allCards.length; }

  // Reset carousel when conversation changes
  $effect(() => {
    characterId;  // track
    activeCardIndex = 0;
  });

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

  // Memories — persisted via backend
  interface MemoryItem {
    id: string;
    content: string;
    source: string;
    is_canon: boolean;
    conversation_id: string | null;
    created_at: string;
  }
  let memories: MemoryItem[] = $state([]);
  let isLoadingMemories = $state(false);
  let showAddMemory = $state(false);
  let newMemoryText = $state('');
  let isSavingMemory = $state(false);

  // Memory extraction toggle — 'none' = disabled, 'character' = enabled
  let memoryScope = $state<'character' | 'conversation' | 'none'>('none');
  let isTogglingMemory = $state(false);
  let memoryEnabled = $derived(memoryScope !== 'none');

  // Load lorebook entries when character changes
  $effect(() => {
    if (characterId && isTauri) {
      loadLorebook(characterId);
    } else {
      lorebookEntries = [];
    }
  });

  // Load memories + scope when character or conversation changes
  $effect(() => {
    // Track both — conversationId changes when switching convos within same character
    const _conv = conversationId;
    if (characterId && isTauri) {
      loadMemories(characterId);
    } else {
      memories = [];
    }
    if (_conv && isTauri) {
      loadMemoryScope(_conv);
    } else {
      memoryScope = 'none';
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

  async function loadMemoryScope(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const conv = await ipc.getConversation(convId);
      memoryScope = conv.memory_scope;
    } catch {
      memoryScope = 'none';
    }
  }

  async function toggleMemoryExtraction() {
    if (!conversationId || isTogglingMemory) return;
    isTogglingMemory = true;
    const newScope = memoryEnabled ? 'none' : 'character';
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.setMemoryScope(conversationId, newScope);
      memoryScope = newScope;
      success(newScope === 'none' ? 'Memory extraction disabled' : 'Memory extraction enabled');
    } catch {
      toastError('Failed to update memory settings');
    }
    isTogglingMemory = false;
  }

  async function loadMemories(charId: string) {
    isLoadingMemories = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const result = await ipc.listMemories(charId);
      memories = result
        // Only show canon memories OR memories belonging to THIS conversation.
        // Memories from other conversations are intentionally excluded —
        // they live in their own timeline and should not bleed across.
        .filter(m => m.is_canon || m.conversation_id === conversationId)
        .map(m => ({
          id: m.id,
          content: m.content,
          source: m.source,
          is_canon: m.is_canon,
          conversation_id: m.conversation_id ?? null,
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
        is_canon: false,
        conversation_id: conversationId,
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
    <span class="ctx-title" id="ctx-character-title">{isMultiChar ? 'CHARACTERS' : 'CHARACTER'}</span>
    <button class="ctx-close" onclick={onClose} aria-label="Close context panel">
      <Icon name="x" size={16} color="var(--fg-muted)" />
    </button>
  </div>

  <!-- Character Carousel -->
  <div class="char-carousel" class:multi={isMultiChar}>
    <div class="carousel-track" style="transform: translateX(-{activeCardIndex * 100}%)">
      {#each allCards as card, i (card.id ?? i)}
        <div class="carousel-slide">
          <div class="char-card" style="--card-accent: {card.avatarColor}">
            <div class="char-avatar-lg" style="background: linear-gradient(135deg, {card.avatarColor}, {card.avatarColor}cc)" aria-hidden="true">
              {#if card.avatarUrl}
                <img src={card.avatarUrl} alt={card.name} class="ctx-avatar-img" />
              {/if}
            </div>
            <span class="char-name-lg">{card.name}</span>
            {#if card.tagline}
              <span class="char-tagline">{card.tagline}</span>
            {/if}
            <div class="char-tags">
              {#if card.tags && card.tags.length > 0}
                {#each card.tags as tag (tag.label)}
                  <span class="tag" style={getTagStyle(tag)}>{tag.label}</span>
                {/each}
              {:else if i === 0}
                <span class="tag tag-violet">Fantasy</span>
                <span class="tag tag-pink">Mystery</span>
                <span class="tag tag-cyan">Magic</span>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>

    {#if isMultiChar}
      <div class="carousel-nav">
        <button class="carousel-nav-btn" onclick={prevCard} aria-label="Previous character">
          <Icon name="chevron-left" size={12} color="#8b8ba7" />
        </button>
        <div class="carousel-dots">
          {#each allCards as card, i}
            <button class="carousel-dot" class:active={activeCardIndex === i}
              onclick={() => activeCardIndex = i}
              aria-label="View {card.name}">
              {#if card.avatarUrl}
                <img src={card.avatarUrl} alt="" class="dot-avatar" />
              {:else}
                <div class="dot-color" style="background: {card.avatarColor}"></div>
              {/if}
            </button>
          {/each}
        </div>
        <button class="carousel-nav-btn" onclick={nextCard} aria-label="Next character">
          <Icon name="chevron-right" size={12} color="#8b8ba7" />
        </button>
      </div>
    {/if}
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

  <div class="ctx-divider" role="separator"></div>

  <!-- Memories -->
  <section class="ctx-section" aria-labelledby="memories-title">
    <div class="ctx-section-header">
      <span class="ctx-section-title" id="memories-title">MEMORIES</span>
      <div class="lore-header-actions">
        {#if characterId}
          <a class="graph-link" href={`/memories?character=${characterId}`} title="Open memory graph">
            <Icon name="git-branch" size={11} color="var(--accent-primary)" />
            Graph
          </a>
        {/if}
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

    <!-- Enable Memory toggle -->
    {#if conversationId}
      <button
        class="mem-toggle-row"
        class:enabled={memoryEnabled}
        onclick={toggleMemoryExtraction}
        disabled={isTogglingMemory}
        title={memoryEnabled ? 'Disable auto memory extraction for this conversation' : 'Enable auto memory extraction for this conversation'}
        aria-label="Toggle memory extraction"
        aria-pressed={memoryEnabled}
      >
        <span class="mem-toggle-icon" aria-hidden="true">
          {memoryEnabled ? '🧠' : '💤'}
        </span>
        <span class="mem-toggle-label">Enable Memory</span>
        <span class="mem-toggle-pill" class:on={memoryEnabled}>
          <span class="mem-toggle-knob"></span>
        </span>
      </button>
    {/if}

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
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.8px;
  }
  .ctx-close {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: background 150ms;
  }
  .ctx-close:hover { background: rgba(139,92,246,0.08); }

  /* ══ Character Carousel ══ */
  .char-carousel {
    position: relative;
    overflow: hidden;
    border-radius: 16px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.1);
    width: 100%;
    flex-shrink: 0;
  }
  .char-carousel.multi {
    border-color: rgba(0,212,224,0.12);
    background: rgba(10,14,28,0.7);
  }

  .carousel-track {
    display: flex;
    transition: transform 400ms cubic-bezier(0.4, 0, 0.2, 1);
    will-change: transform;
  }
  .carousel-slide {
    min-width: 100%; width: 100%; max-width: 100%; flex-shrink: 0;
  }

  .char-card {
    display: flex; flex-direction: column; align-items: center;
    gap: 14px; padding: 24px 16px 20px;
    position: relative; width: 100%; max-width: 100%;
    box-sizing: border-box;
  }
  .char-card::before {
    content: ''; position: absolute; top: -40px; left: 50%; transform: translateX(-50%);
    width: 120px; height: 120px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.15), transparent 70%);
    pointer-events: none;
  }

  /* Navigation Strip — compact bottom bar */
  .carousel-nav {
    display: flex; align-items: center; justify-content: center;
    gap: 10px; padding: 4px 0 12px;
  }
  .carousel-nav-btn {
    width: 24px; height: 24px; border-radius: 50%;
    border: 1px solid rgba(139,92,246,0.12);
    background: rgba(139,92,246,0.04);
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 180ms ease-out;
    flex-shrink: 0; padding: 0;
  }
  .carousel-nav-btn:hover {
    border-color: rgba(0,212,224,0.3);
    background: rgba(0,212,224,0.08);
    transform: scale(1.1);
  }
  .carousel-nav-btn:active { transform: scale(0.9); }

  /* Avatar Dot Indicators */
  .carousel-dots {
    display: flex; align-items: center; gap: 8px;
  }
  .carousel-dot {
    width: 22px; height: 22px; border-radius: 50%;
    padding: 0; cursor: pointer; overflow: hidden;
    border: 2px solid rgba(139,92,246,0.12);
    background: transparent;
    transition: all 250ms ease-out;
    opacity: 0.5; transform: scale(0.85);
  }
  .carousel-dot:hover {
    opacity: 0.8; transform: scale(1);
    border-color: rgba(0,212,224,0.3);
  }
  .carousel-dot.active {
    opacity: 1; transform: scale(1.1);
    border-color: #00d4e0;
    box-shadow: 0 0 10px rgba(0,212,224,0.3);
  }
  .dot-avatar {
    width: 100%; height: 100%; object-fit: cover; display: block;
    border-radius: 50%;
  }
  .dot-color {
    width: 100%; height: 100%; border-radius: 50%;
  }

  .char-avatar-lg {
    width: 82px; height: 82px; min-width: 82px; min-height: 82px;
    border-radius: 50%; aspect-ratio: 1;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    overflow: hidden; position: relative; flex-shrink: 0;
    box-shadow: 0 0 20px rgba(139,92,246,0.25);
    transition: box-shadow 400ms;
  }
  .ctx-avatar-img { width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 50%; }
  .char-name-lg { font-size: var(--text-xl); font-weight: 700; color: #e8e0ff; }
  .char-tagline { font-size: var(--text-sm); color: #6b6b8a; text-align: center; line-height: 1.5; }

  .char-tags { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .tag {
    padding: 4px 10px; border-radius: 99px;
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.3px;
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

  .graph-link {
    display: flex; align-items: center; gap: 4px;
    font-size: 10px; font-weight: 600; color: var(--accent-primary);
    text-decoration: none; padding: 3px 8px; border-radius: 6px;
    border: 1px solid rgba(46,166,126,0.15); transition: all 150ms;
  }
  .graph-link:hover { background: rgba(46,166,126,0.08); border-color: rgba(46,166,126,0.3); }

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

  .memory-entry {
    display: flex; flex-direction: column; gap: 4px;
    padding: 10px 12px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: all 150ms;
  }
  .memory-entry:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.1); }
  .memory-text { font-size: var(--text-sm); color: #8b8ba7; line-height: 1.5; }
  .memory-footer {
    display: flex; align-items: center; gap: 8px; margin-top: 2px;
  }
  .memory-meta { font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono); }
  .memory-source { font-size: var(--text-xs); color: #5a5a7a; }
  .memory-delete {
    background: none; border: none; padding: 2px; cursor: pointer;
    opacity: 0; transition: opacity 150ms; flex-shrink: 0; display: flex; margin-left: auto;
  }
  .memory-entry:hover .memory-delete { opacity: 0.5; }
  .memory-delete:hover { opacity: 1 !important; }

  @media (max-width: 1024px) { .context-panel { display: none; } }

  /* ── Enable Memory Toggle ─────────────────────────────────────────── */
  .mem-toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    background: rgba(139,92,246,0.04);
    border: 1px solid rgba(139,92,246,0.08);
    border-radius: 8px;
    margin-bottom: 8px;
    cursor: pointer;
    transition: background 180ms ease, border-color 180ms ease;
    text-align: left;
  }
  .mem-toggle-row:hover { background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.16); }
  .mem-toggle-row:disabled { opacity: 0.5; cursor: not-allowed; }
  .mem-toggle-row.enabled { background: rgba(139,92,246,0.07); border-color: rgba(139,92,246,0.2); }

  .mem-toggle-icon { font-size: 13px; line-height: 1; flex-shrink: 0; }

  .mem-toggle-label {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    color: #7c7c9a;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }
  .mem-toggle-row.enabled .mem-toggle-label { color: #c4a1ff; }

  /* Pill track */
  .mem-toggle-pill {
    position: relative;
    width: 28px;
    height: 16px;
    border-radius: 8px;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.1);
    transition: background 220ms ease, border-color 220ms ease, box-shadow 220ms ease;
    flex-shrink: 0;
  }
  .mem-toggle-pill.on {
    background: rgba(139,92,246,0.55);
    border-color: rgba(139,92,246,0.7);
    box-shadow: 0 0 8px rgba(139,92,246,0.35);
  }

  /* Knob */
  .mem-toggle-knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: rgba(255,255,255,0.35);
    transition: transform 220ms cubic-bezier(.34,1.56,.64,1), background 220ms ease;
  }
  .mem-toggle-pill.on .mem-toggle-knob {
    transform: translateX(12px);
    background: #fff;
  }
</style>
