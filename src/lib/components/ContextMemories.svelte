<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { success, error as toastError, undoableDelete } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    characterId = null,
    conversationId = null,
  }: {
    characterId?: string | null;
    conversationId?: string | null;
  } = $props();

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

  function deleteMemoryEntry(memoryId: string) {
    if (!isTauri) return;
    const idx = memories.findIndex(m => m.id === memoryId);
    if (idx < 0) return;
    const [removed] = memories.splice(idx, 1);
    memories = memories;

    undoableDelete(
      'Memory removed',
      async () => {
        try {
          const ipc = await import('$lib/services/ipc');
          await ipc.deleteMemory(memoryId);
        } catch {
          toastError('Failed to delete memory');
        }
      },
      () => {
        memories.splice(idx, 0, removed);
        memories = memories;
      },
    );
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
</script>

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

  .graph-link {
    display: flex; align-items: center; gap: 4px;
    font-size: 10px; font-weight: 600; color: var(--accent-primary);
    text-decoration: none; padding: 3px 8px; border-radius: 6px;
    border: 1px solid rgba(46,166,126,0.15); transition: all 150ms;
  }
  .graph-link:hover { background: rgba(46,166,126,0.08); border-color: rgba(46,166,126,0.3); }

  .lore-form {
    display: flex; flex-direction: column; gap: 6px;
    padding: clamp(10px, 3cqi, 16px); border-radius: 12px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.1);
  }
  .lore-textarea {
    padding: 8px 10px; border-radius: 8px;
    background: rgba(9,9,26,0.6); border: 1px solid rgba(139,92,246,0.08);
    font-size: 11px; font-family: var(--font-body); color: #e0e0f0;
    outline: none; transition: border-color 150ms;
    resize: vertical;
  }
  .lore-textarea:focus { border-color: rgba(139,92,246,0.35); }

  .lore-save-btn {
    align-self: flex-end; padding: 6px 14px; border-radius: 8px;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); border: none;
    color: #fff; font-size: 11px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: opacity 150ms, box-shadow 150ms;
    box-shadow: 0 2px 10px rgba(139,92,246,0.2);
  }
  .lore-save-btn:hover:not(:disabled) { box-shadow: 0 4px 16px rgba(139,92,246,0.35); }
  .lore-save-btn:disabled { opacity: 0.35; cursor: default; }

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
    padding: clamp(9px, 2.6cqi, 14px) clamp(10px, 3cqi, 16px); border-radius: 10px;
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
