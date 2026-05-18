<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { ModelEntry } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── State ──────────────────────────────────────────────────
  let allModels = $state<ModelEntry[]>([]);
  let isLoading = $state(true);
  let providers = $state<Array<{ id: string; name: string }>>([]);

  // Filters
  let filterProvider = $state('all');
  let filterType = $state('all');
  let filterStatus = $state('all');
  let filterSearch = $state('');

  // Toggling state
  let togglingId = $state<string | null>(null);

  // Derived filtered list
  let filtered = $derived(() => {
    let list = allModels;
    if (filterProvider !== 'all') list = list.filter(m => m.provider_id === filterProvider);
    if (filterType !== 'all') list = list.filter(m => m.model_type === filterType);
    if (filterStatus === 'enabled') list = list.filter(m => m.enabled);
    else if (filterStatus === 'disabled') list = list.filter(m => !m.enabled);
    if (filterSearch.trim()) {
      const q = filterSearch.toLowerCase();
      list = list.filter(m => m.model_id.toLowerCase().includes(q) || m.provider_name.toLowerCase().includes(q));
    }
    return list;
  });

  let enabledCount = $derived(allModels.filter(m => m.enabled).length);

  onMount(async () => {
    await loadAll();
  });

  async function loadAll() {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      if (isTauri) {
        const [models, pList] = await Promise.all([
          ipc.listAllModels(),
          ipc.listProviders(),
        ]);
        allModels = models;
        providers = pList.map(p => ({ id: p.id, name: p.name }));
      }
    } catch { toastError('Failed to load models'); }
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
      success(newState ? `Enabled ${m.model_id}` : `Disabled ${m.model_id}`);
    } catch { toastError('Failed to update model'); }
    togglingId = null;
  }

  function ctxLabel(n: number | null) {
    if (!n) return '—';
    return n >= 1000 ? `${(n / 1000).toFixed(0)}K` : String(n);
  }

  function adapterBadgeColor(a: string) {
    const map: Record<string, string> = {
      open_router: '#8B5CF6', ollama: '#10B981',
      open_ai_compatible: '#3B82F6', openai_compatible: '#3B82F6',
      silicon_flow: '#F59E0B',
    };
    return map[a] ?? '#6b6b8a';
  }

  function typeIcon(t: string) {
    return t === 'llm' ? 'message-circle' : t === 'image' ? 'image' : 'video';
  }
  function typeColor(t: string) {
    return t === 'llm' ? '#8B5CF6' : t === 'image' ? '#bf40ff' : '#00f2ff';
  }
</script>

<svelte:head><title>Models — Mythic</title></svelte:head>

<div class="page">
  <!-- Header -->
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title">Models</h1>
      <span class="hdr-sub">
        {#if isLoading}Loading…{:else}{allModels.length} available · <span class="enabled-count">{enabledCount} enabled</span>{/if}
      </span>
    </div>
    <button class="btn-refresh" onclick={loadAll} disabled={isLoading} aria-label="Refresh models">
      <Icon name="refresh-cw" size={13} color={isLoading ? '#4a4a6a' : '#8B5CF6'} />
      Refresh
    </button>
  </header>

  <!-- Sticky Filter Bar -->
  <div class="filter-bar">
    <!-- Search -->
    <div class="search-wrap">
      <svg class="search-icon" viewBox="0 0 20 20" fill="none">
        <circle cx="8.5" cy="8.5" r="5.5" stroke="currentColor" stroke-width="1.5"/>
        <path d="M12.5 12.5L17 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
      <input class="search-input" bind:value={filterSearch} placeholder="Search models…" />
      {#if filterSearch}<button class="search-clear" onclick={() => filterSearch = ''}>✕</button>{/if}
    </div>

    <!-- Provider filter -->
    <select class="filter-select" bind:value={filterProvider} aria-label="Filter by provider">
      <option value="all">All Providers</option>
      {#each providers as p}<option value={p.id}>{p.name}</option>{/each}
    </select>

    <!-- Type chips -->
    <div class="chip-group" role="group" aria-label="Filter by type">
      {#each ['all','llm','image','video'] as t}
        <button class="chip" class:chip-active={filterType === t} onclick={() => filterType = t}>
          {t === 'all' ? 'All Types' : t === 'llm' ? '💬 Chat' : t === 'image' ? '🖼 Image' : '🎬 Video'}
        </button>
      {/each}
    </div>

    <!-- Status chips -->
    <div class="chip-group" role="group" aria-label="Filter by status">
      {#each [['all','All'],['enabled','Enabled'],['disabled','Disabled']] as [val, label]}
        <button class="chip" class:chip-active={filterStatus === val} onclick={() => filterStatus = val}>
          {label}
        </button>
      {/each}
    </div>
  </div>

  <!-- Model Table -->
  <div class="table-wrap">
    {#if isLoading}
      <div class="table">
        <div class="thead">
          <span class="th th-model">Model</span>
          <span class="th th-provider">Provider</span>
          <span class="th th-type">Type</span>
          <span class="th th-ctx">Context</span>
          <span class="th th-status">Status</span>
          <span class="th th-action"></span>
        </div>
        {#each Array(8) as _, i}
          <div class="trow skeleton-row" style="animation-delay:{i*40}ms">
            <Skeleton variant="text" width="55%" height="12px" />
            <Skeleton variant="text" width="30%" height="11px" />
            <Skeleton variant="text" width="15%" height="11px" />
            <Skeleton variant="text" width="10%" height="11px" />
            <Skeleton variant="text" width="20%" height="11px" />
          </div>
        {/each}
      </div>
    {:else if filtered().length === 0}
      <div class="empty-state">
        {#if allModels.length === 0}
          <div class="empty-icon">🤖</div>
          <span class="empty-title">No models found</span>
          <span class="empty-sub">Add and test a provider in the Providers section first.</span>
        {:else}
          <div class="empty-icon">🔍</div>
          <span class="empty-title">No models match your filters</span>
          <button class="chip chip-active" onclick={() => { filterProvider='all'; filterType='all'; filterStatus='all'; filterSearch=''; }}>Clear Filters</button>
        {/if}
      </div>
    {:else}
      <div class="table">
        <div class="thead">
          <span class="th th-model">Model</span>
          <span class="th th-provider">Provider</span>
          <span class="th th-type">Type</span>
          <span class="th th-ctx">Context</span>
          <span class="th th-status">Status</span>
          <span class="th th-action"></span>
        </div>
        {#each filtered() as m, i (`${m.provider_id}::${m.model_id}`)}
          {@const rowKey = `${m.provider_id}::${m.model_id}`}
          {@const isToggling = togglingId === rowKey}
          <div class="trow" class:trow-enabled={m.enabled} style="animation-delay:{Math.min(i*18,400)}ms">
            {#if m.enabled}<span class="row-accent"></span>{/if}
            <span class="td td-model">
              <span class="model-id">{m.model_id.includes('/') ? m.model_id.split('/').slice(1).join('/') : m.model_id}</span>
            </span>
            <span class="td td-provider">
              <span class="provider-badge" style="color:{adapterBadgeColor(m.adapter)};background:color-mix(in srgb,{adapterBadgeColor(m.adapter)} 12%,transparent)">
                {m.provider_name}
              </span>
            </span>
            <span class="td td-type">
              <span class="type-chip" style="color:{typeColor(m.model_type)}">
                <Icon name={typeIcon(m.model_type)} size={11} color={typeColor(m.model_type)} />
                {m.model_type}
              </span>
            </span>
            <span class="td td-ctx">{ctxLabel(m.context_length)}</span>
            <span class="td td-status">
              {#if m.enabled}
                <span class="status-pill pill-on">● Enabled</span>
              {:else}
                <span class="status-pill pill-off">○ Disabled</span>
              {/if}
            </span>
            <span class="td td-action">
              <button class="toggle-btn" class:toggle-on={m.enabled} class:toggle-spinning={isToggling}
                onclick={() => toggleModel(m)} disabled={isToggling}
                aria-label={m.enabled ? 'Disable model' : 'Enable model'}>
                {#if isToggling}…{:else if m.enabled}Disable{:else}Enable{/if}
              </button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0b0b1e, #080814 60%, #06060f);
  }

  /* Header */
  .hdr {
    display: flex; align-items: center; justify-content: space-between;
    padding: 22px 28px 18px; flex-shrink: 0; position: relative;
  }
  .hdr::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.2), transparent);
  }
  .hdr-left { display: flex; flex-direction: column; gap: 3px; }
  .hdr-title {
    font-size: 22px; font-weight: 800; letter-spacing: -0.5px; margin: 0;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
  }
  .hdr-sub { font-size: 12px; color: #4a4a6a; }
  .enabled-count { color: #10B981; font-weight: 600; }

  .btn-refresh {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 14px; border-radius: 9px;
    border: 1px solid rgba(139,92,246,0.15); background: rgba(139,92,246,0.06);
    color: #8B5CF6; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms;
  }
  .btn-refresh:hover { background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.25); }
  .btn-refresh:disabled { opacity: 0.4; pointer-events: none; }

  /* Filter bar */
  .filter-bar {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 28px; flex-shrink: 0;
    border-bottom: 1px solid rgba(139,92,246,0.06);
    flex-wrap: wrap;
  }

  .search-wrap {
    position: relative; display: flex; align-items: center; flex: 0 0 220px;
  }
  .search-icon {
    position: absolute; left: 10px; width: 14px; height: 14px;
    color: #4a4a6a; pointer-events: none;
  }
  .search-input {
    width: 100%; height: 34px; padding: 0 30px 0 32px; border-radius: 9px;
    background: rgba(12,12,28,0.8); border: 1px solid rgba(139,92,246,0.1);
    color: #e0e0f0; font-size: 12px; font-family: var(--font-mono); outline: none;
    transition: border-color 180ms;
  }
  .search-input:focus { border-color: rgba(139,92,246,0.35); }
  .search-input::placeholder { color: #3a3a5a; }
  .search-clear {
    position: absolute; right: 8px; background: none; border: none;
    color: #4a4a6a; cursor: pointer; font-size: 11px; padding: 2px 4px;
  }

  .filter-select {
    height: 34px; padding: 0 28px 0 10px; border-radius: 9px;
    background: rgba(12,12,28,0.8); border: 1px solid rgba(139,92,246,0.1);
    color: #c0c0d8; font-size: 12px; font-family: var(--font-body); outline: none;
    appearance: none; cursor: pointer;
    background-image: url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b6b8a' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e");
    background-position: right 6px center; background-repeat: no-repeat; background-size: 14px;
    transition: border-color 180ms;
  }
  .filter-select:focus { border-color: rgba(139,92,246,0.35); }

  .chip-group { display: flex; gap: 4px; }
  .chip {
    padding: 5px 11px; border-radius: 99px; font-size: 11px; font-weight: 600;
    border: 1px solid rgba(139,92,246,0.1); background: transparent;
    color: #5a5a7a; cursor: pointer; font-family: var(--font-body);
    transition: all 140ms;
  }
  .chip:hover { background: rgba(139,92,246,0.06); color: #9d7af5; }
  .chip-active { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.25); color: #c4a1ff; }

  /* Table */
  .table-wrap { flex: 1; overflow-y: auto; padding: 0 28px 28px; }
  .table-wrap::-webkit-scrollbar { width: 4px; }
  .table-wrap::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  .table { display: flex; flex-direction: column; gap: 0; }

  .thead {
    display: grid;
    grid-template-columns: 1fr 140px 90px 70px 110px 90px;
    padding: 8px 14px; position: sticky; top: 0; z-index: 1;
    background: rgba(8,8,20,0.9); backdrop-filter: blur(8px);
    border-bottom: 1px solid rgba(139,92,246,0.08);
  }
  .th {
    font-size: 10px; font-weight: 700; letter-spacing: 1.2px;
    text-transform: uppercase; color: #3a3a5a; font-family: var(--font-mono);
  }

  .trow {
    display: grid;
    grid-template-columns: 1fr 140px 90px 70px 110px 90px;
    align-items: center; padding: 9px 14px;
    border-radius: 9px; position: relative;
    border: 1px solid transparent;
    transition: background 140ms, border-color 140ms;
    animation: rowIn 200ms ease both;
  }
  @keyframes rowIn { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: translateX(0); } }
  .trow:hover { background: rgba(139,92,246,0.05); border-color: rgba(139,92,246,0.08); }
  .trow:hover .toggle-btn { opacity: 1; }
  .trow-enabled { background: rgba(139,92,246,0.03); }
  .trow-enabled:hover { background: rgba(139,92,246,0.07); }

  .skeleton-row { display: flex; align-items: center; gap: 16px; padding: 11px 14px; animation: rowIn 200ms ease both; }

  .row-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 2px; height: 60%; border-radius: 0 2px 2px 0;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 8px rgba(139,92,246,0.6);
  }

  .td { display: flex; align-items: center; overflow: hidden; }

  .model-id {
    font-size: 12px; font-family: var(--font-mono); color: #c8c8e0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .provider-badge {
    padding: 2px 8px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
    white-space: nowrap;
  }

  .type-chip {
    display: flex; align-items: center; gap: 4px;
    font-size: 11px; font-weight: 600; text-transform: capitalize;
  }

  .td-ctx { font-size: 11px; font-family: var(--font-mono); color: #4a4a6a; }

  .status-pill { font-size: 11px; font-weight: 600; }
  .pill-on { color: #10B981; }
  .pill-off { color: #4a4a6a; }

  .toggle-btn {
    padding: 4px 11px; border-radius: 7px;
    border: 1px solid rgba(139,92,246,0.15); background: transparent;
    color: #8B5CF6; font-size: 11px; font-weight: 700; font-family: var(--font-body);
    cursor: pointer; opacity: 0; transition: all 140ms;
  }
  .toggle-btn:hover { background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.3); }
  .toggle-on { color: #F43F5E; border-color: rgba(244,63,94,0.2); }
  .toggle-on:hover { background: rgba(244,63,94,0.08); }
  .toggle-spinning { opacity: 1; color: #5a5a7a; }
  .toggle-btn:disabled { pointer-events: none; }

  /* Empty */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    padding: 80px 20px; text-align: center;
  }
  .empty-icon { font-size: 40px; opacity: 0.35; }
  .empty-title { font-size: 15px; font-weight: 700; color: #6b6b8a; }
  .empty-sub { font-size: 13px; color: #4a4a6a; max-width: 300px; }
</style>
