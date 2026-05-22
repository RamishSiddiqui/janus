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
  let isSaving = $state(false);
  let testResult = $state<{ ok: boolean; latency: number } | null>(null);

  // Derived
  let selectedProvider = $derived(providers.find(p => p.id === selectedProviderId) ?? null);

  const embeddingAdapters = new Set([
    'open_router', 'open_ai_compatible', 'openai_compatible', 'ollama',
    'lm_studio', 'gemini', 'cohere', 'together',
  ]);
  let embeddingProviders = $derived(providers.filter(p => embeddingAdapters.has(p.adapter)));

  // ── Adapter Metadata ──────────────────────────────────────
  const adapterMeta: Record<string, { label: string; color: string; accent: string; icon: string }> = {
    open_router:        { label: 'OpenRouter',        color: '#8B5CF6', accent: '#c084fc', icon: 'globe' },
    open_ai_compatible: { label: 'OpenAI Compatible', color: '#3B82F6', accent: '#60a5fa', icon: 'box' },
    openai_compatible:  { label: 'OpenAI Compatible', color: '#3B82F6', accent: '#60a5fa', icon: 'box' },
    ollama:             { label: 'Ollama',            color: '#10B981', accent: '#34d399', icon: 'terminal' },
    lm_studio:          { label: 'LM Studio',         color: '#06B6D4', accent: '#22d3ee', icon: 'monitor' },
    gemini:             { label: 'Gemini',            color: '#4285F4', accent: '#93bbfd', icon: 'sparkles' },
    cohere:             { label: 'Cohere',            color: '#D97706', accent: '#fbbf24', icon: 'sun' },
    together:           { label: 'Together',          color: '#6366F1', accent: '#818cf8', icon: 'users' },
  };

  const modelSuggestions: Record<string, { model: string; dims: string }> = {
    open_router:        { model: 'openai/text-embedding-3-small', dims: '1536' },
    open_ai_compatible: { model: 'text-embedding-3-small',        dims: '1536' },
    openai_compatible:  { model: 'text-embedding-3-small',        dims: '1536' },
    ollama:             { model: 'nomic-embed-text',              dims: '768' },
    lm_studio:          { model: 'nomic-embed-text',              dims: '768' },
    gemini:             { model: 'text-embedding-004',            dims: '768' },
    cohere:             { model: 'embed-english-v3.0',            dims: '1024' },
    together:           { model: 'togethercomputer/m2-bert-80M-8k-retrieval', dims: '768' },
  };

  function getMeta(adapter: string) {
    return adapterMeta[adapter] ?? { label: adapter, color: '#6b6b8a', accent: '#9ca3af', icon: 'cpu' };
  }
  function getSuggestion(adapter: string) {
    return modelSuggestions[adapter] ?? { model: 'text-embedding-3-small', dims: '1536' };
  }

  // ── Lifecycle ─────────────────────────────────────────────
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
      const embProv = providers.find(p => embeddingAdapters.has(p.adapter));
      if (embProv) {
        selectedProviderId = embProv.id;
        embeddingModel = embProv.config.embedding_model || getSuggestion(embProv.adapter).model;
      }
    } catch (err) { handleIpcError('load providers', err); }
    isLoading = false;
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
    } catch (err) { handleIpcError('save embedding model', err); }
    isSaving = false;
  }

  async function testEmbedding() {
    if (!isTauri || isTesting || !embeddingModel.trim()) return;
    isTesting = true;
    testResult = null;
    const t0 = Date.now();
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.getEmbeddingIndexStatus(null, embeddingModel);
      testResult = { ok: true, latency: Date.now() - t0 };
      success(`Connected (${testResult.latency}ms)`);
    } catch {
      testResult = { ok: false, latency: Date.now() - t0 };
      toastError('Connection failed');
    }
    isTesting = false;
  }

  function selectProvider(id: string) {
    selectedProviderId = id;
    testResult = null;
    const prov = providers.find(p => p.id === id);
    if (prov) {
      embeddingModel = prov.config.embedding_model || getSuggestion(prov.adapter).model;
    }
  }
</script>

<svelte:head><title>Embedders — Mythic</title></svelte:head>

<div class="page">
  <!-- Ambient background effects -->
  <div class="ambient">
    <div class="orb orb-1"></div>
    <div class="orb orb-2"></div>
    <div class="grid-lines"></div>
  </div>

  <!-- Header -->
  <header class="hdr">
    <div class="hdr-left">
      <div class="hdr-icon-wrap">
        <Icon name="zap" size={18} color="#a78bfa" />
        <div class="hdr-icon-ring"></div>
      </div>
      <div class="hdr-text">
        <h1 class="hdr-title">Embedders</h1>
        <p class="hdr-sub">Configure vector embedding models for semantic memory</p>
      </div>
    </div>
    <button class="btn-ghost" onclick={loadProviders} disabled={isLoading}>
      <Icon name="refresh-cw" size={14} color={isLoading ? '#3a3a5a' : '#a78bfa'} />
    </button>
  </header>

  <div class="content">
    {#if isLoading}
      <!-- Skeleton loading state -->
      <div class="loading-state">
        {#each Array(3) as _, i}
          <div class="skel-card" style="animation-delay: {i * 80}ms">
            <Skeleton variant="text" width="60%" height="16px" />
            <Skeleton variant="text" width="40%" height="12px" />
          </div>
        {/each}
      </div>

    {:else if embeddingProviders.length === 0}
      <!-- Empty state -->
      <div class="empty">
        <div class="empty-visual">
          <div class="empty-ring"></div>
          <div class="empty-ring ring-2"></div>
          <Icon name="zap" size={32} color="#2a2a4a" />
        </div>
        <h2 class="empty-title">No embedding providers</h2>
        <p class="empty-desc">Add a provider that supports embeddings — OpenAI, OpenRouter, Ollama, Gemini, Cohere, or Together — in <a href="/providers" class="empty-link">Providers</a>.</p>
      </div>

    {:else}
      <div class="main-layout">
        <!-- ━━━ Left: Provider Cards ━━━ -->
        <section class="providers-section">
          <div class="section-label">
            <span class="label-text">Select Provider</span>
            <span class="label-count">{embeddingProviders.length}</span>
          </div>

          <div class="provider-grid">
            {#each embeddingProviders as p, i (p.id)}
              {@const meta = getMeta(p.adapter)}
              {@const isSelected = selectedProviderId === p.id}
              {@const isConfigured = !!p.config.embedding_model}
              <button
                class="provider-card"
                class:selected={isSelected}
                style="--accent: {meta.color}; --accent-soft: {meta.accent}; animation-delay: {i * 50}ms"
                onclick={() => selectProvider(p.id)}
              >
                <!-- Glow effect on selected -->
                {#if isSelected}
                  <div class="card-glow"></div>
                {/if}

                <div class="card-body">
                  <div class="card-icon-wrap">
                    <Icon name={meta.icon} size={16} color={isSelected ? meta.accent : '#4a4a6a'} />
                  </div>

                  <div class="card-info">
                    <span class="card-name">{p.name}</span>
                    <span class="card-adapter">{meta.label}</span>
                  </div>

                  <div class="card-end">
                    {#if isConfigured}
                      <span class="status-dot configured" title="Configured"></span>
                    {:else}
                      <span class="status-dot unconfigured" title="Not configured"></span>
                    {/if}
                  </div>
                </div>

                {#if isSelected}
                  <div class="card-active-bar"></div>
                {/if}
              </button>
            {/each}
          </div>
        </section>

        <!-- ━━━ Right: Configuration Panel ━━━ -->
        {#if selectedProvider}
          {@const meta = getMeta(selectedProvider.adapter)}
          {@const suggestion = getSuggestion(selectedProvider.adapter)}
          <section class="config-section" style="--accent: {meta.color}; --accent-soft: {meta.accent}">
            <!-- Panel header -->
            <div class="panel-header">
              <div class="panel-provider">
                <div class="panel-icon">
                  <Icon name={meta.icon} size={14} color={meta.accent} />
                </div>
                <div class="panel-provider-info">
                  <span class="panel-provider-name">{selectedProvider.name}</span>
                  <span class="panel-provider-adapter">{meta.label}</span>
                </div>
              </div>
              <div class="panel-badge" style="background: {meta.color}15; border-color: {meta.color}30; color: {meta.accent}">
                Embedding
              </div>
            </div>

            <!-- Model Input -->
            <div class="config-block">
              <label class="field-label" for="embed-model">
                <Icon name="cpu" size={12} color="#6b6b8a" />
                Model ID
              </label>
              <div class="input-group">
                <input
                  id="embed-model"
                  class="model-input"
                  bind:value={embeddingModel}
                  placeholder={suggestion.model}
                  spellcheck="false"
                  autocomplete="off"
                />
                <button
                  class="btn-primary"
                  onclick={saveEmbeddingModel}
                  disabled={!embeddingModel.trim() || isSaving}
                >
                  {#if isSaving}
                    <span class="btn-spinner"></span>
                  {:else}
                    <Icon name="check" size={14} color="#fff" />
                  {/if}
                  Save
                </button>
              </div>
            </div>

            <!-- Suggestion chip -->
            <button class="suggestion-chip" onclick={() => { embeddingModel = suggestion.model; }}>
              <Icon name="sparkles" size={11} color="#6b6b8a" />
              <span class="suggestion-label">Suggested:</span>
              <code class="suggestion-model">{suggestion.model}</code>
              <span class="suggestion-dims">{suggestion.dims}d</span>
            </button>

            <!-- Divider -->
            <div class="divider"></div>

            <!-- Test Connection -->
            <div class="test-area">
              <button
                class="btn-test"
                class:testing={isTesting}
                onclick={testEmbedding}
                disabled={isTesting || !embeddingModel.trim()}
              >
                {#if isTesting}
                  <span class="btn-spinner"></span>
                  Connecting…
                {:else}
                  <Icon name="activity" size={14} color="var(--accent-soft)" />
                  Test Connection
                {/if}
              </button>

              {#if testResult}
                <div class="test-badge" class:test-ok={testResult.ok} class:test-fail={!testResult.ok}>
                  {#if testResult.ok}
                    <Icon name="check-circle" size={13} color="#10B981" />
                    <span>Connected</span>
                    <span class="test-latency">{testResult.latency}ms</span>
                  {:else}
                    <Icon name="x-circle" size={13} color="#F43F5E" />
                    <span>Failed</span>
                  {/if}
                </div>
              {/if}
            </div>

            <!-- Currently saved model -->
            {#if selectedProvider.config.embedding_model}
              <div class="saved-info">
                <Icon name="bookmark" size={12} color="#4a4a6a" />
                <span class="saved-label">Active:</span>
                <code class="saved-model">{selectedProvider.config.embedding_model}</code>
              </div>
            {/if}
          </section>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  /* ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     DESIGN: Neural Constellation — Dark luxury, depth-layered
     glassmorphism with purposeful animations and spatial hierarchy
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ */
  .page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: #06060f;
    position: relative;
  }

  /* ── Ambient background ── */
  .ambient { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
  .orb {
    position: absolute; border-radius: 50%;
    filter: blur(100px); opacity: 0.07;
    animation: orbFloat 20s ease-in-out infinite;
  }
  .orb-1 {
    width: 600px; height: 600px; top: -200px; right: -100px;
    background: radial-gradient(circle, #8B5CF6, transparent 70%);
  }
  .orb-2 {
    width: 400px; height: 400px; bottom: -100px; left: -50px;
    background: radial-gradient(circle, #3B82F6, transparent 70%);
    animation-delay: -10s; animation-duration: 25s;
  }
  .grid-lines {
    position: absolute; inset: 0;
    background-image:
      linear-gradient(rgba(139,92,246,0.02) 1px, transparent 1px),
      linear-gradient(90deg, rgba(139,92,246,0.02) 1px, transparent 1px);
    background-size: 60px 60px;
    mask-image: radial-gradient(ellipse at 50% 0%, black 20%, transparent 70%);
  }
  @keyframes orbFloat {
    0%, 100% { transform: translate(0, 0) scale(1); }
    33% { transform: translate(30px, -20px) scale(1.05); }
    66% { transform: translate(-20px, 15px) scale(0.95); }
  }

  /* ── Header ── */
  .hdr {
    display: flex; align-items: center; justify-content: space-between;
    padding: 28px 32px 20px; position: relative; z-index: 1;
  }
  .hdr-left { display: flex; align-items: center; gap: 14px; }
  .hdr-icon-wrap {
    position: relative;
    width: 38px; height: 38px; border-radius: 12px;
    background: rgba(139,92,246,0.08);
    border: 1px solid rgba(139,92,246,0.12);
    display: flex; align-items: center; justify-content: center;
  }
  .hdr-icon-ring {
    position: absolute; inset: -3px; border-radius: 14px;
    border: 1px solid rgba(139,92,246,0.06);
    animation: ringPulse 3s ease-in-out infinite;
  }
  @keyframes ringPulse {
    0%, 100% { opacity: 0.3; transform: scale(1); }
    50% { opacity: 0.8; transform: scale(1.06); }
  }
  .hdr-text { display: flex; flex-direction: column; gap: 2px; }
  .hdr-title {
    font-size: 20px; font-weight: 700; letter-spacing: -0.4px; margin: 0;
    color: #e8e0ff;
  }
  .hdr-sub {
    font-size: 12px; color: #4a4a6a; margin: 0; font-weight: 400;
    letter-spacing: 0.1px;
  }

  .btn-ghost {
    width: 34px; height: 34px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.08); background: transparent;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 200ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .btn-ghost:hover { background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.16); transform: rotate(90deg); }
  .btn-ghost:disabled { opacity: 0.3; pointer-events: none; }

  /* ── Content ── */
  .content {
    flex: 1; overflow-y: auto; padding: 0 32px 40px;
    position: relative; z-index: 1;
  }
  .content::-webkit-scrollbar { width: 3px; }
  .content::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.12); border-radius: 3px; }

  /* ── Main Layout: Two columns ── */
  .main-layout {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 24px;
    max-width: 820px;
    align-items: start;
  }

  /* ── Section Labels ── */
  .section-label {
    display: flex; align-items: center; gap: 8px;
    margin-bottom: 12px;
  }
  .label-text {
    font-size: 10px; font-weight: 700; letter-spacing: 1.2px;
    text-transform: uppercase; color: #4a4a6a;
    font-family: var(--font-mono);
  }
  .label-count {
    width: 18px; height: 18px; border-radius: 6px;
    display: flex; align-items: center; justify-content: center;
    background: rgba(139,92,246,0.08); border: 1px solid rgba(139,92,246,0.06);
    font-size: 10px; font-weight: 700; color: #6b6b8a;
    font-family: var(--font-mono);
  }

  /* ━━━ Provider Cards ━━━ */
  .provider-grid {
    display: flex; flex-direction: column; gap: 6px;
  }
  .provider-card {
    position: relative; overflow: hidden;
    padding: 14px 16px; border-radius: 14px;
    background: rgba(10,10,24,0.5);
    border: 1px solid rgba(255,255,255,0.03);
    cursor: pointer; transition: all 220ms cubic-bezier(0.34,1.56,0.64,1);
    text-align: left; width: 100%;
    font-family: var(--font-body);
    animation: cardReveal 400ms cubic-bezier(0.34,1.56,0.64,1) both;
  }
  @keyframes cardReveal {
    from { opacity: 0; transform: translateX(-12px) scale(0.97); }
    to { opacity: 1; transform: translateX(0) scale(1); }
  }
  .provider-card:hover {
    background: rgba(14,14,32,0.7);
    border-color: rgba(255,255,255,0.06);
    transform: translateX(4px);
  }
  .provider-card.selected {
    background: rgba(var(--accent), 0.06);
    border-color: var(--accent);
    border-color: rgba(139,92,246,0.15);
  }

  .card-glow {
    position: absolute; inset: 0; pointer-events: none;
    background: radial-gradient(ellipse at 0% 50%, color-mix(in srgb, var(--accent) 6%, transparent), transparent 70%);
    animation: glowIn 300ms ease;
  }
  @keyframes glowIn { from { opacity: 0; } to { opacity: 1; } }

  .card-body {
    position: relative; z-index: 1;
    display: flex; align-items: center; gap: 12px;
  }
  .card-icon-wrap {
    width: 32px; height: 32px; border-radius: 9px;
    display: flex; align-items: center; justify-content: center;
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.04);
    transition: all 200ms;
    flex-shrink: 0;
  }
  .provider-card.selected .card-icon-wrap {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border-color: color-mix(in srgb, var(--accent) 15%, transparent);
  }

  .card-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .card-name {
    font-size: 13px; font-weight: 600; color: #c8c8e0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    transition: color 150ms;
  }
  .provider-card.selected .card-name { color: #e8e0ff; }
  .card-adapter {
    font-size: 10px; color: #3a3a5a; font-family: var(--font-mono);
    letter-spacing: 0.3px;
  }

  .card-end { display: flex; align-items: center; gap: 6px; }
  .status-dot {
    width: 7px; height: 7px; border-radius: 50%;
    transition: all 200ms;
  }
  .status-dot.configured {
    background: #10B981;
    box-shadow: 0 0 8px rgba(16,185,129,0.4);
  }
  .status-dot.unconfigured {
    background: #2a2a3a;
    border: 1px solid #3a3a4a;
  }

  .card-active-bar {
    position: absolute; left: 0; top: 12px; bottom: 12px; width: 3px;
    background: linear-gradient(180deg, var(--accent), var(--accent-soft));
    border-radius: 0 3px 3px 0;
    animation: barSlide 250ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes barSlide { from { transform: scaleY(0); } to { transform: scaleY(1); } }

  /* ━━━ Configuration Panel ━━━ */
  .config-section {
    border-radius: 18px; padding: 24px;
    background: rgba(10,10,24,0.6);
    border: 1px solid rgba(255,255,255,0.04);
    backdrop-filter: blur(12px);
    display: flex; flex-direction: column; gap: 20px;
    animation: panelIn 300ms cubic-bezier(0.34,1.56,0.64,1);
    position: relative; overflow: hidden;
  }
  .config-section::before {
    content: ''; position: absolute; top: 0; left: 0; right: 0; height: 1px;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--accent) 20%, transparent), transparent);
  }
  @keyframes panelIn {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .panel-header {
    display: flex; align-items: center; justify-content: space-between;
  }
  .panel-provider {
    display: flex; align-items: center; gap: 10px;
  }
  .panel-icon {
    width: 30px; height: 30px; border-radius: 9px;
    display: flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .panel-provider-info {
    display: flex; flex-direction: column; gap: 1px;
  }
  .panel-provider-name {
    font-size: 14px; font-weight: 700; color: #e0e0f0;
    letter-spacing: -0.2px;
  }
  .panel-provider-adapter {
    font-size: 10px; color: #4a4a6a; font-family: var(--font-mono);
  }
  .panel-badge {
    padding: 4px 10px; border-radius: 8px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.5px;
    text-transform: uppercase;
    font-family: var(--font-mono);
    border: 1px solid;
  }

  /* ── Model Input ── */
  .config-block {
    display: flex; flex-direction: column; gap: 8px;
  }
  .field-label {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; font-weight: 600; color: #6b6b8a;
    letter-spacing: 0.3px;
  }
  .input-group {
    display: flex; gap: 8px;
  }
  .model-input {
    flex: 1; height: 42px; padding: 0 16px; border-radius: 12px;
    background: rgba(6,6,15,0.8);
    border: 1px solid rgba(255,255,255,0.06);
    color: #e0e0f0; font-size: 13px; font-family: var(--font-mono);
    outline: none; transition: all 200ms;
    letter-spacing: 0.2px;
  }
  .model-input::placeholder { color: #2a2a4a; }
  .model-input:focus {
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 6%, transparent);
  }

  .btn-primary {
    display: flex; align-items: center; gap: 6px;
    padding: 0 18px; height: 42px; border-radius: 12px; border: none;
    background: var(--accent);
    color: #fff; font-size: 12px; font-weight: 700;
    font-family: var(--font-body); cursor: pointer;
    transition: all 200ms cubic-bezier(0.34,1.56,0.64,1);
    letter-spacing: 0.2px;
    box-shadow: 0 2px 16px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .btn-primary:hover {
    transform: translateY(-1px) scale(1.02);
    box-shadow: 0 4px 24px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .btn-primary:active { transform: translateY(0) scale(0.98); }
  .btn-primary:disabled { opacity: 0.4; pointer-events: none; }

  /* ── Suggestion ── */
  .suggestion-chip {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 8px; width: fit-content;
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.04);
    cursor: pointer; transition: all 180ms;
    font-family: var(--font-body);
  }
  .suggestion-chip:hover {
    background: rgba(139,92,246,0.06);
    border-color: rgba(139,92,246,0.12);
  }
  .suggestion-label { font-size: 10px; color: #4a4a6a; font-weight: 500; }
  .suggestion-model {
    font-size: 11px; color: #8b8ba7; font-family: var(--font-mono);
    background: none; padding: 0;
  }
  .suggestion-dims {
    font-size: 9px; color: #3a3a5a; font-family: var(--font-mono);
    padding: 1px 5px; border-radius: 4px;
    background: rgba(255,255,255,0.03);
  }

  /* ── Divider ── */
  .divider {
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255,255,255,0.04), transparent);
  }

  /* ── Test Area ── */
  .test-area {
    display: flex; align-items: center; gap: 12px;
    flex-wrap: wrap;
  }
  .btn-test {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 18px; border-radius: 11px;
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.06);
    color: #c0c0d8; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 200ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .btn-test:hover {
    background: rgba(255,255,255,0.06);
    border-color: rgba(255,255,255,0.1);
    transform: translateY(-1px);
  }
  .btn-test:disabled { opacity: 0.3; pointer-events: none; }
  .btn-test.testing { color: var(--accent-soft); }

  .test-badge {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 8px;
    font-size: 12px; font-weight: 600;
    animation: badgePop 250ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes badgePop { from { opacity: 0; transform: scale(0.9); } to { opacity: 1; transform: scale(1); } }
  .test-ok {
    background: rgba(16,185,129,0.08); border: 1px solid rgba(16,185,129,0.12);
    color: #10B981;
  }
  .test-fail {
    background: rgba(244,63,94,0.08); border: 1px solid rgba(244,63,94,0.12);
    color: #F43F5E;
  }
  .test-latency {
    font-size: 10px; color: #6b6b8a; font-family: var(--font-mono);
    margin-left: 2px;
  }

  /* ── Saved Info ── */
  .saved-info {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 14px; border-radius: 10px;
    background: rgba(255,255,255,0.02);
    border: 1px dashed rgba(255,255,255,0.05);
  }
  .saved-label {
    font-size: 10px; color: #4a4a6a; font-weight: 600;
    font-family: var(--font-mono); letter-spacing: 0.3px;
  }
  .saved-model {
    font-size: 12px; color: #a78bfa; font-family: var(--font-mono);
    background: none; padding: 0;
  }

  /* ── Spinner ── */
  .btn-spinner {
    width: 14px; height: 14px; border-radius: 50%;
    border: 2px solid rgba(255,255,255,0.15);
    border-top-color: rgba(255,255,255,0.8);
    animation: spin 600ms linear infinite;
    flex-shrink: 0;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Empty State ── */
  .empty {
    display: flex; flex-direction: column; align-items: center;
    gap: 16px; padding: 100px 20px; text-align: center;
  }
  .empty-visual {
    position: relative;
    width: 80px; height: 80px;
    display: flex; align-items: center; justify-content: center;
  }
  .empty-ring {
    position: absolute; inset: 0; border-radius: 50%;
    border: 1px solid rgba(139,92,246,0.08);
    animation: ringPulse 3s ease-in-out infinite;
  }
  .empty-ring.ring-2 { inset: -8px; animation-delay: -1.5s; border-color: rgba(139,92,246,0.04); }
  .empty-title { font-size: 16px; font-weight: 700; color: #4a4a6a; margin: 0; }
  .empty-desc {
    font-size: 13px; color: #3a3a5a; max-width: 360px; line-height: 1.6;
    margin: 0;
  }
  .empty-link {
    color: #8B5CF6; text-decoration: none; font-weight: 600;
    transition: color 150ms;
  }
  .empty-link:hover { color: #c084fc; text-decoration: underline; }

  /* ── Loading ── */
  .loading-state {
    display: flex; flex-direction: column; gap: 8px;
    max-width: 280px;
  }
  .skel-card {
    padding: 16px; border-radius: 14px;
    background: rgba(10,10,24,0.5);
    border: 1px solid rgba(255,255,255,0.03);
    display: flex; flex-direction: column; gap: 8px;
    animation: cardReveal 400ms cubic-bezier(0.34,1.56,0.64,1) both;
  }
</style>
