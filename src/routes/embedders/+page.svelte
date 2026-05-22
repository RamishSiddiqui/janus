<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  interface ProviderRow {
    id: string;
    name: string;
    adapter: string;
    config: Record<string, string>;
  }

  let providers = $state<ProviderRow[]>([]);
  let isLoading = $state(true);
  let selectedProviderId = $state<string | null>(null);
  let embeddingModel = $state('');
  let isTesting = $state(false);
  let isSaving = $state(false);
  let testResult = $state<{ ok: boolean; latency: number } | null>(null);

  let selectedProvider = $derived(providers.find(p => p.id === selectedProviderId) ?? null);

  const embeddingAdapters = new Set([
    'open_router', 'open_ai_compatible', 'openai_compatible', 'ollama',
    'lm_studio', 'gemini', 'cohere', 'together',
  ]);
  let embeddingProviders = $derived(providers.filter(p => embeddingAdapters.has(p.adapter)));

  const meta: Record<string, { label: string; color: string; icon: string }> = {
    open_router:        { label: 'OpenRouter',        color: '#8B5CF6', icon: 'globe' },
    open_ai_compatible: { label: 'OpenAI Compatible', color: '#3B82F6', icon: 'box' },
    openai_compatible:  { label: 'OpenAI Compatible', color: '#3B82F6', icon: 'box' },
    ollama:             { label: 'Ollama',            color: '#10B981', icon: 'terminal' },
    lm_studio:          { label: 'LM Studio',         color: '#06B6D4', icon: 'monitor' },
    gemini:             { label: 'Gemini',            color: '#4285F4', icon: 'sparkles' },
    cohere:             { label: 'Cohere',            color: '#D97706', icon: 'sun' },
    together:           { label: 'Together',          color: '#6366F1', icon: 'users' },
  };

  const suggestions: Record<string, { model: string; dims: string }> = {
    open_router:        { model: 'openai/text-embedding-3-small', dims: '1536' },
    open_ai_compatible: { model: 'text-embedding-3-small',        dims: '1536' },
    openai_compatible:  { model: 'text-embedding-3-small',        dims: '1536' },
    ollama:             { model: 'nomic-embed-text',              dims: '768' },
    lm_studio:          { model: 'nomic-embed-text',              dims: '768' },
    gemini:             { model: 'text-embedding-004',            dims: '768' },
    cohere:             { model: 'embed-english-v3.0',            dims: '1024' },
    together:           { model: 'togethercomputer/m2-bert-80M-8k-retrieval', dims: '768' },
  };

  function m(a: string) { return meta[a] ?? { label: a, color: '#6b6b8a', icon: 'cpu' }; }
  function s(a: string) { return suggestions[a] ?? { model: 'text-embedding-3-small', dims: '1536' }; }

  onMount(async () => { await loadProviders(); });

  async function loadProviders() {
    if (!isTauri) { isLoading = false; return; }
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const rows = await ipc.listProviders();
      providers = rows.map(p => ({
        id: p.id, name: p.name, adapter: p.adapter,
        config: p.config as Record<string, string>,
      }));
      const first = providers.find(p => embeddingAdapters.has(p.adapter));
      if (first) {
        selectedProviderId = first.id;
        embeddingModel = first.config.embedding_model || s(first.adapter).model;
      }
    } catch (err) { handleIpcError('load providers', err); }
    isLoading = false;
  }

  function selectProvider(id: string) {
    selectedProviderId = id;
    testResult = null;
    const prov = providers.find(p => p.id === id);
    if (prov) embeddingModel = prov.config.embedding_model || s(prov.adapter).model;
  }

  async function saveEmbeddingModel() {
    if (!isTauri || !selectedProviderId || !embeddingModel.trim()) return;
    isSaving = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const existing = await ipc.getProvider(selectedProviderId);
      const config = { ...(existing.config as Record<string, unknown>), embedding_model: embeddingModel };
      await ipc.updateProvider(selectedProviderId, undefined, config);
      providers = providers.map(p =>
        p.id === selectedProviderId
          ? { ...p, config: { ...p.config, embedding_model: embeddingModel } }
          : p
      );
      success('Embedding model saved');
    } catch (err) { handleIpcError('save', err); }
    isSaving = false;
  }

  async function testEmbedding() {
    if (!isTauri || isTesting || !embeddingModel.trim()) return;
    isTesting = true; testResult = null;
    const t0 = Date.now();
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.getEmbeddingIndexStatus(null, embeddingModel);
      testResult = { ok: true, latency: Date.now() - t0 };
      success(`Connected · ${testResult.latency}ms`);
    } catch {
      testResult = { ok: false, latency: Date.now() - t0 };
      toastError('Connection failed');
    }
    isTesting = false;
  }
</script>

<svelte:head><title>Embedders — Mythic</title></svelte:head>

<div class="page">
  <div class="ambient">
    <div class="glow glow-1"></div>
    <div class="glow glow-2"></div>
  </div>

  <div class="scroll-area">
    <!-- Header -->
    <header class="hdr">
      <div class="hdr-top">
        <h1 class="hdr-title">Embedders</h1>
        <button class="icon-btn" onclick={loadProviders} disabled={isLoading} title="Refresh">
          <Icon name="refresh-cw" size={14} color={isLoading ? '#2a2a3a' : '#6b6b8a'} />
        </button>
      </div>
      <p class="hdr-sub">Select a provider and configure the embedding model used for semantic memory retrieval.</p>
    </header>

    {#if isLoading}
      <div class="container">
        <div class="skel-row">
          <Skeleton variant="text" width="100%" height="62px" />
        </div>
        <div class="skel-row">
          <Skeleton variant="text" width="100%" height="180px" />
        </div>
      </div>
    {:else if embeddingProviders.length === 0}
      <div class="empty">
        <div class="empty-icon"><Icon name="zap" size={28} color="#2a2a4a" /></div>
        <span class="empty-title">No embedding providers found</span>
        <span class="empty-sub">Add a provider that supports embeddings in <a href="/providers" class="link">Providers</a>.</span>
      </div>
    {:else}
      <div class="container">
        <!-- Provider selector -->
        <div class="field-group">
          <label class="field-lbl">Provider</label>
          <div class="provider-list">
            {#each embeddingProviders as p, i (p.id)}
              {@const pm = m(p.adapter)}
              {@const active = selectedProviderId === p.id}
              {@const configured = !!p.config.embedding_model}
              <button
                class="prov"
                class:prov-active={active}
                style="--c: {pm.color}; animation-delay: {i * 40}ms"
                onclick={() => selectProvider(p.id)}
              >
                <div class="prov-icon" class:prov-icon-active={active}>
                  <Icon name={pm.icon} size={15} color={active ? pm.color : '#4a4a6a'} />
                </div>
                <div class="prov-info">
                  <span class="prov-name">{p.name}</span>
                  <span class="prov-type">{pm.label}</span>
                </div>
                <div class="prov-end">
                  {#if configured}
                    <span class="dot-ok" title="Configured"></span>
                  {/if}
                  {#if active}
                    <Icon name="chevron-right" size={14} color={pm.color} />
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        </div>

        <!-- Configuration panel -->
        {#if selectedProvider}
          {@const pm = m(selectedProvider.adapter)}
          {@const sg = s(selectedProvider.adapter)}
          <div class="config" style="--c: {pm.color}">
            <!-- Model input -->
            <div class="field-group">
              <label class="field-lbl" for="model-input">Embedding Model</label>
              <div class="input-row">
                <input
                  id="model-input"
                  class="input"
                  bind:value={embeddingModel}
                  placeholder={sg.model}
                  spellcheck="false"
                  autocomplete="off"
                />
                <button class="btn-save" onclick={saveEmbeddingModel} disabled={!embeddingModel.trim() || isSaving}>
                  {#if isSaving}
                    <span class="spin"></span>
                  {:else}
                    <Icon name="check" size={14} color="#fff" />
                  {/if}
                </button>
              </div>
              <button class="hint" onclick={() => { embeddingModel = sg.model; }}>
                <Icon name="sparkles" size={10} color="#4a4a6a" />
                <span>Suggested: <code>{sg.model}</code></span>
                <span class="dims">{sg.dims}d</span>
              </button>
            </div>

            <!-- Active model readout -->
            {#if selectedProvider.config.embedding_model}
              <div class="active-model">
                <span class="active-label">Active</span>
                <code class="active-value">{selectedProvider.config.embedding_model}</code>
              </div>
            {/if}

            <!-- Test -->
            <div class="test-row">
              <button class="btn-test" onclick={testEmbedding} disabled={isTesting || !embeddingModel.trim()}>
                {#if isTesting}
                  <span class="spin"></span>
                  <span>Connecting…</span>
                {:else}
                  <Icon name="activity" size={14} color="#6b6b8a" />
                  <span>Test Connection</span>
                {/if}
              </button>
              {#if testResult}
                <div class="badge" class:badge-ok={testResult.ok} class:badge-fail={!testResult.ok}>
                  {#if testResult.ok}
                    <Icon name="check-circle" size={12} color="#10B981" />
                    <span>OK · {testResult.latency}ms</span>
                  {:else}
                    <Icon name="x-circle" size={12} color="#F43F5E" />
                    <span>Failed</span>
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  /* ── Page ── */
  .page {
    flex: 1; display: flex; flex-direction: column;
    background: #060610; position: relative; overflow: hidden;
  }

  /* ── Ambient ── */
  .ambient { position: absolute; inset: 0; pointer-events: none; }
  .glow {
    position: absolute; border-radius: 50%; filter: blur(120px); opacity: 0.05;
  }
  .glow-1 { width: 500px; height: 500px; top: -180px; right: -80px; background: #8B5CF6; }
  .glow-2 { width: 300px; height: 300px; bottom: -80px; left: 10%; background: #3B82F6; }

  .scroll-area {
    flex: 1; overflow-y: auto; position: relative; z-index: 1;
    padding: 28px 32px 48px;
  }
  .scroll-area::-webkit-scrollbar { width: 3px; }
  .scroll-area::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.1); border-radius: 3px; }

  /* ── Header ── */
  .hdr { margin-bottom: 28px; max-width: 520px; }
  .hdr-top { display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px; }
  .hdr-title {
    font-size: 22px; font-weight: 800; letter-spacing: -0.5px; margin: 0;
    color: #e8e0ff;
  }
  .hdr-sub {
    font-size: 13px; color: #3e3e5e; line-height: 1.5; margin: 0;
  }

  .icon-btn {
    width: 32px; height: 32px; border-radius: 9px;
    border: 1px solid rgba(255,255,255,0.04); background: rgba(255,255,255,0.02);
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 200ms;
  }
  .icon-btn:hover { background: rgba(255,255,255,0.05); border-color: rgba(255,255,255,0.08); }
  .icon-btn:disabled { opacity: 0.3; pointer-events: none; }

  /* ── Container ── */
  .container {
    max-width: 520px;
    display: flex; flex-direction: column; gap: 2px;
  }

  /* ── Field Group ── */
  .field-group {
    display: flex; flex-direction: column; gap: 8px;
  }
  .field-lbl {
    font-size: 11px; font-weight: 600; color: #4a4a6a;
    letter-spacing: 0.4px; padding-left: 2px;
  }

  /* ━━━ Provider List ━━━ */
  .provider-list {
    display: flex; flex-direction: column; gap: 4px;
  }
  .prov {
    display: flex; align-items: center; gap: 12px;
    padding: 12px 14px; border-radius: 12px;
    background: rgba(255,255,255,0.015);
    border: 1px solid rgba(255,255,255,0.03);
    cursor: pointer; transition: all 180ms ease;
    text-align: left; width: 100%;
    font-family: var(--font-body);
    animation: fadeUp 300ms ease both;
  }
  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .prov:hover {
    background: rgba(255,255,255,0.03);
    border-color: rgba(255,255,255,0.06);
  }
  .prov-active {
    background: color-mix(in srgb, var(--c) 5%, transparent) !important;
    border-color: color-mix(in srgb, var(--c) 14%, transparent) !important;
  }

  .prov-icon {
    width: 34px; height: 34px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.03);
    transition: all 200ms; flex-shrink: 0;
  }
  .prov-icon-active {
    background: color-mix(in srgb, var(--c) 8%, transparent);
    border-color: color-mix(in srgb, var(--c) 12%, transparent);
  }

  .prov-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .prov-name {
    font-size: 13px; font-weight: 600; color: #c0c0d8;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .prov-active .prov-name { color: #e8e0ff; }
  .prov-type { font-size: 10px; color: #3a3a5a; font-family: var(--font-mono); }

  .prov-end { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .dot-ok {
    width: 6px; height: 6px; border-radius: 50%;
    background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.4);
  }

  /* ━━━ Config Panel ━━━ */
  .config {
    margin-top: 20px;
    padding: 22px;
    border-radius: 16px;
    background: rgba(255,255,255,0.015);
    border: 1px solid rgba(255,255,255,0.04);
    display: flex; flex-direction: column; gap: 18px;
    animation: fadeUp 250ms ease;
    position: relative;
  }
  .config::before {
    content: ''; position: absolute; top: 0; left: 24px; right: 24px; height: 1px;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--c) 15%, transparent), transparent);
  }

  /* ── Input Row ── */
  .input-row { display: flex; gap: 8px; }
  .input {
    flex: 1; height: 40px; padding: 0 14px; border-radius: 10px;
    background: rgba(0,0,0,0.3);
    border: 1px solid rgba(255,255,255,0.06);
    color: #e0e0f0; font-size: 13px; font-family: var(--font-mono);
    outline: none; transition: all 180ms;
  }
  .input::placeholder { color: #2a2a3e; }
  .input:focus {
    border-color: color-mix(in srgb, var(--c) 35%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 5%, transparent);
  }

  .btn-save {
    width: 40px; height: 40px; border-radius: 10px; border: none;
    background: var(--c); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: all 180ms; flex-shrink: 0;
    box-shadow: 0 2px 12px color-mix(in srgb, var(--c) 25%, transparent);
  }
  .btn-save:hover { transform: translateY(-1px); box-shadow: 0 4px 20px color-mix(in srgb, var(--c) 35%, transparent); }
  .btn-save:active { transform: translateY(0) scale(0.96); }
  .btn-save:disabled { opacity: 0.3; pointer-events: none; }

  /* ── Hint ── */
  .hint {
    display: inline-flex; align-items: center; gap: 5px; width: fit-content;
    padding: 4px 10px; border-radius: 6px;
    background: transparent; border: 1px solid rgba(255,255,255,0.03);
    font-size: 10px; color: #4a4a6a; cursor: pointer;
    font-family: var(--font-body);
    transition: all 150ms;
  }
  .hint:hover { background: rgba(255,255,255,0.02); border-color: rgba(255,255,255,0.06); }
  .hint code {
    color: #6b6b8a; font-family: var(--font-mono); font-size: 10px;
    background: none; padding: 0;
  }
  .dims {
    color: #3a3a5a; font-family: var(--font-mono); font-size: 9px;
    padding: 1px 4px; border-radius: 3px;
    background: rgba(255,255,255,0.03);
  }

  /* ── Active Model ── */
  .active-model {
    display: flex; align-items: center; gap: 8px;
    padding: 9px 14px; border-radius: 9px;
    background: rgba(16,185,129,0.03);
    border: 1px solid rgba(16,185,129,0.06);
  }
  .active-label {
    font-size: 9px; font-weight: 700; letter-spacing: 0.8px;
    text-transform: uppercase; color: #10B981;
    font-family: var(--font-mono);
  }
  .active-value {
    font-size: 12px; color: #8b8ba7; font-family: var(--font-mono);
    background: none; padding: 0;
  }

  /* ── Test Row ── */
  .test-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .btn-test {
    display: flex; align-items: center; gap: 7px;
    padding: 9px 16px; border-radius: 9px;
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.05);
    color: #8b8ba7; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 160ms;
  }
  .btn-test:hover { background: rgba(255,255,255,0.04); border-color: rgba(255,255,255,0.08); }
  .btn-test:disabled { opacity: 0.3; pointer-events: none; }

  .badge {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 10px; border-radius: 7px;
    font-size: 11px; font-weight: 600; border: 1px solid;
    animation: pop 200ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes pop { from { opacity: 0; transform: scale(0.9); } to { opacity: 1; transform: scale(1); } }
  .badge-ok { background: rgba(16,185,129,0.06); border-color: rgba(16,185,129,0.1); color: #10B981; }
  .badge-fail { background: rgba(244,63,94,0.06); border-color: rgba(244,63,94,0.1); color: #F43F5E; }

  /* ── Spinner ── */
  .spin {
    width: 14px; height: 14px; border-radius: 50%;
    border: 2px solid rgba(255,255,255,0.12);
    border-top-color: rgba(255,255,255,0.7);
    animation: spin 600ms linear infinite; flex-shrink: 0;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Empty ── */
  .empty {
    display: flex; flex-direction: column; align-items: center;
    gap: 12px; padding: 80px 20px; text-align: center;
  }
  .empty-icon { opacity: 0.5; margin-bottom: 4px; }
  .empty-title { font-size: 14px; font-weight: 700; color: #4a4a6a; }
  .empty-sub { font-size: 12px; color: #3a3a5a; line-height: 1.5; }
  .link { color: #8B5CF6; text-decoration: none; font-weight: 600; }
  .link:hover { text-decoration: underline; }

  /* ── Loading ── */
  .skel-row { margin-bottom: 12px; }
</style>
