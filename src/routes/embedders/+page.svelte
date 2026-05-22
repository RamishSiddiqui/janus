<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── Types ─────────────────────────────────────────────────
  interface ProviderRow {
    id: string;
    name: string;
    adapter: string;
    config: Record<string, string>;
  }



  // ── State ─────────────────────────────────────────────────
  let providers = $state<ProviderRow[]>([]);
  let isLoading = $state(true);
  let selectedProviderId = $state<string | null>(null);
  let embeddingModel = $state('');

  let isTesting = $state(false);
  let testResult = $state<{ ok: boolean; dims: number; latency: number } | null>(null);

  // Derived: selected provider
  let selectedProvider = $derived(providers.find(p => p.id === selectedProviderId) ?? null);
  let providerEmbedModel = $derived(selectedProvider?.config.embedding_model ?? '');

  // Adapters that support embeddings
  const embeddingAdapters = new Set([
    'open_router', 'open_ai_compatible', 'openai_compatible', 'ollama',
    'lm_studio', 'gemini', 'cohere', 'together',
  ]);
  let embeddingProviders = $derived(providers.filter(p => embeddingAdapters.has(p.adapter)));

  onMount(async () => {
    await loadProviders();
  });

  async function loadProviders() {
    if (!isTauri) { isLoading = false; return; }
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const rows = await ipc.listProviders();
      providers = rows.map(p => ({
        id: p.id,
        name: p.name,
        adapter: p.adapter,
        config: p.config as Record<string, string>,
      }));
      // Auto-select first embedding-capable provider
      const embProv = providers.find(p => embeddingAdapters.has(p.adapter));
      if (embProv) {
        selectedProviderId = embProv.id;
        embeddingModel = embProv.config.embedding_model || suggestModel(embProv.adapter);
      }
    } catch (err) { handleIpcError('load providers', err); }
    isLoading = false;
  }

  function suggestModel(adapter: string): string {
    const suggestions: Record<string, string> = {
      open_router: 'openai/text-embedding-3-small',
      open_ai_compatible: 'text-embedding-3-small',
      openai_compatible: 'text-embedding-3-small',
      ollama: 'nomic-embed-text',
      lm_studio: 'nomic-embed-text',
      gemini: 'text-embedding-004',
      cohere: 'embed-english-v3.0',
      together: 'togethercomputer/m2-bert-80M-8k-retrieval',
    };
    return suggestions[adapter] ?? 'text-embedding-3-small';
  }

  function adapterLabel(a: string) {
    const map: Record<string, string> = {
      open_router: 'OpenRouter', ollama: 'Ollama',
      open_ai_compatible: 'OpenAI Compatible', openai_compatible: 'OpenAI Compatible',
      lm_studio: 'LM Studio', gemini: 'Gemini',
      cohere: 'Cohere', together: 'Together',
    };
    return map[a] ?? a;
  }

  function adapterColor(a: string): string {
    const map: Record<string, string> = {
      open_router: '#8B5CF6', ollama: '#10B981',
      open_ai_compatible: '#3B82F6', openai_compatible: '#3B82F6',
      lm_studio: '#06B6D4', gemini: '#4285F4',
      cohere: '#D97706', together: '#6366F1',
    };
    return map[a] ?? '#6b6b8a';
  }



  async function testEmbedding() {
    if (!isTauri || isTesting || !embeddingModel.trim()) return;
    isTesting = true;
    testResult = null;
    const t0 = Date.now();
    try {
      const ipc = await import('$lib/services/ipc');
      // Use a simple test embedding via rebuild with 0 messages
      // For now just check index status as a connectivity test
      await ipc.getEmbeddingIndexStatus(null, embeddingModel);
      testResult = { ok: true, dims: 0, latency: Date.now() - t0 };
      success(`Embedder connected (${testResult.latency}ms)`);
    } catch (err) {
      testResult = { ok: false, dims: 0, latency: Date.now() - t0 };
      console.error('[Embedders] Test failed:', err);
    }
    isTesting = false;
  }

  // Save embedding model to provider config
  async function saveEmbeddingModel() {
    if (!isTauri || !selectedProviderId || !embeddingModel.trim()) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const existing = await ipc.getProvider(selectedProviderId);
      const config = { ...(existing.config as Record<string, unknown>), embedding_model: embeddingModel };
      await ipc.updateProvider(selectedProviderId, undefined, config);
      // Update local state
      providers = providers.map(p =>
        p.id === selectedProviderId
          ? { ...p, config: { ...p.config, embedding_model: embeddingModel } }
          : p
      );
      success('Embedding model saved');
    } catch (err) { handleIpcError('save embedding model', err); }
  }

  // When provider changes, load its embedding model
  $effect(() => {
    if (selectedProvider) {
      embeddingModel = selectedProvider.config.embedding_model || suggestModel(selectedProvider.adapter);
    }
  });
</script>

<svelte:head><title>Embedders — Mythic</title></svelte:head>

<div class="page">
  <!-- Header -->
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title">Embedders</h1>
      <span class="hdr-sub">Configure embedding models for semantic memory</span>
    </div>
    <button class="btn-refresh" onclick={loadProviders} disabled={isLoading} aria-label="Refresh">
      <Icon name="refresh-cw" size={13} color={isLoading ? '#4a4a6a' : '#8B5CF6'} />
      Refresh
    </button>
  </header>

  <div class="content">
    {#if isLoading}
      <div class="loading-grid">
        {#each Array(3) as _}
          <div class="skeleton-card">
            <Skeleton variant="text" width="40%" height="14px" />
            <Skeleton variant="text" width="70%" height="11px" />
            <Skeleton variant="text" width="55%" height="11px" />
          </div>
        {/each}
      </div>
    {:else if embeddingProviders.length === 0}
      <div class="empty-state">
        <div class="empty-icon-wrap">
          <Icon name="zap" size={48} color="#3a3a5a" />
          <div class="empty-glow"></div>
        </div>
        <span class="empty-title">No embedding-capable providers</span>
        <span class="empty-sub">Add a provider that supports embeddings (OpenAI, OpenRouter, Ollama, Gemini, Cohere, or Together) in the Providers section.</span>
      </div>
    {:else}
      <div class="embedder-layout">
        <!-- Provider Selection -->
        <div class="section-card">
          <div class="section-header">
            <Icon name="plug" size={14} color="#a78bfa" />
            <span class="section-title">Provider</span>
            <span class="section-count">{embeddingProviders.length} available</span>
          </div>

          <div class="provider-chips">
            {#each embeddingProviders as p (p.id)}
              {@const isSelected = selectedProviderId === p.id}
              <button
                class="provider-chip"
                class:chip-selected={isSelected}
                onclick={() => { selectedProviderId = p.id; }}
              >
                <span class="chip-dot" style="background: {adapterColor(p.adapter)}"></span>
                <div class="chip-content">
                  <span class="chip-name">{p.name}</span>
                  <span class="chip-adapter">{adapterLabel(p.adapter)}</span>
                </div>
                {#if p.config.embedding_model}
                  <span class="chip-badge">Configured</span>
                {/if}
                {#if isSelected}
                  <span class="chip-check">
                    <Icon name="check" size={12} color="#10B981" />
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        </div>

        <!-- Model Configuration -->
        {#if selectedProvider}
          <div class="section-card" style="animation: slideDown 220ms cubic-bezier(0.34,1.56,0.64,1)">
            <div class="section-header">
              <Icon name="cpu" size={14} color="#a78bfa" />
              <span class="section-title">Embedding Model</span>
              <span class="section-hint">{adapterLabel(selectedProvider.adapter)}</span>
            </div>

            <div class="model-config">
              <div class="model-input-row">
                <input
                  class="model-input"
                  bind:value={embeddingModel}
                  placeholder={suggestModel(selectedProvider.adapter)}
                  aria-label="Embedding model ID"
                />
                <button class="btn-save" onclick={saveEmbeddingModel} disabled={!embeddingModel.trim()}>
                  <Icon name="save" size={12} color="#e0e0f0" />
                  Save
                </button>
              </div>

              <div class="model-suggestion">
                <Icon name="info" size={11} color="#4a4a6a" />
                <span>Suggested: <code>{suggestModel(selectedProvider.adapter)}</code></span>
              </div>

              <!-- Action Buttons -->
              <div class="action-row">
                <button class="action-btn" class:testing={isTesting} onclick={testEmbedding} disabled={isTesting || !embeddingModel.trim()}>
                  {#if isTesting}
                    <span class="spinner"></span>
                    Testing…
                  {:else}
                    <Icon name="activity" size={13} color="#a78bfa" />
                    Test Connection
                  {/if}
                </button>

                {#if testResult}
                  <span class="test-result" class:test-ok={testResult.ok} class:test-fail={!testResult.ok}>
                    {#if testResult.ok}
                      <Icon name="check-circle" size={12} color="#10B981" />
                      Connected ({testResult.latency}ms)
                    {:else}
                      <Icon name="x-circle" size={12} color="#F43F5E" />
                      Failed
                    {/if}
                  </span>
                {/if}
              </div>
            </div>
          </div>

        {/if}
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

  .btn-refresh {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.15); background: rgba(139,92,246,0.06);
    color: #8B5CF6; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 180ms;
  }
  .btn-refresh:hover { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.3); transform: translateY(-1px); }
  .btn-refresh:disabled { opacity: 0.4; pointer-events: none; }

  /* ── Content ── */
  .content {
    flex: 1; overflow-y: auto; padding: 20px 28px 32px;
  }
  .content::-webkit-scrollbar { width: 4px; }
  .content::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  .embedder-layout {
    display: flex; flex-direction: column; gap: 16px;
    max-width: 680px;
  }

  /* ── Section Card ── */
  .section-card {
    border-radius: 14px; padding: 18px 20px;
    background: rgba(12,12,26,0.6);
    border: 1px solid rgba(139,92,246,0.07);
    display: flex; flex-direction: column; gap: 14px;
    animation: cardIn 240ms ease both;
  }
  @keyframes cardIn { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }
  @keyframes slideDown { from { opacity: 0; transform: translateY(-12px); } to { opacity: 1; transform: translateY(0); } }

  .section-header {
    display: flex; align-items: center; gap: 8px;
  }
  .section-title {
    font-size: 11px; font-weight: 700; letter-spacing: 1px;
    text-transform: uppercase; color: #a78bfa;
    font-family: var(--font-mono);
  }
  .section-count {
    margin-left: auto; font-size: 10px; color: #4a4a6a;
    font-family: var(--font-mono);
  }
  .section-hint {
    margin-left: auto; font-size: 10px; color: #4a4a6a;
    padding: 2px 8px; border-radius: 99px;
    background: rgba(139,92,246,0.06);
  }

  /* ── Provider Chips ── */
  .provider-chips {
    display: flex; flex-direction: column; gap: 6px;
  }
  .provider-chip {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 14px; border-radius: 10px;
    background: rgba(10,10,24,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    cursor: pointer; transition: all 180ms;
    font-family: var(--font-body); text-align: left; width: 100%;
    position: relative;
  }
  .provider-chip:hover {
    background: rgba(139,92,246,0.06);
    border-color: rgba(139,92,246,0.12);
  }
  .chip-selected {
    background: rgba(139,92,246,0.08) !important;
    border-color: rgba(139,92,246,0.22) !important;
    box-shadow: 0 0 20px rgba(139,92,246,0.06);
  }

  .chip-dot {
    width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0;
    box-shadow: 0 0 6px color-mix(in srgb, currentColor 30%, transparent);
  }
  .chip-content {
    display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0;
  }
  .chip-name {
    font-size: 13px; font-weight: 600; color: #d0d0e8;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .chip-adapter {
    font-size: 10px; color: #4a4a6a; font-family: var(--font-mono);
  }
  .chip-badge {
    padding: 2px 7px; border-radius: 99px;
    background: rgba(16,185,129,0.1); border: 1px solid rgba(16,185,129,0.2);
    color: #10B981; font-size: 9px; font-weight: 700; font-family: var(--font-mono);
    letter-spacing: 0.3px;
  }
  .chip-check {
    width: 22px; height: 22px; border-radius: 6px;
    display: flex; align-items: center; justify-content: center;
    background: rgba(16,185,129,0.1);
  }

  /* ── Model Config ── */
  .model-config {
    display: flex; flex-direction: column; gap: 12px;
  }
  .model-input-row {
    display: flex; gap: 8px;
  }
  .model-input {
    flex: 1; height: 38px; padding: 0 14px; border-radius: 10px;
    background: rgba(10,10,24,0.8); border: 1px solid rgba(139,92,246,0.1);
    color: #e0e0f0; font-size: 13px; font-family: var(--font-mono); outline: none;
    transition: border-color 200ms;
  }
  .model-input:focus { border-color: rgba(139,92,246,0.4); }

  .btn-save {
    display: flex; align-items: center; gap: 6px;
    padding: 0 16px; border-radius: 10px; border: none; cursor: pointer;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    color: #fff; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    box-shadow: 0 2px 12px rgba(139,92,246,0.3);
    transition: all 180ms;
  }
  .btn-save:hover { transform: translateY(-1px); box-shadow: 0 4px 20px rgba(139,92,246,0.45); }
  .btn-save:disabled { opacity: 0.4; pointer-events: none; }

  .model-suggestion {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: #4a4a6a;
  }
  .model-suggestion code {
    padding: 2px 6px; border-radius: 4px;
    background: rgba(139,92,246,0.08); color: #a78bfa;
    font-size: 10px; font-family: var(--font-mono);
  }

  /* ── Actions ── */
  .action-row {
    display: flex; align-items: center; gap: 10px;
  }
  .action-btn {
    display: flex; align-items: center; gap: 7px;
    padding: 8px 16px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.12);
    background: rgba(139,92,246,0.06);
    color: #c0c0d8; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 180ms;
  }
  .action-btn:hover { background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.2); }
  .action-btn:disabled { opacity: 0.4; cursor: default; }
  .action-btn.testing { color: #a78bfa; }

  .test-result {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; font-weight: 600;
  }
  .test-ok { color: #10B981; }
  .test-fail { color: #F43F5E; }



  /* ── Spinner ── */
  .spinner {
    width: 14px; height: 14px; border-radius: 50%;
    border: 2px solid rgba(139,92,246,0.2);
    border-top-color: #a78bfa;
    animation: spin 700ms linear infinite;
    flex-shrink: 0;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Empty State ── */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; gap: 14px;
    padding: 80px 20px; text-align: center;
  }
  .empty-icon-wrap { position: relative; }
  .empty-glow {
    position: absolute; inset: -20px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.1) 0%, transparent 70%);
    pointer-events: none;
  }
  .empty-title { font-size: 15px; font-weight: 700; color: #6b6b8a; }
  .empty-sub { font-size: 13px; color: #4a4a6a; max-width: 380px; line-height: 1.5; }

  /* ── Loading ── */
  .loading-grid { display: flex; flex-direction: column; gap: 12px; }
  .skeleton-card {
    border-radius: 14px; padding: 18px 20px;
    background: rgba(12,12,26,0.6);
    border: 1px solid rgba(139,92,246,0.07);
    display: flex; flex-direction: column; gap: 8px;
  }
</style>
