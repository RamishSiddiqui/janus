<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── State ──────────────────────────────────────────────────
  interface ProviderRow {
    id: string;
    name: string;
    provider_type: string;
    adapter: string;
    config: Record<string, string>;
    is_default: boolean;
    isConnected?: boolean;
    isTestingConnection?: boolean;
    isExpanded?: boolean;
    latencyMs?: number | null;
  }

  let providers = $state<ProviderRow[]>([]);
  let isLoading = $state(true);
  let showAddForm = $state(false);
  let isSaving = $state(false);

  // Add form fields
  let newName = $state('');
  let newAdapter = $state('open_router');
  let newType = $state('llm');
  let newApiKey = $state('');
  let newBaseUrl = $state('');
  let newModel = $state('');

  // Cloud providers that don't need a base URL
  const cloudAdapters = new Set([
    'open_router', 'anthropic', 'gemini', 'cohere', 'deepseek',
    'groq', 'perplexity', 'xai', 'hugging_face', 'hyperbolic', 'moonshot', 'together',
  ]);
  let adapterNeedsBaseUrl = $derived(!cloudAdapters.has(newAdapter));

  // Edit state per provider
  let editFields = $state<Record<string, { apiKey: string; model: string; baseUrl: string }>>({});

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
        provider_type: p.provider_type,
        adapter: p.adapter,
        config: p.config as Record<string, string>,
        is_default: p.is_default,
        isConnected: undefined,
        isExpanded: p.is_default,
      }));
    } catch (err) { handleIpcError('load providers', err); }
    isLoading = false;
  }

  async function testConnection(p: ProviderRow) {
    if (!isTauri) return;
    p.isTestingConnection = true;
    p.latencyMs = null;
    const t0 = Date.now();
    try {
      const ipc = await import('$lib/services/ipc');
      const ok = await ipc.testProviderConnection(p.id);
      p.isConnected = ok;
      p.latencyMs = ok ? Date.now() - t0 : null;
      success(ok ? `${p.name} connected (${p.latencyMs}ms)` : `${p.name} unreachable`);
    } catch (err) { console.error('[Mythic IPC] Failed to test connection:', err); p.isConnected = false; }
    p.isTestingConnection = false;
    providers = [...providers];
  }

  async function setDefault(p: ProviderRow) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.setDefaultProvider(p.id);
      providers = providers.map(r => ({ ...r, is_default: r.id === p.id }));
      success(`${p.name} set as default`);
    } catch (err) { handleIpcError('set default provider', err); }
  }

  async function deleteProvider(p: ProviderRow) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteProvider(p.id);
      providers = providers.filter(r => r.id !== p.id);
      success(`Deleted ${p.name}`);
    } catch (err) { handleIpcError('delete provider', err); }
  }

  async function saveField(p: ProviderRow, field: string, value: string) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const existing = await ipc.getProvider(p.id);
      const config = { ...(existing.config as Record<string, unknown>), [field]: value };
      await ipc.updateProvider(p.id, undefined, config);
    } catch (err) { handleIpcError('save provider field', err); }
  }

  async function addProvider() {
    if (!newName.trim()) return;
    isSaving = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const config: Record<string, unknown> = {};
      if (newApiKey) config.api_key = newApiKey;
      if (newBaseUrl && adapterNeedsBaseUrl) config.base_url = newBaseUrl;
      if (newModel) config.model = newModel;
      const p = await ipc.createProvider(newName, newType, newAdapter, config, false);
      providers = [...providers, {
        id: p.id, name: p.name, provider_type: p.provider_type,
        adapter: p.adapter, config: p.config as Record<string, string>,
        is_default: p.is_default, isExpanded: true,
      }];
      showAddForm = false;
      newName = ''; newApiKey = ''; newBaseUrl = ''; newModel = '';
      success(`Added ${p.name}`);
    } catch (err) { handleIpcError('add provider', err); }
    isSaving = false;
  }

  function adapterLabel(a: string) {
    const map: Record<string, string> = {
      open_router: 'OpenRouter', ollama: 'Ollama',
      open_ai_compatible: 'OpenAI Compatible', openai_compatible: 'OpenAI Compatible',
      lm_studio: 'LM Studio',
      silicon_flow: 'SiliconFlow', anthropic: 'Anthropic', gemini: 'Gemini',
      cohere: 'Cohere', deepseek: 'DeepSeek', groq: 'Groq',
      perplexity: 'Perplexity', xai: 'xAI', hugging_face: 'HuggingFace',
      hyperbolic: 'Hyperbolic', moonshot: 'Moonshot', together: 'Together',
    };
    return map[a] ?? a;
  }

  function typeColor(t: string) {
    return t === 'llm' ? '#8B5CF6' : t === 'image' ? '#bf40ff' : '#00f2ff';
  }

  /** Returns e.g. "sk-or-••••••••3a1f" from a full API key. */
  function maskApiKey(key: string): string {
    if (!key || key.length < 8) return '••••••••';
    const head = key.slice(0, Math.min(6, Math.floor(key.length / 3)));
    const tail = key.slice(-4);
    return `${head}${'•'.repeat(8)}${tail}`;
  }

  // Tracks which cards are in "edit key" mode
  let keyEditMode = $state<Record<string, boolean>>({});
  function toggleKeyEdit(id: string) {
    keyEditMode = { ...keyEditMode, [id]: !keyEditMode[id] };
  }
</script>

<svelte:head><title>Providers — Mythic</title></svelte:head>

<div class="page">
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title">Providers</h1>
      <span class="hdr-sub">Manage API credentials and connections</span>
    </div>
    <button class="btn-add" onclick={() => showAddForm = !showAddForm} aria-label="Add provider">
      <Icon name={showAddForm ? 'x' : 'plus'} size={13} color="#fff" />
      {showAddForm ? 'Cancel' : 'Add Provider'}
    </button>
  </header>

  <!-- Add Provider Slide-down Form -->
  {#if showAddForm}
    <div class="add-form">
      <div class="add-form-inner">
        <div class="form-row">
          <div class="form-field">
            <label class="flabel" for="pf-name">Name</label>
            <input id="pf-name" class="finput" bind:value={newName} placeholder="My OpenRouter" />
          </div>
          <div class="form-field">
            <label class="flabel" for="pf-type">Type</label>
            <select id="pf-type" class="finput fselect" bind:value={newType}>
              <option value="llm">Chat (LLM)</option>
              <option value="image">Image</option>
              <option value="video">Video</option>
            </select>
          </div>
          <div class="form-field">
            <label class="flabel" for="pf-adapter">Adapter</label>
            <select id="pf-adapter" class="finput fselect" bind:value={newAdapter}>
              <optgroup label="Cloud Providers">
                <option value="open_router">OpenRouter</option>
                <option value="anthropic">Anthropic</option>
                <option value="gemini">Gemini</option>
                <option value="groq">Groq</option>
                <option value="deepseek">DeepSeek</option>
                <option value="perplexity">Perplexity</option>
                <option value="xai">xAI</option>
                <option value="cohere">Cohere</option>
                <option value="together">Together</option>
                <option value="hyperbolic">Hyperbolic</option>
                <option value="moonshot">Moonshot</option>
                <option value="hugging_face">HuggingFace</option>
              </optgroup>
              <optgroup label="Local / Self-hosted">
                <option value="ollama">Ollama</option>
                <option value="lm_studio">LM Studio</option>
                <option value="open_ai_compatible">OpenAI Compatible</option>
              </optgroup>
              <optgroup label="Image">
                <option value="silicon_flow">SiliconFlow</option>
              </optgroup>
            </select>
          </div>
        </div>
        <div class="form-row">
          {#if adapterNeedsBaseUrl}
            <div class="form-field">
              <label class="flabel" for="pf-url">Base URL</label>
              <input id="pf-url" class="finput mono" bind:value={newBaseUrl} placeholder="http://localhost:11434" />
            </div>
          {:else}
            <div class="adapter-hint">
              <span>🔑</span>
              <span>Cloud provider — API key only, no base URL needed.</span>
              {#if newAdapter === 'open_router'}
                <a href="https://openrouter.ai/keys" target="_blank" class="hint-link">Get key →</a>
              {:else if newAdapter === 'anthropic'}
                <a href="https://console.anthropic.com/account/keys" target="_blank" class="hint-link">Get key →</a>
              {:else if newAdapter === 'gemini'}
                <a href="https://aistudio.google.com/apikey" target="_blank" class="hint-link">Get key →</a>
              {:else if newAdapter === 'groq'}
                <a href="https://console.groq.com/keys" target="_blank" class="hint-link">Get key →</a>
              {/if}
            </div>
          {/if}
          <div class="form-field">
            <label class="flabel" for="pf-key">API Key</label>
            <input id="pf-key" class="finput mono" type="password" bind:value={newApiKey} placeholder="sk-or-..." />
          </div>
          <div class="form-field">
            <label class="flabel" for="pf-model">Default Model</label>
            <input id="pf-model" class="finput mono" bind:value={newModel}
              placeholder={newAdapter === 'open_router' ? 'anthropic/claude-3.5-sonnet' : 'model-name'} />
          </div>
        </div>
        <div class="form-actions">
          <button class="btn-add" onclick={addProvider} disabled={isSaving || !newName.trim()}>
            {isSaving ? 'Adding…' : 'Add Provider'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Provider List -->
  <div class="provider-list">
    {#if isLoading}
      {#each Array(3) as _}
        <div class="pcard skeleton-card">
          <Skeleton variant="text" width="40%" height="14px" />
          <Skeleton variant="text" width="60%" height="11px" />
        </div>
      {/each}
    {:else if providers.length === 0}
      <div class="empty-state">
        <div class="empty-icon">⚡</div>
        <span class="empty-title">No providers configured</span>
        <span class="empty-sub">Add your first AI provider to start chatting</span>
        <button class="btn-add" onclick={() => showAddForm = true}>Add Provider</button>
      </div>
    {:else}
      {#each providers as p (p.id)}
        <div class="pcard" class:pcard-default={p.is_default}>
          <!-- Card Header -->
          <div class="pcard-hdr">
            <div class="pcard-hdr-left">
              <span class="status-dot"
                class:dot-connected={p.isConnected === true}
                class:dot-failed={p.isConnected === false}
                class:dot-pulse={p.isTestingConnection}>
              </span>
              <div class="pcard-name-wrap">
                <span class="pcard-name">{p.name}</span>
                <div class="pcard-badges">
                  <span class="badge badge-adapter">{adapterLabel(p.adapter)}</span>
                  <span class="badge" style="color:{typeColor(p.provider_type)};background:color-mix(in srgb,{typeColor(p.provider_type)} 12%,transparent)">
                    {p.provider_type.toUpperCase()}
                  </span>
                  {#if p.is_default}<span class="badge badge-default">Default</span>{/if}
                </div>
              </div>
            </div>
            <div class="pcard-hdr-right">
              {#if p.latencyMs != null}
                <span class="latency">{p.latencyMs}ms</span>
              {/if}
              {#if p.isConnected === true}
                <span class="conn-ok">● Connected</span>
              {:else if p.isConnected === false}
                <span class="conn-fail">● Failed</span>
              {/if}
              <button class="icon-btn" onclick={() => { p.isExpanded = !p.isExpanded; providers = [...providers]; }}
                aria-label="Toggle details">
                <Icon name={p.isExpanded ? 'chevron-up' : 'chevron-down'} size={14} color="#5a5a7a" />
              </button>
            </div>
          </div>

          <!-- Expanded Detail -->
          {#if p.isExpanded}
            <div class="pcard-body">
              <div class="pfield-row">

                <!-- Model field -->
                <div class="pfield">
                  <span class="pflabel">Default Model</span>
                  {#if p.config.model}
                    <input class="pfinput mono" value={p.config.model}
                      onblur={(e) => { const v = e.currentTarget.value; p.config.model = v; providers = [...providers]; saveField(p, 'model', v); }} />
                  {:else}
                    <div class="field-empty-wrap">
                      <span class="field-empty-chip">No model set</span>
                      <input class="pfinput mono field-empty-input" placeholder="Enter model ID…"
                        onblur={(e) => { const v = e.currentTarget.value.trim(); if (v) { p.config.model = v; providers = [...providers]; saveField(p, 'model', v); } }} />
                    </div>
                  {/if}
                </div>

                <!-- API Key field -->
                <div class="pfield">
                  <div class="pflabel-row">
                    <span class="pflabel">API Key</span>
                    {#if p.config.api_key}
                      <button class="key-edit-btn" onclick={() => toggleKeyEdit(p.id)}>
                        {keyEditMode[p.id] ? 'Cancel' : 'Change'}
                      </button>
                    {/if}
                  </div>
                  {#if p.config.api_key && !keyEditMode[p.id]}
                    <div class="key-masked">
                      <span class="key-mask-icon">🔑</span>
                      <span class="key-mask-text mono">{maskApiKey(p.config.api_key)}</span>
                      <span class="key-set-badge">Set</span>
                    </div>
                  {:else}
                    <input class="pfinput mono" type="password"
                      placeholder={p.config.api_key ? 'Enter new key to replace…' : 'sk-or-…'}
                      onblur={(e) => {
                        const v = e.currentTarget.value.trim();
                        if (v) { p.config.api_key = v; providers = [...providers]; saveField(p, 'api_key', v); keyEditMode = { ...keyEditMode, [p.id]: false }; }
                      }} />
                  {/if}
                </div>

              </div>
              {#if p.adapter !== 'open_router'}
                <div class="pfield">
                  <span class="pflabel">Base URL</span>
                  <input class="pfinput mono" value={p.config.base_url ?? ''} placeholder="http://..."
                    onblur={(e) => saveField(p, 'base_url', e.currentTarget.value)} />
                </div>
              {/if}

              <!-- Embedder Config -->
              <div class="embedder-section">
                <div class="embedder-header">
                  <Icon name="cpu" size={12} color="#a78bfa" />
                  <span class="embedder-title">Embedder</span>
                  <span class="embedder-hint">Model used for semantic memory indexing</span>
                </div>
                <div class="pfield">
                  <span class="pflabel">Embedding Model</span>
                  {#if p.config.embedding_model}
                    <input class="pfinput mono" value={p.config.embedding_model}
                      onblur={(e) => {
                        const v = e.currentTarget.value;
                        p.config.embedding_model = v;
                        providers = [...providers];
                        saveField(p, 'embedding_model', v);
                      }} />
                  {:else}
                    <div class="field-empty-wrap">
                      <span class="field-empty-chip">No embedder set</span>
                      <input class="pfinput mono field-empty-input"
                        placeholder={p.adapter === 'open_router' ? 'openai/text-embedding-3-small' : p.adapter === 'ollama' ? 'nomic-embed-text' : 'text-embedding-3-small'}
                        onblur={(e) => {
                          const v = e.currentTarget.value.trim();
                          if (v) {
                            p.config.embedding_model = v;
                            providers = [...providers];
                            saveField(p, 'embedding_model', v);
                          }
                        }} />
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          {/if}

          <!-- Card Actions -->
          <div class="pcard-actions">
            <button class="act-btn" class:act-testing={p.isTestingConnection}
              onclick={() => testConnection(p)} disabled={p.isTestingConnection}>
              {p.isTestingConnection ? 'Testing…' : 'Test'}
            </button>
            {#if !p.is_default}
              <button class="act-btn" onclick={() => setDefault(p)}>Set Default</button>
            {/if}
            <button class="act-btn act-danger" onclick={() => deleteProvider(p)}>Delete</button>
          </div>
        </div>
      {/each}
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
    font-size: 22px; font-weight: 800; letter-spacing: -0.5px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
    margin: 0;
  }
  .hdr-sub { font-size: 12px; color: #4a4a6a; }

  .btn-add {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px; border: none; cursor: pointer;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); color: #fff;
    font-weight: 600; font-size: 13px; font-family: var(--font-body);
    box-shadow: 0 2px 12px rgba(139,92,246,0.3);
    transition: all 180ms ease;
  }
  .btn-add:hover { transform: translateY(-1px); box-shadow: 0 4px 20px rgba(139,92,246,0.45); }
  .btn-add:disabled { opacity: 0.5; pointer-events: none; }

  /* Add form */
  .add-form {
    margin: 0 28px 16px; border-radius: 14px;
    background: rgba(14,14,30,0.7); border: 1px solid rgba(139,92,246,0.12);
    animation: slideDown 220ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-12px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .add-form-inner { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
  .form-row { display: flex; gap: 12px; flex-wrap: wrap; }
  .form-field { display: flex; flex-direction: column; gap: 5px; flex: 1; min-width: 160px; }
  .flabel { font-size: 10px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; color: #4a4a6a; font-family: var(--font-mono); }
  .finput {
    height: 36px; padding: 0 12px; border-radius: 9px;
    background: rgba(10,10,24,0.8); border: 1px solid rgba(139,92,246,0.1);
    color: #e0e0f0; font-size: 13px; font-family: var(--font-body); outline: none;
    transition: border-color 180ms;
  }
  .finput:focus { border-color: rgba(139,92,246,0.4); }
  .finput.mono { font-family: var(--font-mono); }
  .fselect { appearance: none; cursor: pointer;
    background-image: url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b6b8a' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e");
    background-position: right 8px center; background-repeat: no-repeat; background-size: 16px; padding-right: 28px;
  }
  .adapter-hint {
    display: flex; align-items: center; gap: 8px; flex: 1;
    padding: 9px 12px; border-radius: 9px; min-width: 160px;
    background: rgba(139,92,246,0.06); border: 1px solid rgba(139,92,246,0.1);
    font-size: 12px; color: #8b8ba7;
  }
  .hint-link { color: #a78bfa; font-weight: 600; text-decoration: none; margin-left: auto; }
  .form-actions { display: flex; justify-content: flex-end; }

  /* Provider list */
  .provider-list {
    flex: 1; overflow-y: auto; padding: 16px 28px 28px;
    display: flex; flex-direction: column; gap: 12px;
  }
  .provider-list::-webkit-scrollbar { width: 4px; }
  .provider-list::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  /* Provider card */
  .pcard {
    border-radius: 14px; padding: 16px 18px;
    background: rgba(12,12,26,0.6);
    border: 1px solid rgba(139,92,246,0.07);
    display: flex; flex-direction: column; gap: 12px;
    transition: border-color 200ms, box-shadow 200ms;
    animation: cardIn 240ms ease both;
  }
  @keyframes cardIn { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }
  .pcard:hover { border-color: rgba(139,92,246,0.14); box-shadow: 0 6px 28px rgba(0,0,0,0.25); }
  .pcard-default { border-color: rgba(139,92,246,0.22) !important; box-shadow: 0 0 20px rgba(139,92,246,0.08); }
  .skeleton-card { gap: 8px; }

  .pcard-hdr { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .pcard-hdr-left { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; }
  .pcard-hdr-right { display: flex; align-items: center; gap: 10px; flex-shrink: 0; }

  .status-dot {
    width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0;
    background: rgba(60,60,90,0.6); transition: background 300ms, box-shadow 300ms;
  }
  .dot-connected { background: #10B981; box-shadow: 0 0 8px rgba(16,185,129,0.5); }
  .dot-failed { background: #F43F5E; box-shadow: 0 0 6px rgba(244,63,94,0.4); }
  .dot-pulse { animation: dotPulse 900ms ease-in-out infinite; }
  @keyframes dotPulse { 0%,100% { opacity: 1; } 50% { opacity: 0.3; } }

  .pcard-name-wrap { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .pcard-name { font-size: 14px; font-weight: 600; color: #d0d0e8; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pcard-badges { display: flex; gap: 6px; flex-wrap: wrap; }

  .badge {
    padding: 2px 8px; border-radius: 99px; font-size: 10px; font-weight: 700;
    letter-spacing: 0.3px;
  }
  .badge-adapter { background: rgba(139,92,246,0.12); color: #9d7af5; }
  .badge-default { background: rgba(16,185,129,0.12); color: #10B981; }

  .latency { font-size: 11px; font-family: var(--font-mono); color: #10B981; }
  .conn-ok { font-size: 11px; color: #10B981; font-weight: 600; }
  .conn-fail { font-size: 11px; color: #F43F5E; font-weight: 600; }

  .icon-btn {
    width: 28px; height: 28px; border-radius: 8px; border: 1px solid rgba(139,92,246,0.08);
    background: transparent; cursor: pointer; display: flex; align-items: center; justify-content: center;
    transition: all 150ms;
  }
  .icon-btn:hover { background: rgba(139,92,246,0.08); }

  .pcard-body { display: flex; flex-direction: column; gap: 10px; padding-top: 4px; border-top: 1px solid rgba(139,92,246,0.06); }
  .pfield-row { display: flex; gap: 12px; }
  .pfield { display: flex; flex-direction: column; gap: 5px; flex: 1; }
  .pflabel { font-size: 10px; font-weight: 700; letter-spacing: 1px; text-transform: uppercase; color: #4a4a6a; font-family: var(--font-mono); }
  .pfinput {
    height: 34px; padding: 0 10px; border-radius: 8px;
    background: rgba(10,10,22,0.7); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 12px; font-family: var(--font-body); outline: none;
    transition: border-color 180ms;
  }
  .pfinput:focus { border-color: rgba(139,92,246,0.35); }
  .pfinput.mono { font-family: var(--font-mono); }

  .pcard-actions { display: flex; gap: 6px; }
  .act-btn {
    padding: 5px 13px; border-radius: 8px; border: 1px solid rgba(139,92,246,0.1);
    background: transparent; color: #6b6b8a; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer; transition: all 150ms;
  }
  .act-btn:hover { background: rgba(139,92,246,0.08); color: #e0e0f0; border-color: rgba(139,92,246,0.2); }
  .act-btn:disabled { opacity: 0.5; pointer-events: none; }
  .act-testing { color: #a78bfa; }
  .act-danger { color: #F43F5E; border-color: rgba(244,63,94,0.15); }
  .act-danger:hover { background: rgba(244,63,94,0.08); }

  /* Empty state */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    padding: 64px 20px; text-align: center;
  }
  .empty-icon { font-size: 36px; opacity: 0.4; }
  .empty-title { font-size: 15px; font-weight: 700; color: #6b6b8a; }
  .empty-sub { font-size: 13px; color: #4a4a6a; }

  /* ── Smart field states ── */

  /* "No model set" display */
  .field-empty-wrap {
    display: flex; flex-direction: column; gap: 6px;
  }
  .field-empty-chip {
    display: inline-flex; align-items: center;
    padding: 5px 10px; border-radius: 7px; width: fit-content;
    background: rgba(74,74,106,0.12); border: 1px dashed rgba(74,74,106,0.25);
    color: #4a4a6a; font-size: 11px; font-style: italic; letter-spacing: 0.2px;
  }
  .field-empty-input {
    height: 30px !important; font-size: 11px !important;
    border-style: dashed !important;
  }

  /* API Key masked display */
  .pflabel-row {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
  }
  .key-edit-btn {
    font-size: 10px; font-weight: 700; font-family: var(--font-mono);
    color: #6b5cf6; background: none; border: none; cursor: pointer; padding: 0;
    letter-spacing: 0.3px; transition: color 150ms;
  }
  .key-edit-btn:hover { color: #c4a1ff; }

  .key-masked {
    display: flex; align-items: center; gap: 8px;
    height: 34px; padding: 0 10px; border-radius: 8px;
    background: rgba(10,10,22,0.5); border: 1px solid rgba(139,92,246,0.08);
  }
  .key-mask-icon { font-size: 13px; flex-shrink: 0; }
  .key-mask-text {
    flex: 1; font-size: 12px; color: #6b6b8a; letter-spacing: 1px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .key-set-badge {
    flex-shrink: 0; padding: 2px 7px; border-radius: 99px;
    background: rgba(16,185,129,0.1); border: 1px solid rgba(16,185,129,0.2);
    color: #10B981; font-size: 10px; font-weight: 700; font-family: var(--font-mono);
  }

  /* ── Embedder Section ── */
  .embedder-section {
    display: flex; flex-direction: column; gap: 10px;
    padding: 14px 16px; margin-top: 4px; border-radius: 10px;
    background: rgba(139,92,246,0.03);
    border: 1px solid rgba(139,92,246,0.06);
    border-left: 2px solid rgba(139,92,246,0.2);
  }
  .embedder-header {
    display: flex; align-items: center; gap: 8px;
  }
  .embedder-title {
    font-size: 11px; font-weight: 700; letter-spacing: 0.8px;
    text-transform: uppercase; color: #a78bfa;
    font-family: var(--font-mono);
  }
  .embedder-hint {
    font-size: 10px; color: #4a4a6a; margin-left: auto;
  }
</style>
