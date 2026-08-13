<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import { success } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';
  import type { ModelEntry } from '$lib/services/ipc';
  import { priceNum, formatPrice, ctxLabel, ctxPercent as ctxPercentOf, modelSlug, adapterColor, dimLabel } from '$lib/utils/models';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── State ──────────────────────────────────────────────────
  let allModels = $state<ModelEntry[]>([]);
  let isLoading = $state(true);
  let providers = $state<Array<{ id: string; name: string }>>([]);

  // Filters
  let filterProvider = $state('all');
  let filterStatus = $state('all');
  let filterPricing = $state('all');
  let filterSearch = $state('');

  // Sorting
  let sortBy = $state<'name' | 'price-asc' | 'price-desc' | 'context-desc' | 'context-asc'>('name');

  // Toggling / expanded
  let togglingId = $state<string | null>(null);
  let expandedId = $state<string | null>(null);

  // Derived filtered + sorted list
  let filtered = $derived(() => {
    let list = allModels;
    if (filterProvider !== 'all') list = list.filter(m => m.provider_id === filterProvider);
    if (filterStatus === 'enabled') list = list.filter(m => m.enabled);
    else if (filterStatus === 'disabled') list = list.filter(m => !m.enabled);
    if (filterPricing === 'free') list = list.filter(m => m.is_free);
    else if (filterPricing === 'paid') list = list.filter(m => !m.is_free);
    if (filterSearch.trim()) {
      const q = filterSearch.toLowerCase();
      list = list.filter(m =>
        m.model_id.toLowerCase().includes(q) ||
        m.provider_name.toLowerCase().includes(q) ||
        (m.display_name && m.display_name.toLowerCase().includes(q)) ||
        (m.description && m.description.toLowerCase().includes(q))
      );
    }
    // Sort
    list = [...list];
    switch (sortBy) {
      case 'name': list.sort((a, b) => (a.display_name ?? a.model_id).localeCompare(b.display_name ?? b.model_id)); break;
      case 'price-asc': list.sort((a, b) => priceNum(a.pricing_prompt) - priceNum(b.pricing_prompt)); break;
      case 'price-desc': list.sort((a, b) => priceNum(b.pricing_prompt) - priceNum(a.pricing_prompt)); break;
      case 'context-desc': list.sort((a, b) => (b.context_length ?? 0) - (a.context_length ?? 0)); break;
      case 'context-asc': list.sort((a, b) => (a.context_length ?? 0) - (b.context_length ?? 0)); break;
    }
    return list;
  });

  let enabledCount = $derived(allModels.filter(m => m.enabled).length);
  let freeCount = $derived(allModels.filter(m => m.is_free).length);
  let maxCtx = $derived(Math.max(...allModels.map(m => m.context_length ?? 0), 1));

  onMount(async () => { await loadAll(); });

  async function loadAll() {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      if (isTauri) {
        const [models, pList] = await Promise.all([
          ipc.listEmbeddingModels(),
          ipc.listProviders(),
        ]);
        allModels = models;
        providers = pList.map(p => ({ id: p.id, name: p.name }));
      }
    } catch (err) { handleIpcError('load models', err); }
    isLoading = false;
  }

  async function toggleModel(m: ModelEntry) {
    const key = `${m.provider_id}::${m.model_id}`;
    togglingId = key;
    try {
      const ipc = await import('$lib/services/ipc');
      const newState = !m.enabled;
      await ipc.toggleModelEnabled(m.provider_id, m.model_id, m.model_type, newState);
      allModels = allModels.map(x =>
        x.provider_id === m.provider_id && x.model_id === m.model_id
          ? { ...x, enabled: newState } : x
      );
      success(newState ? `Enabled ${m.display_name ?? m.model_id}` : `Disabled ${m.display_name ?? m.model_id}`);
    } catch (err) { handleIpcError('toggle model', err); }
    togglingId = null;
  }

  function ctxPercent(n: number | null): number {
    return ctxPercentOf(n, maxCtx);
  }
</script>

<svelte:head><title>Embedding Models — Janus</title></svelte:head>

<div class="page">
  <!-- Header -->
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title"><SplitHeading text="Embedding Models" /></h1>
      <div class="hdr-stats">
        {#if isLoading}
          <span class="stat">Loading…</span>
        {:else}
          <span class="stat">{allModels.length} <span class="stat-label">available</span></span>
          <span class="stat-sep">·</span>
          <span class="stat stat-enabled">{enabledCount} <span class="stat-label">enabled</span></span>
          <span class="stat-sep">·</span>
          <span class="stat stat-free">{freeCount} <span class="stat-label">free</span></span>
        {/if}
      </div>
    </div>
    <button class="btn-refresh" onclick={loadAll} disabled={isLoading} aria-label="Refresh models">
      <Icon name="refresh-cw" size={13} color={isLoading ? '#4a4a6a' : '#8B5CF6'} />
      Refresh
    </button>
  </header>

  <!-- Sticky Filter Bar -->
  <div class="filter-bar">
    <!-- Row 1: Search + Provider + Sort -->
    <div class="filter-row">
      <div class="search-wrap">
        <svg class="search-icon" viewBox="0 0 20 20" fill="none">
          <circle cx="8.5" cy="8.5" r="5.5" stroke="currentColor" stroke-width="1.5"/>
          <path d="M12.5 12.5L17 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        </svg>
        <input class="search-input" bind:value={filterSearch} placeholder="Search models, providers…" />
        {#if filterSearch}<button class="search-clear" onclick={() => filterSearch = ''}>✕</button>{/if}
      </div>

      <select class="filter-select" bind:value={filterProvider} aria-label="Filter by provider">
        <option value="all">All Providers</option>
        {#each providers as p}<option value={p.id}>{p.name}</option>{/each}
      </select>

      <select class="filter-select" bind:value={sortBy} aria-label="Sort by">
        <option value="name">Sort: Name A→Z</option>
        <option value="price-asc">Sort: Price Low→High</option>
        <option value="price-desc">Sort: Price High→Low</option>
        <option value="context-desc">Sort: Context ↓</option>
        <option value="context-asc">Sort: Context ↑</option>
      </select>
    </div>

    <!-- Row 2: Chips -->
    <div class="filter-row">
      <div class="chip-group" role="group" aria-label="Filter by status">
        {#each [['all','All'],['enabled','Enabled'],['disabled','Disabled']] as [val, label]}
          <button class="chip" class:chip-active={filterStatus === val} onclick={() => filterStatus = val}>
            {label}
          </button>
        {/each}
      </div>

      <div class="chip-divider"></div>

      <div class="chip-group" role="group" aria-label="Filter by pricing">
        {#each [['all','All Pricing'],['free','🆓 Free'],['paid','💰 Paid']] as [val, label]}
          <button class="chip" class:chip-active={filterPricing === val} onclick={() => filterPricing = val}>
            {label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Model Table -->
  <div class="table-wrap">
    {#if isLoading}
      <div class="table">
        <div class="thead">
          <span class="th th-model">Model</span>
          <span class="th th-provider">Provider</span>
          <span class="th th-dims">Dimensions</span>
          <span class="th th-price">Input / Output</span>
          <span class="th th-ctx">Context</span>
          <span class="th th-action"></span>
        </div>
        {#each Array(10) as _, i}
          <div class="trow skeleton-row" style="animation-delay:{i*40}ms">
            <Skeleton variant="text" width="55%" height="12px" />
            <Skeleton variant="text" width="20%" height="11px" />
            <Skeleton variant="text" width="15%" height="11px" />
            <Skeleton variant="text" width="15%" height="11px" />
          </div>
        {/each}
      </div>
    {:else if filtered().length === 0}
      <div class="empty-state">
        {#if allModels.length === 0}
          <div class="empty-icon">
            <Icon name="cpu" size={48} color="#3a3a5a" />
          </div>
          <span class="empty-title">No embedding models found</span>
          <span class="empty-sub">Add a provider that supports embedding models (OpenRouter, Ollama, etc.) in the Providers section first.</span>
        {:else}
          <div class="empty-icon">
            <Icon name="search" size={48} color="#3a3a5a" />
          </div>
          <span class="empty-title">No models match your filters</span>
          <button class="chip chip-active" onclick={() => { filterProvider='all'; filterStatus='all'; filterPricing='all'; filterSearch=''; }}>Clear Filters</button>
        {/if}
      </div>
    {:else}
      <div class="table">
        <div class="thead">
          <span class="th th-model">Model</span>
          <span class="th th-provider">Provider</span>
          <span class="th th-dims">Dimensions</span>
          <span class="th th-price">Input / Output <span class="th-unit">(per 1M tokens)</span></span>
          <span class="th th-ctx">Context</span>
          <span class="th th-action"></span>
        </div>
        {#each filtered() as m, i (`${m.provider_id}::${m.model_id}`)}
          {@const rowKey = `${m.provider_id}::${m.model_id}`}
          {@const isToggling = togglingId === rowKey}
          {@const isExpanded = expandedId === rowKey}
          <div
            class="trow"
            class:trow-enabled={m.enabled}
            class:trow-free={m.is_free}
            class:trow-expanded={isExpanded}
            class:trow-stale={m.is_stale}
            style="animation-delay:{Math.min(i*15,350)}ms"
            onclick={() => expandedId = isExpanded ? null : rowKey}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); expandedId = isExpanded ? null : rowKey; }}}
            role="button"
            tabindex="0"
          >
            {#if m.enabled}<span class="row-accent"></span>{/if}

            <!-- Model Column -->
            <span class="td td-model">
              <div class="model-info">
                <span class="model-name-row">
                  <span class="model-name">{m.display_name ?? modelSlug(m.model_id)}</span>
                  {#if m.is_free}<span class="free-badge">FREE</span>{/if}
                  {#if m.is_stale}<span class="stale-badge" title="No longer listed by the provider — automatically disabled.">NO LONGER LISTED</span>{/if}
                </span>
                {#if m.display_name}
                  <span class="model-slug">{m.model_id}</span>
                {/if}
              </div>
            </span>

            <!-- Provider Column -->
            <span class="td td-provider">
              <span class="provider-badge" style="color:{adapterColor(m.adapter)};background:color-mix(in srgb,{adapterColor(m.adapter)} 12%,transparent)">
                {m.provider_name}
              </span>
            </span>

            <!-- Dimensions Column -->
            <span class="td td-dims">
              {#if m.embedding_dimensions}
                <span class="dims-value">{dimLabel(m.embedding_dimensions)}</span>
                <span class="dims-unit">dims</span>
              {:else}
                <span class="dims-unknown">—</span>
              {/if}
            </span>

            <!-- Pricing Column -->
            <span class="td td-price">
              {#if m.is_free}
                <span class="price-free">Free</span>
              {:else if m.pricing_prompt}
                <span class="price-group">
                  <span class="price-in">{formatPrice(m.pricing_prompt)}</span>
                  <span class="price-sep">/</span>
                  <span class="price-out">{formatPrice(m.pricing_completion)}</span>
                </span>
              {:else}
                <span class="price-na">—</span>
              {/if}
            </span>

            <!-- Context Column -->
            <span class="td td-ctx">
              <div class="ctx-col">
                <span class="ctx-label">{ctxLabel(m.context_length)}</span>
                {#if m.context_length}
                  <div class="ctx-bar-bg">
                    <div class="ctx-bar-fill" style="width:{ctxPercent(m.context_length)}%"></div>
                  </div>
                {/if}
              </div>
            </span>

            <!-- Action Column -->
            <span class="td td-action" onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); }}} role="button" tabindex="0">
              <button
                class="toggle-btn"
                class:toggle-on={m.enabled}
                class:toggle-spinning={isToggling}
                onclick={(e) => { e.stopPropagation(); toggleModel(m); }}
                disabled={isToggling}
                aria-label={m.enabled ? 'Disable model' : 'Enable model'}
              >
                {#if isToggling}
                  <span class="toggle-spinner"></span>
                {:else}
                  <span class="toggle-track">
                    <span class="toggle-thumb"></span>
                  </span>
                {/if}
              </button>
            </span>
          </div>

          <!-- Expanded Detail Row -->
          {#if isExpanded}
            <div class="detail-row" style="animation-delay:0ms">
              <div class="detail-grid">
                {#if m.description}
                  <div class="detail-block detail-desc">
                    <span class="detail-label">Description</span>
                    <p class="detail-text">{m.description}</p>
                  </div>
                {/if}
                <div class="detail-block">
                  <span class="detail-label">Model ID</span>
                  <span class="detail-mono">{m.model_id}</span>
                </div>
                {#if m.context_length}
                  <div class="detail-block">
                    <span class="detail-label">Max Input</span>
                    <span class="detail-value">{ctxLabel(m.context_length)} tokens</span>
                  </div>
                {/if}
                {#if m.input_modalities.length > 0}
                  <div class="detail-block">
                    <span class="detail-label">Input</span>
                    <span class="detail-value">{m.input_modalities.join(', ')}</span>
                  </div>
                {/if}
                {#if m.output_modalities.length > 0}
                  <div class="detail-block">
                    <span class="detail-label">Output</span>
                    <span class="detail-value">{m.output_modalities.join(', ')}</span>
                  </div>
                {/if}
                {#if m.embedding_dimensions}
                  <div class="detail-block">
                    <span class="detail-label">Embedding Dimensions</span>
                    <span class="detail-value">
                      <span style="color:#c4a1ff;font-family:var(--font-mono);font-weight:700">{dimLabel(m.embedding_dimensions)}</span>
                      dimensions
                    </span>
                  </div>
                {/if}
                <div class="detail-block">
                  <span class="detail-label">Type</span>
                  <span class="detail-value">
                    <Icon name="cpu" size={12} color="#8B5CF6" />
                    Embedding
                  </span>
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
    font-size: 24px; font-weight: 600; letter-spacing: -0.5px; margin: 0;
  }
  .hdr-stats { display: flex; align-items: center; gap: 6px; }
  .stat { font-size: 13px; font-weight: 700; color: #c0c0d8; }
  .stat-label { font-weight: 400; color: #4a4a6a; }
  .stat-sep { color: #2a2a4a; }
  .stat-enabled { color: #10B981; }
  .stat-free { color: #06B6D4; }

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

  .filter-select {
    height: 34px; padding: 0 28px 0 10px; border-radius: 10px;
    background: rgba(12,12,28,0.9); border: 1px solid rgba(139,92,246,0.1);
    color: #c0c0d8; font-size: 12px; font-family: var(--font-body); outline: none;
    appearance: none; cursor: pointer;
    background-image: url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b6b8a' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e");
    background-position: right 6px center; background-repeat: no-repeat; background-size: 14px;
    transition: border-color 200ms;
  }
  .filter-select:focus { border-color: rgba(139,92,246,0.4); }

  .chip-group { display: flex; gap: 4px; }
  .chip-divider { width: 1px; height: 20px; background: rgba(139,92,246,0.1); margin: 0 4px; }
  .chip {
    padding: 5px 11px; border-radius: 99px; font-size: 11px; font-weight: 600;
    border: 1px solid rgba(139,92,246,0.1); background: transparent;
    color: #5a5a7a; cursor: pointer; font-family: var(--font-body);
    transition: all 160ms; white-space: nowrap;
  }
  .chip:hover { background: rgba(139,92,246,0.06); color: #9d7af5; }
  .chip-active { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.25); color: #c4a1ff; }

  /* ── Table ── */
  .table-wrap { flex: 1; overflow-y: auto; padding: 0 28px 28px; }
  .table-wrap::-webkit-scrollbar { width: 4px; }
  .table-wrap::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }
  .table { display: flex; flex-direction: column; gap: 0; }

  .thead {
    display: grid;
    grid-template-columns: 1.4fr 130px 100px 160px 120px 60px;
    padding: 10px 16px; position: sticky; top: 0; z-index: 2;
    background: rgba(8,8,20,0.95); backdrop-filter: blur(12px);
    border-bottom: 1px solid rgba(139,92,246,0.08);
  }
  .th {
    font-size: 10px; font-weight: 700; letter-spacing: 1.2px;
    text-transform: uppercase; color: #3a3a5a; font-family: var(--font-mono);
  }
  .th-unit { font-weight: 400; letter-spacing: 0.5px; font-size: 9px; color: #2a2a4a; }

  .trow {
    display: grid;
    grid-template-columns: 1.4fr 130px 100px 160px 120px 60px;
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
  .trow:hover .toggle-btn { opacity: 1; }
  .trow-enabled { background: rgba(139,92,246,0.025); }
  .trow-enabled:hover { background: rgba(139,92,246,0.06); }

  .trow-expanded { background: rgba(139,92,246,0.05); border-color: rgba(139,92,246,0.12); border-bottom-left-radius: 0; border-bottom-right-radius: 0; }

  .trow-stale { background: rgba(245,158,11,0.03); }
  .trow-stale:hover { background: rgba(245,158,11,0.07); }

  .skeleton-row { display: flex; align-items: center; gap: 16px; padding: 12px 16px; animation: rowIn 200ms ease both; }

  .row-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 2.5px; height: 55%; border-radius: 0 3px 3px 0;
    background: linear-gradient(180deg, #8B5CF6, #06B6D4);
    box-shadow: 0 0 10px rgba(139,92,246,0.5);
  }

  .td { display: flex; align-items: center; overflow: hidden; }

  /* Model column */
  .model-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .model-name-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .model-name {
    font-size: 12.5px; font-weight: 600; color: #dcdcf0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    line-height: 1.3;
  }
  .model-slug {
    font-size: 10px; font-family: var(--font-mono); color: #3a3a5a;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .free-badge {
    display: inline-flex; flex-shrink: 0;
    padding: 1px 5px; border-radius: 3px; font-size: 8px; font-weight: 800;
    letter-spacing: 0.8px; color: #06B6D4;
    background: rgba(6,182,212,0.1); border: 1px solid rgba(6,182,212,0.2);
  }

  .stale-badge {
    display: inline-flex; flex-shrink: 0;
    padding: 1px 5px; border-radius: 3px; font-size: 8px; font-weight: 800;
    letter-spacing: 0.6px; color: #F59E0B;
    background: rgba(245,158,11,0.1); border: 1px solid rgba(245,158,11,0.25);
  }

  /* Provider badge */
  .provider-badge {
    padding: 3px 9px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
    white-space: nowrap;
  }

  /* Pricing */
  .price-group { display: flex; align-items: center; gap: 3px; font-family: var(--font-mono); }
  .price-in { font-size: 11px; color: #10B981; font-weight: 600; }
  .price-sep { font-size: 10px; color: #2a2a4a; }
  .price-out { font-size: 11px; color: #F59E0B; font-weight: 600; }
  .price-free {
    font-size: 11px; font-weight: 800; letter-spacing: 0.5px;
    color: #06B6D4;
    text-shadow: 0 0 12px rgba(6,182,212,0.4);
  }
  .price-na { font-size: 11px; color: #2a2a4a; }

  /* Dimensions */
  .td-dims { display: flex; align-items: center; gap: 4px; }
  .dims-value {
    font-size: 12px; font-family: var(--font-mono); font-weight: 700;
    color: #c4a1ff;
    text-shadow: 0 0 12px rgba(139,92,246,0.3);
  }
  .dims-unit {
    font-size: 9px; font-weight: 500; letter-spacing: 0.5px;
    text-transform: uppercase; color: #4a4a6a;
  }
  .dims-unknown { font-size: 11px; color: #2a2a4a; }

  /* Context */
  .ctx-col { display: flex; flex-direction: column; gap: 4px; width: 100%; }
  .ctx-label { font-size: 11px; font-family: var(--font-mono); color: #6b6b8a; font-weight: 600; }
  .ctx-bar-bg {
    width: 100%; max-width: 80px; height: 3px; border-radius: 2px;
    background: rgba(139,92,246,0.08); overflow: hidden;
  }
  .ctx-bar-fill {
    height: 100%; border-radius: 2px;
    background: linear-gradient(90deg, #8B5CF6, #06B6D4);
    transition: width 400ms ease;
  }

  /* Toggle */
  .toggle-btn {
    display: flex; align-items: center; justify-content: center;
    width: 40px; height: 22px; padding: 0;
    border-radius: 11px; border: none; cursor: pointer;
    background: rgba(30,30,55,0.8); opacity: 0.5;
    transition: all 180ms;
  }
  .toggle-btn:hover { opacity: 1; }
  .trow:hover .toggle-btn { opacity: 0.8; }
  .toggle-on { background: rgba(16,185,129,0.2); opacity: 1; }
  .toggle-track {
    width: 100%; height: 100%; border-radius: 11px;
    position: relative; display: flex; align-items: center;
    padding: 0 2px;
  }
  .toggle-thumb {
    width: 16px; height: 16px; border-radius: 50%;
    background: #4a4a6a;
    transition: all 200ms cubic-bezier(0.4, 0, 0.2, 1);
    box-shadow: 0 1px 3px rgba(0,0,0,0.3);
  }
  .toggle-on .toggle-thumb {
    transform: translateX(18px);
    background: #10B981;
    box-shadow: 0 0 8px rgba(16,185,129,0.5);
  }
  .toggle-spinning { opacity: 0.4; pointer-events: none; }
  .toggle-spinner {
    width: 12px; height: 12px; border: 2px solid rgba(139,92,246,0.2);
    border-top-color: #8B5CF6; border-radius: 50%;
    animation: spin 600ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .toggle-btn:disabled { pointer-events: none; }

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
  .detail-desc { grid-column: 1 / -1; }
  .detail-label { font-size: 9px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; color: #3a3a5a; }
  .detail-text { font-size: 12px; color: #6b6b8a; line-height: 1.5; margin: 0; }
  .detail-mono { font-size: 11px; font-family: var(--font-mono); color: #8B5CF6; word-break: break-all; }
  .detail-value { font-size: 12px; color: #c0c0d8; display: flex; align-items: center; gap: 4px; }

  /* ── Empty State ── */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; gap: 14px;
    padding: 80px 20px; text-align: center;
  }
  .empty-icon { opacity: 0.4; }
  .empty-title { font-size: 15px; font-weight: 700; color: #6b6b8a; }
  .empty-sub { font-size: 13px; color: #4a4a6a; max-width: 300px; }
</style>
