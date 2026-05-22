<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── State ──────────────────────────────────────────────────
  interface EmbedRow {
    id: string;
    name: string;
    adapter: string;
    config: Record<string, string>;
    isEditing?: boolean;
    isSaving?: boolean;
    isTesting?: boolean;
    testLatency?: number | null;
    testOk?: boolean | null;
    editModel?: string;
  }

  let allRows = $state<EmbedRow[]>([]);
  let isLoading = $state(true);
  let filterSearch = $state('');
  let expandedId = $state<string | null>(null);

  const embeddingAdapters = new Set([
    'open_router', 'open_ai_compatible', 'openai_compatible', 'ollama',
    'lm_studio', 'gemini', 'cohere', 'together',
  ]);

  // Derived filtered list
  let filtered = $derived(() => {
    let list = allRows;
    if (filterSearch.trim()) {
      const q = filterSearch.toLowerCase();
      list = list.filter(r =>
        r.name.toLowerCase().includes(q) ||
        r.adapter.toLowerCase().includes(q) ||
        (r.config.embedding_model || '').toLowerCase().includes(q) ||
        adapterLabel(r.adapter).toLowerCase().includes(q)
      );
    }
    return list;
  });

  let configuredCount = $derived(allRows.filter(r => !!r.config.embedding_model).length);

  // ── Adapter metadata ──────────────────────────────────────
  const adapterMeta: Record<string, { label: string; color: string }> = {
    open_router:        { label: 'OpenRouter',        color: '#8B5CF6' },
    open_ai_compatible: { label: 'OpenAI Compatible', color: '#3B82F6' },
    openai_compatible:  { label: 'OpenAI Compatible', color: '#3B82F6' },
    ollama:             { label: 'Ollama',            color: '#10B981' },
    lm_studio:          { label: 'LM Studio',        color: '#06B6D4' },
    gemini:             { label: 'Gemini',            color: '#4285F4' },
    cohere:             { label: 'Cohere',            color: '#D97706' },
    together:           { label: 'Together',          color: '#6366F1' },
  };
  function adapterLabel(a: string) { return adapterMeta[a]?.label ?? a; }
  function adapterColor(a: string) { return adapterMeta[a]?.color ?? '#6b6b8a'; }

  const suggestedModels: Record<string, { model: string; dims: number }> = {
    open_router:        { model: 'openai/text-embedding-3-small', dims: 1536 },
    open_ai_compatible: { model: 'text-embedding-3-small',        dims: 1536 },
    openai_compatible:  { model: 'text-embedding-3-small',        dims: 1536 },
    ollama:             { model: 'nomic-embed-text',              dims: 768 },
    lm_studio:          { model: 'nomic-embed-text',              dims: 768 },
    gemini:             { model: 'text-embedding-004',            dims: 768 },
    cohere:             { model: 'embed-english-v3.0',            dims: 1024 },
    together:           { model: 'togethercomputer/m2-bert-80M-8k-retrieval', dims: 768 },
  };
  function suggested(a: string) { return suggestedModels[a] ?? { model: 'text-embedding-3-small', dims: 1536 }; }

  // ── Lifecycle ─────────────────────────────────────────────
  onMount(async () => { await loadAll(); });

  async function loadAll() {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      if (isTauri) {
        const pList = await ipc.listProviders();
        allRows = pList
          .filter(p => embeddingAdapters.has(p.adapter))
          .map(p => ({
            id: p.id, name: p.name, adapter: p.adapter,
            config: p.config as Record<string, string>,
          }));
      }
    } catch (err) { handleIpcError('load providers', err); }
    isLoading = false;
  }

  // ── Actions ───────────────────────────────────────────────
  function startEdit(row: EmbedRow) {
    row.isEditing = true;
    row.editModel = row.config.embedding_model || suggested(row.adapter).model;
    allRows = [...allRows];
  }

  function cancelEdit(row: EmbedRow) {
    row.isEditing = false;
    row.editModel = undefined;
    allRows = [...allRows];
  }

  async function saveModel(row: EmbedRow) {
    if (!isTauri || !row.editModel?.trim()) return;
    row.isSaving = true;
    allRows = [...allRows];
    try {
      const ipc = await import('$lib/services/ipc');
      const existing = await ipc.getProvider(row.id);
      const config = { ...(existing.config as Record<string, unknown>), embedding_model: row.editModel };
      await ipc.updateProvider(row.id, undefined, config);
      row.config = { ...row.config, embedding_model: row.editModel! };
      row.isEditing = false;
      row.editModel = undefined;
      success(`Saved ${row.name} embedding model`);
    } catch (err) { handleIpcError('save model', err); }
    row.isSaving = false;
    allRows = [...allRows];
  }

  async function testConnection(row: EmbedRow) {
    if (!isTauri || row.isTesting) return;
    const model = row.config.embedding_model || suggested(row.adapter).model;
    if (!model) return;
    row.isTesting = true;
    row.testOk = null;
    row.testLatency = null;
    allRows = [...allRows];
    const t0 = Date.now();
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.getEmbeddingIndexStatus(null, model);
      row.testOk = true;
      row.testLatency = Date.now() - t0;
      success(`${row.name}: connected (${row.testLatency}ms)`);
    } catch {
      row.testOk = false;
      row.testLatency = Date.now() - t0;
      toastError(`${row.name}: connection failed`);
    }
    row.isTesting = false;
    allRows = [...allRows];
  }

  function useSuggested(row: EmbedRow) {
    row.editModel = suggested(row.adapter).model;
    allRows = [...allRows];
  }
</script>

<svelte:head><title>Embedding Models — Mythic</title></svelte:head>

<div class="page">
  <!-- Header -->
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title">Embedding Models</h1>
      <div class="hdr-stats">
        {#if isLoading}
          <span class="stat">Loading…</span>
        {:else}
          <span class="stat">{allRows.length} <span class="stat-label">available</span></span>
          <span class="stat-sep">·</span>
          <span class="stat stat-enabled">{configuredCount} <span class="stat-label">configured</span></span>
        {/if}
      </div>
    </div>
    <button class="btn-refresh" onclick={loadAll} disabled={isLoading} aria-label="Refresh">
      <Icon name="refresh-cw" size={13} color={isLoading ? '#4a4a6a' : '#8B5CF6'} />
      Refresh
    </button>
  </header>

  <!-- Filter Bar -->
  <div class="filter-bar">
    <div class="filter-row">
      <div class="search-wrap">
        <svg class="search-icon" viewBox="0 0 20 20" fill="none">
          <circle cx="8.5" cy="8.5" r="5.5" stroke="currentColor" stroke-width="1.5"/>
          <path d="M12.5 12.5L17 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        </svg>
        <input class="search-input" bind:value={filterSearch} placeholder="Search providers, models…" />
        {#if filterSearch}<button class="search-clear" onclick={() => filterSearch = ''}>✕</button>{/if}
      </div>
    </div>
  </div>

  <!-- Table -->
  <div class="table-wrap">
    {#if isLoading}
      <div class="table">
        <div class="thead">
          <span class="th th-provider">Provider</span>
          <span class="th th-model">Embedding Model</span>
          <span class="th th-dims">Dimensions</span>
          <span class="th th-status">Status</span>
          <span class="th th-action"></span>
        </div>
        {#each Array(4) as _, i}
          <div class="trow skeleton-row" style="animation-delay:{i*40}ms">
            <Skeleton variant="text" width="30%" height="12px" />
            <Skeleton variant="text" width="50%" height="12px" />
            <Skeleton variant="text" width="15%" height="11px" />
            <Skeleton variant="text" width="15%" height="11px" />
          </div>
        {/each}
      </div>
    {:else if filtered().length === 0}
      <div class="empty-state">
        {#if allRows.length === 0}
          <div class="empty-icon">
            <Icon name="cpu" size={48} color="#3a3a5a" />
          </div>
          <span class="empty-title">No embedding providers found</span>
          <span class="empty-sub">Add a provider that supports embeddings (OpenRouter, Ollama, Gemini, etc.) in the <a href="/providers" class="empty-link">Providers</a> section.</span>
        {:else}
          <div class="empty-icon">
            <Icon name="search" size={48} color="#3a3a5a" />
          </div>
          <span class="empty-title">No models match your search</span>
          <button class="chip chip-active" onclick={() => filterSearch = ''}>Clear Search</button>
        {/if}
      </div>
    {:else}
      <div class="table">
        <div class="thead">
          <span class="th th-provider">Provider</span>
          <span class="th th-model">Embedding Model</span>
          <span class="th th-dims">Dimensions</span>
          <span class="th th-status">Status</span>
          <span class="th th-action"></span>
        </div>
        {#each filtered() as row, i (row.id)}
          {@const isExpanded = expandedId === row.id}
          {@const hasModel = !!row.config.embedding_model}
          <div
            class="trow"
            class:trow-enabled={hasModel}
            class:trow-expanded={isExpanded}
            style="animation-delay:{Math.min(i*15,350)}ms"
            onclick={() => expandedId = isExpanded ? null : row.id}
            role="button"
            tabindex="0"
          >
            {#if hasModel}<span class="row-accent"></span>{/if}

            <!-- Provider Column -->
            <span class="td td-provider">
              <span class="provider-badge" style="color:{adapterColor(row.adapter)};background:color-mix(in srgb,{adapterColor(row.adapter)} 12%,transparent)">
                {row.name}
              </span>
            </span>

            <!-- Model Column -->
            <span class="td td-model">
              {#if row.isEditing}
                <div class="edit-wrap" onclick={(e) => e.stopPropagation()}>
                  <input
                    class="edit-input"
                    bind:value={row.editModel}
                    placeholder={suggested(row.adapter).model}
                    spellcheck="false"
                    onkeydown={(e) => { if (e.key === 'Enter') saveModel(row); if (e.key === 'Escape') cancelEdit(row); }}
                  />
                  <button class="edit-btn save" onclick={() => saveModel(row)} disabled={row.isSaving}>
                    {#if row.isSaving}<span class="toggle-spinner"></span>{:else}<Icon name="check" size={12} color="#10B981" />{/if}
                  </button>
                  <button class="edit-btn cancel" onclick={() => cancelEdit(row)}>
                    <Icon name="x" size={12} color="#F43F5E" />
                  </button>
                </div>
              {:else}
                <div class="model-info">
                  <span class="model-name">{row.config.embedding_model || '—'}</span>
                  {#if !hasModel}
                    <span class="model-slug">not configured</span>
                  {/if}
                </div>
              {/if}
            </span>

            <!-- Dimensions Column -->
            <span class="td td-dims">
              <span class="dims-label">{suggested(row.adapter).dims}d</span>
            </span>

            <!-- Status Column -->
            <span class="td td-status">
              {#if hasModel}
                <span class="status-badge status-ok">
                  <span class="status-dot"></span>
                  Configured
                </span>
              {:else}
                <span class="status-badge status-none">Not set</span>
              {/if}
              {#if row.testOk === true}
                <span class="test-pill test-ok">
                  <Icon name="check-circle" size={11} color="#10B981" />
                  {row.testLatency}ms
                </span>
              {:else if row.testOk === false}
                <span class="test-pill test-fail">
                  <Icon name="x-circle" size={11} color="#F43F5E" />
                  Failed
                </span>
              {/if}
            </span>

            <!-- Action Column -->
            <span class="td td-action" onclick={(e) => e.stopPropagation()}>
              <div class="action-btns">
                <button class="action-btn" title="Edit model" onclick={() => startEdit(row)} aria-label="Edit model">
                  <Icon name="edit-2" size={13} color="#6b6b8a" />
                </button>
                <button
                  class="action-btn"
                  title="Test connection"
                  onclick={() => testConnection(row)}
                  disabled={row.isTesting}
                  aria-label="Test connection"
                >
                  {#if row.isTesting}
                    <span class="toggle-spinner"></span>
                  {:else}
                    <Icon name="activity" size={13} color="#6b6b8a" />
                  {/if}
                </button>
              </div>
            </span>
          </div>

          <!-- Expanded Detail Row -->
          {#if isExpanded}
            <div class="detail-row" style="animation-delay:0ms">
              <div class="detail-grid">
                <div class="detail-block">
                  <span class="detail-label">Provider ID</span>
                  <span class="detail-mono">{row.id}</span>
                </div>
                <div class="detail-block">
                  <span class="detail-label">Adapter</span>
                  <span class="detail-value">{adapterLabel(row.adapter)}</span>
                </div>
                <div class="detail-block">
                  <span class="detail-label">Model ID</span>
                  <span class="detail-mono">{row.config.embedding_model || 'not set'}</span>
                </div>
                <div class="detail-block">
                  <span class="detail-label">Suggested Model</span>
                  <button class="suggest-btn" onclick={(e) => { e.stopPropagation(); startEdit(row); useSuggested(row); }}>
                    <code>{suggested(row.adapter).model}</code>
                    <span class="suggest-dims">{suggested(row.adapter).dims}d</span>
                    <Icon name="arrow-right" size={10} color="#8B5CF6" />
                  </button>
                </div>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0b0b1e 0%, #080814 40%, #06060f 100%);
  }

  /* ── Header ── */
  .hdr {
    display: flex; align-items: flex-start; justify-content: space-between;
    padding: 24px 28px 20px; flex-shrink: 0; position: relative;
  }
  .hdr::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.2), transparent);
  }
  .hdr-left { display: flex; flex-direction: column; gap: 6px; }
  .hdr-title {
    font-size: 24px; font-weight: 800; letter-spacing: -0.5px; margin: 0;
    background: linear-gradient(135deg, #f0e8ff, #c4a1ff 50%, #8B5CF6);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
  }
  .hdr-stats { display: flex; align-items: center; gap: 6px; }
  .stat { font-size: 13px; font-weight: 700; color: #c0c0d8; }
  .stat-label { font-weight: 400; color: #4a4a6a; }
  .stat-sep { color: #2a2a4a; }
  .stat-enabled { color: #10B981; }

  .btn-refresh {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.15); background: rgba(139,92,246,0.06);
    color: #8B5CF6; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 180ms;
  }
  .btn-refresh:hover { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.3); transform: translateY(-1px); }
  .btn-refresh:disabled { opacity: 0.4; pointer-events: none; }

  /* ── Filter Bar ── */
  .filter-bar {
    display: flex; flex-direction: column; gap: 8px;
    padding: 14px 28px; flex-shrink: 0;
    border-bottom: 1px solid rgba(139,92,246,0.06);
    background: rgba(8,8,20,0.5); backdrop-filter: blur(12px);
  }
  .filter-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

  .search-wrap { position: relative; display: flex; align-items: center; flex: 1 1 240px; max-width: 320px; }
  .search-icon { position: absolute; left: 11px; width: 14px; height: 14px; color: #4a4a6a; pointer-events: none; }
  .search-input {
    width: 100%; height: 34px; padding: 0 30px 0 34px; border-radius: 10px;
    background: rgba(12,12,28,0.9); border: 1px solid rgba(139,92,246,0.1);
    color: #e0e0f0; font-size: 12px; font-family: var(--font-mono); outline: none;
    transition: border-color 200ms, box-shadow 200ms;
  }
  .search-input:focus { border-color: rgba(139,92,246,0.4); box-shadow: 0 0 0 3px rgba(139,92,246,0.08); }
  .search-input::placeholder { color: #3a3a5a; }
  .search-clear {
    position: absolute; right: 8px; background: none; border: none;
    color: #4a4a6a; cursor: pointer; font-size: 11px; padding: 2px 4px;
  }

  /* ── Table ── */
  .table-wrap { flex: 1; overflow-y: auto; padding: 0 28px 28px; }
  .table-wrap::-webkit-scrollbar { width: 4px; }
  .table-wrap::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }
  .table { display: flex; flex-direction: column; gap: 0; }

  .thead {
    display: grid;
    grid-template-columns: 160px 1fr 90px 160px 80px;
    padding: 10px 16px; position: sticky; top: 0; z-index: 2;
    background: rgba(8,8,20,0.95); backdrop-filter: blur(12px);
    border-bottom: 1px solid rgba(139,92,246,0.08);
  }
  .th {
    font-size: 10px; font-weight: 700; letter-spacing: 1.2px;
    text-transform: uppercase; color: #3a3a5a; font-family: var(--font-mono);
  }

  .trow {
    display: grid;
    grid-template-columns: 160px 1fr 90px 160px 80px;
    align-items: center; padding: 10px 16px;
    border-radius: 10px; position: relative;
    border: 1px solid transparent; cursor: pointer;
    transition: background 160ms, border-color 160ms, transform 120ms;
    animation: rowIn 220ms ease both;
  }
  @keyframes rowIn {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .trow:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.08); }
  .trow-enabled { background: rgba(139,92,246,0.025); }
  .trow-enabled:hover { background: rgba(139,92,246,0.06); }
  .trow-expanded { background: rgba(139,92,246,0.05); border-color: rgba(139,92,246,0.12); border-bottom-left-radius: 0; border-bottom-right-radius: 0; }

  .skeleton-row { display: flex; align-items: center; gap: 16px; padding: 12px 16px; animation: rowIn 200ms ease both; }

  .row-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 2.5px; height: 55%; border-radius: 0 3px 3px 0;
    background: linear-gradient(180deg, #8B5CF6, #06B6D4);
    box-shadow: 0 0 10px rgba(139,92,246,0.5);
  }

  .td { display: flex; align-items: center; overflow: hidden; }

  /* Provider badge */
  .provider-badge {
    padding: 3px 9px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
    white-space: nowrap;
  }

  /* Model column */
  .model-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .model-name {
    font-size: 12.5px; font-weight: 600; color: #dcdcf0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    line-height: 1.3; font-family: var(--font-mono);
  }
  .model-slug {
    font-size: 10px; font-family: var(--font-mono); color: #3a3a5a;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  /* Dims */
  .dims-label {
    font-size: 11px; font-family: var(--font-mono); color: #6b6b8a; font-weight: 600;
  }

  /* Status */
  .status-badge {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 3px 8px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
  }
  .status-ok {
    color: #10B981; background: rgba(16,185,129,0.1);
  }
  .status-none {
    color: #4a4a6a; background: rgba(255,255,255,0.03);
  }
  .status-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.5);
  }

  .test-pill {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 7px; border-radius: 99px;
    font-size: 10px; font-weight: 600; font-family: var(--font-mono);
    margin-left: 6px;
    animation: pop 200ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes pop { from { opacity: 0; transform: scale(0.9); } to { opacity: 1; transform: scale(1); } }
  .test-ok { color: #10B981; background: rgba(16,185,129,0.08); }
  .test-fail { color: #F43F5E; background: rgba(244,63,94,0.08); }

  /* Actions */
  .action-btns { display: flex; gap: 4px; }
  .action-btn {
    width: 30px; height: 30px; border-radius: 8px;
    border: 1px solid rgba(139,92,246,0.08); background: transparent;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 160ms; opacity: 0.5;
  }
  .action-btn:hover { background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.16); opacity: 1; }
  .action-btn:disabled { opacity: 0.25; pointer-events: none; }
  .trow:hover .action-btn { opacity: 0.7; }

  /* Inline edit */
  .edit-wrap {
    display: flex; align-items: center; gap: 4px; width: 100%;
  }
  .edit-input {
    flex: 1; height: 30px; padding: 0 10px; border-radius: 7px;
    background: rgba(12,12,28,0.9); border: 1px solid rgba(139,92,246,0.25);
    color: #e0e0f0; font-size: 11.5px; font-family: var(--font-mono);
    outline: none;
  }
  .edit-input:focus { border-color: rgba(139,92,246,0.5); box-shadow: 0 0 0 2px rgba(139,92,246,0.08); }
  .edit-btn {
    width: 28px; height: 28px; border-radius: 6px; border: none;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 150ms; flex-shrink: 0;
  }
  .edit-btn.save { background: rgba(16,185,129,0.1); }
  .edit-btn.save:hover { background: rgba(16,185,129,0.2); }
  .edit-btn.cancel { background: rgba(244,63,94,0.08); }
  .edit-btn.cancel:hover { background: rgba(244,63,94,0.15); }
  .edit-btn:disabled { opacity: 0.4; pointer-events: none; }

  /* ── Detail Row ── */
  .detail-row {
    padding: 14px 16px 16px; margin-top: -1px;
    border: 1px solid rgba(139,92,246,0.12); border-top: none;
    border-radius: 0 0 10px 10px;
    background: rgba(139,92,246,0.03);
    animation: detailIn 200ms ease both;
  }
  @keyframes detailIn {
    from { opacity: 0; transform: translateY(-4px); max-height: 0; }
    to { opacity: 1; transform: translateY(0); max-height: 300px; }
  }
  .detail-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 12px 24px;
  }
  .detail-block { display: flex; flex-direction: column; gap: 3px; }
  .detail-label { font-size: 9px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; color: #3a3a5a; }
  .detail-mono { font-size: 11px; font-family: var(--font-mono); color: #8B5CF6; word-break: break-all; }
  .detail-value { font-size: 12px; color: #c0c0d8; display: flex; align-items: center; gap: 4px; }

  .suggest-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 10px; border-radius: 6px; width: fit-content;
    background: rgba(139,92,246,0.06); border: 1px solid rgba(139,92,246,0.12);
    cursor: pointer; transition: all 160ms; font-family: var(--font-body);
  }
  .suggest-btn:hover { background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.25); }
  .suggest-btn code {
    font-size: 11px; color: #c4a1ff; font-family: var(--font-mono);
    background: none; padding: 0;
  }
  .suggest-dims {
    font-size: 9px; font-family: var(--font-mono); color: #4a4a6a;
    padding: 1px 4px; border-radius: 3px; background: rgba(255,255,255,0.03);
  }

  /* Spinner */
  .toggle-spinner {
    width: 12px; height: 12px; border: 2px solid rgba(139,92,246,0.2);
    border-top-color: #8B5CF6; border-radius: 50%;
    animation: spin 600ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Chip (for clear filters) */
  .chip {
    padding: 5px 11px; border-radius: 99px; font-size: 11px; font-weight: 600;
    border: 1px solid rgba(139,92,246,0.1); background: transparent;
    color: #5a5a7a; cursor: pointer; font-family: var(--font-body);
    transition: all 160ms; white-space: nowrap;
  }
  .chip-active { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.25); color: #c4a1ff; }

  /* ── Empty State ── */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; gap: 14px;
    padding: 80px 20px; text-align: center;
  }
  .empty-icon { opacity: 0.4; }
  .empty-title { font-size: 15px; font-weight: 700; color: #6b6b8a; }
  .empty-sub { font-size: 13px; color: #4a4a6a; max-width: 340px; line-height: 1.5; }
  .empty-link { color: #8B5CF6; text-decoration: none; font-weight: 600; }
  .empty-link:hover { text-decoration: underline; }
</style>
