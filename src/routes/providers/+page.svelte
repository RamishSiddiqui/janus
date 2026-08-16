<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { handleIpcError } from '$lib/utils/error';
  import { humanizeProviderError } from '$lib/utils/providerError';
  import { HORDE_SAMPLERS } from '$lib/constants/aiHorde';

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

  // Per-provider inline JSON validation errors for the ComfyUI workflow
  // textarea in the expanded/existing-provider card, keyed by provider id.
  let comfyWorkflowErrors = $state<Record<string, string>>({});

  // AI Horde generation settings — only shown/used when newAdapter === 'ai_horde'.
  // Defaults match the researched AI Horde API defaults, with `karras: true`
  // overriding the API's bare default of `false` per community consensus for
  // smoother results at the same step count.
  let newHordeSampler = $state('k_euler_a');
  let newHordeCfgScale = $state(7.5);
  let newHordeSteps = $state(30);
  let newHordeKarras = $state(true);
  // A named Horde style (browsable at artbot.site) overrides sampler/model/
  // resolution entirely — leave blank to use the manual settings above.
  let newHordeStyle = $state('');
  // Only used when no style is set — left blank to use the built-in
  // researched default (blurry/bad-anatomy/watermark/etc. avoidance).
  let newHordeNegativePrompt = $state('');

  // ComfyUI workflow — only shown/used when newAdapter === 'comfy_ui'. The
  // user's own exported (API-format) workflow JSON, with placeholder tokens
  // ({{POSITIVE_PROMPT}}, {{SEED}}, {{CHARACTER_IMAGE_n}}, ...) dropped into
  // whichever node fields they want Janus to fill in dynamically — see
  // `providers::comfyui` on the backend for the full token contract.
  let newComfyWorkflow = $state('');
  let newComfyWorkflowError = $state('');

  // Cloud providers that don't need a base URL — AI Horde has one fixed,
  // well-known endpoint (aihorde.net), so it's treated the same way.
  const cloudAdapters = new Set([
    'open_router', 'anthropic', 'gemini', 'cohere', 'deepseek',
    'groq', 'perplexity', 'xai', 'hugging_face', 'hyperbolic', 'moonshot', 'together',
    'ai_horde',
  ]);
  let adapterNeedsBaseUrl = $derived(!cloudAdapters.has(newAdapter));

  // Pre-fill an excellent default on switching to AI Horde — the anonymous
  // key works with zero registration — but never clobber something the user
  // already typed. Default Model is deliberately left untouched here: it
  // only shows "Deliberate" (the most-served model, fastest turnaround) as a
  // *placeholder suggestion*, not a real committed value — auto-filling it
  // for real made an unset field indistinguishable from a chosen one, so a
  // model that was never actually saved as default looked like it was.
  $effect(() => {
    if (newAdapter === 'ai_horde') {
      if (!newApiKey) newApiKey = '0000000000';
      newType = 'image';
    } else if (newAdapter === 'comfy_ui') {
      if (!newBaseUrl) newBaseUrl = 'http://localhost:8188';
      newType = 'image';
    } else if (newAdapter === 'wan_gp') {
      // Serves both image and video off the same MCP server — leave Type
      // as whatever the user picked, since they'll add two provider rows
      // (one per type) pointing at this same base URL.
      if (!newBaseUrl) newBaseUrl = 'http://127.0.0.1:7866';
    } else if (newAdapter === 'puter') {
      // Puter (puter.com) speaks the OpenAI-compatible wire format, so it
      // rides the existing open_ai_compatible adapter with zero backend
      // changes — see the "puter" -> "open_ai_compatible" normalization in
      // addProvider(). Auth is a personal token from the user's own
      // dashboard (puter.com/dashboard#account -> Account -> Create token),
      // not an account password — unlike the unofficial reverse-engineered
      // login flow some third-party wrappers use.
      if (!newBaseUrl) newBaseUrl = 'https://api.puter.com/puterai/openai/v1/';
      newType = 'text';
    }
  });

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
      const result = await ipc.testProviderConnection(p.id);
      p.isConnected = result.ok;
      p.latencyMs = result.ok ? Date.now() - t0 : null;
      if (result.ok) {
        success(`${p.name} connected (${p.latencyMs}ms)`);
      } else {
        // A bare "unreachable" collapses several very different problems
        // (bad key, wrong Base URL, the server genuinely being down,
        // a timeout...) into one useless word — show the actual reason
        // the backend determined instead.
        const reason = result.detail ? humanizeProviderError(result.detail) : 'Connection failed.';
        toastError(`${p.name}: ${reason}`);
      }
    } catch (err) { console.error('[Janus IPC] Failed to test connection:', err); p.isConnected = false; }
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
    if (newAdapter === 'comfy_ui' && newComfyWorkflow.trim()) {
      try {
        JSON.parse(newComfyWorkflow);
        newComfyWorkflowError = '';
      } catch (e) {
        newComfyWorkflowError = `Not valid JSON: ${(e as Error).message}`;
        return;
      }
    }
    isSaving = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const config: Record<string, unknown> = {};
      if (newApiKey) config.api_key = newApiKey;
      if (newBaseUrl && adapterNeedsBaseUrl) config.base_url = newBaseUrl;
      if (newModel) config.model = newModel;
      if (newAdapter === 'ai_horde') {
        config.sampler_name = newHordeSampler;
        config.cfg_scale = String(newHordeCfgScale);
        config.steps = String(newHordeSteps);
        config.karras = String(newHordeKarras);
        if (newHordeStyle) config.style = newHordeStyle;
        if (newHordeNegativePrompt) config.negative_prompt = newHordeNegativePrompt;
      }
      if (newAdapter === 'comfy_ui' && newComfyWorkflow.trim()) {
        config.workflow = newComfyWorkflow;
      }
      // "puter" is a UI-only preset — Puter speaks the OpenAI-compatible
      // wire format, so it's stored as the real open_ai_compatible adapter
      // (see the matching comment in the prefill $effect above).
      const actualAdapter = newAdapter === 'puter' ? 'open_ai_compatible' : newAdapter;
      const p = await ipc.createProvider(newName, newType, actualAdapter, config, false);
      providers = [...providers, {
        id: p.id, name: p.name, provider_type: p.provider_type,
        adapter: p.adapter, config: p.config as Record<string, string>,
        is_default: p.is_default, isExpanded: true,
      }];
      showAddForm = false;
      newName = ''; newApiKey = ''; newBaseUrl = ''; newModel = '';
      newHordeSampler = 'k_euler_a'; newHordeCfgScale = 7.5; newHordeSteps = 30; newHordeKarras = true;
      newHordeStyle = ''; newHordeNegativePrompt = '';
      newComfyWorkflow = ''; newComfyWorkflowError = '';
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
      ai_horde: 'AI Horde', comfy_ui: 'ComfyUI', wan_gp: 'WanGP',
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

<svelte:head><title>Providers — Janus</title></svelte:head>

<div class="page">
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title"><SplitHeading text="Providers" /></h1>
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
                <option value="puter">Puter (free)</option>
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
                <option value="ai_horde">AI Horde (free)</option>
                <option value="comfy_ui">ComfyUI</option>
                <option value="wan_gp">WanGP</option>
              </optgroup>
              <optgroup label="Video">
                <option value="wan_gp">WanGP</option>
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
            {#if newAdapter === 'puter'}
              <div class="adapter-hint">
                <span>🌐</span>
                <span>Free access to 500+ models. Generate a personal token from your own Puter account (not your password) — paste it as the API key below.</span>
                <a href="https://puter.com/dashboard#account" target="_blank" class="hint-link">Create token →</a>
              </div>
            {/if}
          {:else if newAdapter === 'ai_horde'}
            <div class="adapter-hint">
              <span>🌐</span>
              <span>Free, crowdsourced generation. The default "0000000000" key works with zero signup (lowest priority) — register at aihorde.net for faster queueing.</span>
              <a href="https://aihorde.net/register" target="_blank" class="hint-link">Register →</a>
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
          {#if newType !== 'llm'}
            <!-- LLM providers pick their active model on the Models page
                 (the enabled_models table) — a free-text default here was
                 disconnected from that and a recurring source of "wrong
                 model silently used" bugs. Image/video providers have no
                 equivalent picker yet, so they still need it. -->
            <div class="form-field">
              <label class="flabel" for="pf-model">Default Model</label>
              <input id="pf-model" class="finput mono" bind:value={newModel}
                placeholder={newAdapter === 'ai_horde' ? 'Deliberate' : newAdapter === 'wan_gp' ? 'qwen_image_20B' : 'model-name'} />
              <span class="field-hint">
                {#if newModel}
                  Will use <strong>{newModel}</strong> for every generation.
                {:else if newAdapter === 'ai_horde'}
                  Left blank — a live model will be auto-selected each generation instead (may vary).
                {:else}
                  Leave blank to use the provider's own default.
                {/if}
              </span>
            </div>
          {/if}
        </div>
        {#if newAdapter === 'ai_horde'}
          <div class="form-row">
            <div class="form-field">
              <label class="flabel" for="pf-horde-sampler">Sampler</label>
              <select id="pf-horde-sampler" class="finput fselect" bind:value={newHordeSampler}>
                {#each HORDE_SAMPLERS as s}<option value={s}>{s}</option>{/each}
              </select>
            </div>
            <div class="form-field">
              <label class="flabel" for="pf-horde-cfg">CFG Scale</label>
              <input id="pf-horde-cfg" class="finput mono" type="number" step="0.5" min="1" max="30" bind:value={newHordeCfgScale} />
            </div>
            <div class="form-field">
              <label class="flabel" for="pf-horde-steps">Steps</label>
              <input id="pf-horde-steps" class="finput mono" type="number" step="1" min="1" max="150" bind:value={newHordeSteps} />
            </div>
            <div class="form-field form-field-checkbox">
              <label class="flabel" for="pf-horde-karras">Karras</label>
              <label class="checkbox-wrap">
                <input id="pf-horde-karras" type="checkbox" bind:checked={newHordeKarras} />
                <span class="checkbox-hint">Smoother noise schedule</span>
              </label>
            </div>
          </div>
          <div class="form-row">
            <div class="form-field">
              <label class="flabel" for="pf-horde-style">Style (optional)</label>
              <input id="pf-horde-style" class="finput mono" bind:value={newHordeStyle}
                placeholder="e.g. raw-png, pixel-art — overrides sampler/model/resolution above" />
            </div>
          </div>
          <div class="adapter-hint">
            <span>🎨</span>
            <span>A named Horde style bundles a curated prompt, model, sampler and resolution for a specific look. Leave blank to use the manual settings above.</span>
            <a href="https://artbot.site/" target="_blank" class="hint-link">Browse styles →</a>
          </div>
          <div class="form-row">
            <div class="form-field">
              <label class="flabel" for="pf-horde-negative">Negative Prompt (optional, ignored if a style is set)</label>
              <input id="pf-horde-negative" class="finput mono" bind:value={newHordeNegativePrompt}
                placeholder="Leave blank to use the built-in default (blurry, bad anatomy, watermark, …)" />
            </div>
          </div>
        {/if}
        {#if newAdapter === 'comfy_ui'}
          <div class="form-row">
            <div class="form-field form-field-full">
              <label class="flabel" for="pf-comfy-workflow">Workflow (API format JSON)</label>
              <textarea id="pf-comfy-workflow" class="finput mono comfy-workflow-textarea" rows="8"
                bind:value={newComfyWorkflow}
                oninput={() => (newComfyWorkflowError = '')}
                placeholder={'{ "3": { "class_type": "KSampler", "inputs": { "seed": "{{SEED}}", ... } }, ... }'}
              ></textarea>
              {#if newComfyWorkflowError}
                <span class="field-error">{newComfyWorkflowError}</span>
              {/if}
            </div>
          </div>
          <div class="adapter-hint comfy-hint">
            <span>🧩</span>
            <span>
              Export your workflow from ComfyUI with <strong>"Save (API Format)"</strong>, then replace whichever
              node values you want filled in dynamically with one of these tokens:
              <code>{'{{POSITIVE_PROMPT}}'}</code>, <code>{'{{NEGATIVE_PROMPT}}'}</code>, <code>{'{{SEED}}'}</code>,
              <code>{'{{WIDTH}}'}</code>, <code>{'{{HEIGHT}}'}</code>, and <code>{'{{CHARACTER_IMAGE_1}}'}</code>,
              <code>{'{{CHARACTER_IMAGE_2}}'}</code>… (one per <code>LoadImage</code> node you want a cast portrait
              sent to). All tokens are optional — only add the ones your workflow actually needs.
            </span>
          </div>
        {/if}
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

                {#if p.provider_type !== 'llm'}
                  <!-- LLM providers pick their active model on the Models
                       page instead — see the matching comment on the Add
                       Provider form. -->
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
                      {#if p.adapter === 'ai_horde'}
                        <span class="field-hint">Every generation will auto-select a live model instead — set one above for consistent results.</span>
                      {/if}
                    {/if}
                  </div>
                {/if}

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
              {#if p.adapter !== 'open_router' && p.adapter !== 'ai_horde'}
                <div class="pfield">
                  <span class="pflabel">Base URL</span>
                  <input class="pfinput mono" value={p.config.base_url ?? ''} placeholder="http://..."
                    onblur={(e) => saveField(p, 'base_url', e.currentTarget.value)} />
                </div>
              {/if}

              {#if p.adapter === 'ai_horde'}
                <!-- AI Horde Generation Settings -->
                <div class="embedder-section">
                  <div class="embedder-header">
                    <Icon name="cpu" size={12} color="#a78bfa" />
                    <span class="embedder-title">AI Horde Generation Settings</span>
                    <span class="embedder-hint">Tunables sent with every generation request</span>
                  </div>
                  <div class="pfield-row">
                    <div class="pfield">
                      <span class="pflabel">Sampler</span>
                      <select class="pfinput mono fselect" value={p.config.sampler_name ?? 'k_euler_a'}
                        onchange={(e) => { const v = e.currentTarget.value; p.config.sampler_name = v; providers = [...providers]; saveField(p, 'sampler_name', v); }}>
                        {#each HORDE_SAMPLERS as s}<option value={s}>{s}</option>{/each}
                      </select>
                    </div>
                    <div class="pfield">
                      <span class="pflabel">CFG Scale</span>
                      <input class="pfinput mono" type="number" step="0.5" min="1" max="30" value={p.config.cfg_scale ?? '7.5'}
                        onblur={(e) => { const v = e.currentTarget.value; p.config.cfg_scale = v; providers = [...providers]; saveField(p, 'cfg_scale', v); }} />
                    </div>
                  </div>
                  <div class="pfield-row">
                    <div class="pfield">
                      <span class="pflabel">Steps</span>
                      <input class="pfinput mono" type="number" step="1" min="1" max="150" value={p.config.steps ?? '30'}
                        onblur={(e) => { const v = e.currentTarget.value; p.config.steps = v; providers = [...providers]; saveField(p, 'steps', v); }} />
                    </div>
                    <div class="pfield">
                      <span class="pflabel">Karras</span>
                      <label class="checkbox-wrap">
                        <input type="checkbox" checked={(p.config.karras ?? 'true') === 'true'}
                          onchange={(e) => { const v = String(e.currentTarget.checked); p.config.karras = v; providers = [...providers]; saveField(p, 'karras', v); }} />
                        <span class="checkbox-hint">Smoother noise schedule</span>
                      </label>
                    </div>
                  </div>
                  <div class="pfield">
                    <span class="pflabel">Style (optional — overrides sampler/model/resolution above)</span>
                    <input class="pfinput mono" value={p.config.style ?? ''} placeholder="e.g. raw-png, pixel-art"
                      onblur={(e) => { const v = e.currentTarget.value.trim(); p.config.style = v; providers = [...providers]; saveField(p, 'style', v); }} />
                    <a href="https://artbot.site/" target="_blank" class="hint-link" style="margin-top:2px;">Browse styles →</a>
                  </div>
                  <div class="pfield">
                    <span class="pflabel">Negative Prompt (optional, ignored if a style is set)</span>
                    <input class="pfinput mono" value={p.config.negative_prompt ?? ''}
                      placeholder="Leave blank for the built-in default"
                      onblur={(e) => { const v = e.currentTarget.value; p.config.negative_prompt = v; providers = [...providers]; saveField(p, 'negative_prompt', v); }} />
                  </div>
                </div>
              {/if}

              {#if p.adapter === 'comfy_ui'}
                <!-- ComfyUI Workflow Settings -->
                <div class="embedder-section">
                  <div class="embedder-header">
                    <Icon name="cpu" size={12} color="#a78bfa" />
                    <span class="embedder-title">ComfyUI Workflow</span>
                    <span class="embedder-hint">Your exported (API format) workflow JSON</span>
                  </div>
                  <div class="pfield">
                    <span class="pflabel">Workflow (API format JSON)</span>
                    <textarea class="pfinput mono comfy-workflow-textarea" rows="8"
                      value={p.config.workflow ?? ''}
                      onblur={(e) => {
                        const v = e.currentTarget.value;
                        try {
                          if (v.trim()) JSON.parse(v);
                          p.config.workflow = v; providers = [...providers];
                          saveField(p, 'workflow', v);
                          comfyWorkflowErrors = { ...comfyWorkflowErrors, [p.id]: '' };
                        } catch (err) {
                          comfyWorkflowErrors = { ...comfyWorkflowErrors, [p.id]: `Not valid JSON: ${(err as Error).message}` };
                        }
                      }}
                    ></textarea>
                    {#if comfyWorkflowErrors[p.id]}
                      <span class="field-error">{comfyWorkflowErrors[p.id]}</span>
                    {/if}
                  </div>
                  <div class="adapter-hint comfy-hint">
                    <span>🧩</span>
                    <span>
                      Placeholder tokens: <code>{'{{POSITIVE_PROMPT}}'}</code>, <code>{'{{NEGATIVE_PROMPT}}'}</code>,
                      <code>{'{{SEED}}'}</code>, <code>{'{{WIDTH}}'}</code>, <code>{'{{HEIGHT}}'}</code>,
                      <code>{'{{CHARACTER_IMAGE_1}}'}</code>, <code>{'{{CHARACTER_IMAGE_2}}'}</code>…
                    </span>
                  </div>
                </div>
              {/if}

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
    font-size: 24px; font-weight: 600; letter-spacing: -0.5px;
    margin: 0;
  }
  .hdr-sub { font-size: 13px; color: #4a4a6a; }

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
  .field-hint {
    font-size: 11px; line-height: 1.5; color: #6b6b8a;
  }
  .field-hint strong { color: #c4a1ff; font-weight: 600; }
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
  .hint-link { color: #a78bfa; font-weight: 600; text-decoration: none; margin-left: auto; white-space: nowrap; }
  .form-actions { display: flex; justify-content: flex-end; }

  .form-field-checkbox { justify-content: flex-end; }
  .checkbox-wrap {
    display: flex; align-items: center; gap: 8px; height: 36px;
    font-size: 11px; color: #6b6b8a; cursor: pointer;
  }
  .checkbox-wrap input[type="checkbox"] { accent-color: #8B5CF6; width: 14px; height: 14px; cursor: pointer; }

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

  /* ── ComfyUI workflow textarea ── */
  .form-field-full { flex-basis: 100%; }
  .comfy-workflow-textarea {
    height: auto !important; min-height: 140px; padding: 10px 12px;
    resize: vertical; line-height: 1.5; white-space: pre;
  }
  .field-error {
    font-size: 11px; color: #F43F5E; font-family: var(--font-mono);
  }
  .comfy-hint code {
    background: rgba(139,92,246,0.1); border-radius: 4px; padding: 1px 5px;
    font-family: var(--font-mono); color: #c4a1ff; font-size: 11px;
  }
</style>
