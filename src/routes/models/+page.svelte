<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { ProviderConfig } from '$lib/types';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Chat providers
  let chatProviders: ProviderConfig[] = $state([]);

  let chatTemp = $state('0.80');
  let chatMaxTokens = $state('2048');
  let isLoadingProviders = $state(true);

  // Image providers
  let imgProviders: ProviderConfig[] = $state([]);

  let imgWidth = $state('1024');
  let imgHeight = $state('1024');
  let imgSteps = $state('20');
  let imgGuidance = $state('7.5');

  // Video providers
  let vidProviders: ProviderConfig[] = $state([]);

  let vidDuration = $state('5');
  let vidFps = $state('24');
  let vidResolution = $state('720p (1280×720)');

  // Backend provider IDs for test connection
  let providerIds: Map<string, string> = $state(new Map());

  onMount(async () => {
    if (!isTauri) {
      isLoadingProviders = false;
      return;
    }

    try {
      const ipc = await import('$lib/services/ipc');
      const providers = await ipc.listProviders();

      // Map backend providers to display format
      const llmProviders = providers.filter(p => p.provider_type === 'llm');
      const imageProviders = providers.filter(p => p.provider_type === 'image');
      const videoProviders = providers.filter(p => p.provider_type === 'video');

      if (llmProviders.length > 0) {
        chatProviders = llmProviders.map(p => {
          providerIds.set(p.name, p.id);
          const config = p.config as Record<string, string>;
          return {
            name: p.name,
            model: config.model || '',
            apiKey: config.api_key ? `${config.api_key.slice(0, 6)}••••••••••••` : undefined,
            isActive: p.is_default,
            isConnected: true,
            url: config.base_url,
          };
        });
      }

      if (imageProviders.length > 0) {
        imgProviders = imageProviders.map(p => {
          providerIds.set(p.name, p.id);
          const config = p.config as Record<string, string>;
          return {
            name: p.name,
            model: config.model || '',
            isActive: p.is_default,
            isConnected: true,
            url: config.base_url,
          };
        });
      }

      if (videoProviders.length > 0) {
        vidProviders = videoProviders.map(p => {
          providerIds.set(p.name, p.id);
          const config = p.config as Record<string, string>;
          return {
            name: p.name,
            model: config.model || '',
            isActive: p.is_default,
            isConnected: true,
            url: config.base_url,
          };
        });
      }

      // Load generation params from the active LLM provider config
      const activeLlm = llmProviders.find(p => p.is_default);
      if (activeLlm) {
        const config = activeLlm.config as Record<string, string>;
        if (config.temperature) chatTemp = config.temperature;
        if (config.max_tokens) chatMaxTokens = config.max_tokens;
      }
    } catch (err) {
      console.error('Failed to load providers:', err);
    }
    isLoadingProviders = false;
  });

  // Add provider modal state
  let showAddModal = $state(false);
  let newProviderName = $state('');
  let newProviderType: 'llm' | 'image' | 'video' = $state('llm');
  let newProviderAdapter = $state('open_router');
  let newApiKey = $state('');
  let newBaseUrl = $state('');
  let newModel = $state('');
  let isSaving = $state(false);

  async function testConnection(providerName: string) {
    if (!isTauri) return;
    const id = providerIds.get(providerName);
    if (!id) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const connected = await ipc.testProviderConnection(id);
      chatProviders = chatProviders.map(p => p.name === providerName ? { ...p, isConnected: connected } : p);
      imgProviders = imgProviders.map(p => p.name === providerName ? { ...p, isConnected: connected } : p);
      vidProviders = vidProviders.map(p => p.name === providerName ? { ...p, isConnected: connected } : p);
      success(connected ? `${providerName} connected` : `${providerName} unreachable`);
    } catch (err) {
      toastError(`Connection test failed for ${providerName}`);
    }
  }

  async function addProvider() {
    if (!newProviderName.trim()) return;
    isSaving = true;
    try {
      let display: ProviderConfig;

      if (isTauri) {
        const ipc = await import('$lib/services/ipc');
        const config: Record<string, unknown> = {};
        if (newApiKey) config.api_key = newApiKey;
        if (newBaseUrl) config.base_url = newBaseUrl;
        if (newModel) config.model = newModel;
        const p = await ipc.createProvider(newProviderName, newProviderType, newProviderAdapter, config, false);
        providerIds.set(p.name, p.id);
        display = { name: p.name, model: newModel, isActive: false, isConnected: false, url: newBaseUrl || undefined, apiKey: newApiKey ? `${newApiKey.slice(0,6)}••••` : undefined };
      } else {
        // Browser dev mode — add to local state only
        display = { name: newProviderName, model: newModel, isActive: false, isConnected: false, url: newBaseUrl || undefined, apiKey: newApiKey ? `${newApiKey.slice(0,6)}••••` : undefined };
      }

      if (newProviderType === 'llm') chatProviders = [...chatProviders, display];
      else if (newProviderType === 'image') imgProviders = [...imgProviders, display];
      else vidProviders = [...vidProviders, display];
      showAddModal = false;
      newProviderName = ''; newApiKey = ''; newBaseUrl = ''; newModel = '';
      success(`Added ${display.name}`);
    } catch (err) { toastError('Failed to add provider'); }
    isSaving = false;
  }

  async function setActive(providerName: string, type: 'llm' | 'image' | 'video') {
    try {
      if (isTauri) {
        const id = providerIds.get(providerName);
        if (!id) return;
        const ipc = await import('$lib/services/ipc');
        await ipc.setDefaultProvider(id);
      }
      if (type === 'llm') chatProviders = chatProviders.map(p => ({ ...p, isActive: p.name === providerName }));
      else if (type === 'image') imgProviders = imgProviders.map(p => ({ ...p, isActive: p.name === providerName }));
      else vidProviders = vidProviders.map(p => ({ ...p, isActive: p.name === providerName }));
      success(`${providerName} set as default`);
    } catch (err) { toastError('Failed to set active provider'); }
  }

  async function deleteProvider(providerName: string, type: 'llm' | 'image' | 'video') {
    try {
      if (isTauri) {
        const id = providerIds.get(providerName);
        if (!id) return;
        const ipc = await import('$lib/services/ipc');
        await ipc.deleteProvider(id);
        providerIds.delete(providerName);
      }
      if (type === 'llm') chatProviders = chatProviders.filter(p => p.name !== providerName);
      else if (type === 'image') imgProviders = imgProviders.filter(p => p.name !== providerName);
      else vidProviders = vidProviders.filter(p => p.name !== providerName);
      success(`Deleted ${providerName}`);
    } catch (err) { toastError('Failed to delete provider'); }
  }

  async function saveProviderField(providerName: string, field: string, value: string) {
    if (!isTauri) return;
    const id = providerIds.get(providerName);
    if (!id) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const existing = await ipc.getProvider(id);
      const config = { ...(existing.config as Record<string, unknown>), [field]: value };
      await ipc.updateProvider(id, undefined, config);
    } catch (err) { toastError('Failed to save setting'); }
  }
</script>

<svelte:head>
  <title>Model Configuration — Mythic</title>
</svelte:head>

<div class="models-page">
  <!-- Header -->
  <header class="models-header">
    <div class="models-header-left">
      <h1 class="models-title">Model Configuration</h1>
      <span class="models-subtitle">Configure AI providers for chat, image, and video generation</span>
    </div>
    <button class="add-provider-btn" aria-label="Add new AI provider" onclick={() => showAddModal = true}>
      <Icon name="plus" size={14} color="#FFFFFF" />
      <span>Add Provider</span>
    </button>
  </header>

  <!-- Add Provider Modal -->
  {#if showAddModal}
    <div class="modal-backdrop" onclick={() => showAddModal = false} onkeydown={(e) => { if (e.key === 'Escape') showAddModal = false; }} role="dialog" aria-modal="true" aria-label="Add AI Provider">
      <div class="modal-card" onclick={(e) => e.stopPropagation()} role="document">
        <span class="modal-title">Add AI Provider</span>
        <div class="modal-field">
          <label class="field-label" for="np-name">Name</label>
          <input id="np-name" class="modal-input" bind:value={newProviderName} placeholder="My Provider" />
        </div>
        <div class="modal-row">
          <div class="modal-field">
            <label class="field-label" for="np-type">Type</label>
            <select id="np-type" class="modal-input" bind:value={newProviderType}>
              <option value="llm">Chat (LLM)</option>
              <option value="image">Image</option>
              <option value="video">Video</option>
            </select>
          </div>
          <div class="modal-field">
            <label class="field-label" for="np-adapter">Adapter</label>
            <select id="np-adapter" class="modal-input" bind:value={newProviderAdapter}>
              <option value="open_router">OpenRouter</option>
              <option value="ollama">Ollama</option>
              <option value="open_ai_compatible">OpenAI Compatible</option>
              <option value="silicon_flow">SiliconFlow</option>
            </select>
          </div>
        </div>
        <div class="modal-field">
          <label class="field-label" for="np-url">Base URL</label>
          <input id="np-url" class="modal-input mono" bind:value={newBaseUrl} placeholder="http://localhost:11434" />
        </div>
        <div class="modal-field">
          <label class="field-label" for="np-key">API Key</label>
          <input id="np-key" class="modal-input mono" type="password" bind:value={newApiKey} placeholder="sk-..." />
        </div>
        <div class="modal-field">
          <label class="field-label" for="np-model">Default Model</label>
          <input id="np-model" class="modal-input mono" bind:value={newModel} placeholder="meta-llama/llama-4-maverick" />
        </div>
        <div class="modal-actions">
          <button class="settings-btn outline" onclick={() => showAddModal = false}>Cancel</button>
          <button class="add-provider-btn" onclick={addProvider} disabled={isSaving || !newProviderName.trim()}>
            {isSaving ? 'Adding...' : 'Add Provider'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Three Column Layout -->
  <div class="models-grid">
    <!-- Chat Model Column -->
    <div class="model-column animate-fade-in-up stagger-1">
      <div class="column-header">
        <div class="column-icon" style="background: rgba(139, 92, 246, 0.12);">
          <Icon name="message-circle" size={14} color="var(--accent-primary)" />
        </div>
        <span class="column-title">Chat Model</span>
      </div>

      {#if isLoadingProviders}
        {#each Array(2) as _}
          <div class="provider-card skeleton-provider">
            <div class="skeleton-provider-row">
              <Skeleton variant="circle" width="28px" height="28px" />
              <div class="skeleton-provider-info">
                <Skeleton variant="text" width="60%" height="12px" />
                <Skeleton variant="text" width="80%" height="10px" />
              </div>
            </div>
          </div>
        {/each}
      {:else}
      {#each chatProviders as provider, i (provider.name)}
        <div
          class="provider-card"
          class:active={provider.isActive}
          style={provider.isActive ? 'border-color: var(--accent-primary);' : ''}
        >
          <div class="provider-header">
            <div class="provider-left">
              <span class="provider-dot" class:connected={provider.isConnected}></span>
              <span class="provider-name" class:active={provider.isActive}>{provider.name}</span>
            </div>
            <span class="provider-badge" class:active={provider.isActive} class:warning={!provider.isConnected && !provider.isActive}>
              {provider.isActive ? 'Active' : provider.isConnected ? 'Connected' : 'Not Connected'}
            </span>
          </div>

          {#if provider.isActive}
            <div class="field-group">
              <span class="field-label">Model</span>
              <div class="field-input">
                <input type="text" value={provider.model} class="mono" onblur={(e) => saveProviderField(provider.name, 'model', e.currentTarget.value)} />
              </div>
            </div>

            <div class="field-group">
              <span class="field-label">API Key</span>
              <div class="field-input">
                <input type="password" placeholder="sk-..." onblur={(e) => saveProviderField(provider.name, 'api_key', e.currentTarget.value)} />
              </div>
            </div>

            <div class="field-row">
              <div class="field-group">
                <label class="field-label" for="chat-temp">Temperature</label>
                <div class="field-input">
                  <input id="chat-temp" type="text" bind:value={chatTemp} class="mono" onblur={() => saveProviderField(provider.name, 'temperature', chatTemp)} />
                </div>
              </div>
              <div class="field-group">
                <label class="field-label" for="chat-max-tokens">Max Tokens</label>
                <div class="field-input">
                  <input id="chat-max-tokens" type="text" bind:value={chatMaxTokens} class="mono" onblur={() => saveProviderField(provider.name, 'max_tokens', chatMaxTokens)} />
                </div>
              </div>
            </div>
          {:else if provider.url}
            <span class="provider-url">{provider.url}</span>
          {/if}

          <div class="provider-actions">
            {#if !provider.isActive}
              <button class="action-btn" onclick={() => setActive(provider.name, 'llm')}>Set Active</button>
            {/if}
            <button class="action-btn" onclick={() => testConnection(provider.name)}>Test</button>
            <button class="action-btn danger" onclick={() => deleteProvider(provider.name, 'llm')}>Delete</button>
          </div>
        </div>
      {/each}
      {#if chatProviders.length === 0}
        <div class="empty-provider">
          <Icon name="message-circle" size={20} color="var(--fg-muted)" />
          <span class="empty-provider-text">No chat providers configured</span>
          <span class="empty-provider-hint">Click "Add Provider" to get started</span>
        </div>
      {/if}
      {/if}
    </div>

    <!-- Image Model Column -->
    <div class="model-column animate-fade-in-up stagger-2">
      <div class="column-header">
        <div class="column-icon" style="background: rgba(191, 64, 255, 0.12);">
          <Icon name="image" size={14} color="var(--accent-secondary)" />
        </div>
        <span class="column-title">Image Model</span>
      </div>

      {#if isLoadingProviders}
        {#each Array(2) as _}
          <div class="provider-card skeleton-provider">
            <div class="skeleton-provider-row">
              <Skeleton variant="circle" width="28px" height="28px" />
              <div class="skeleton-provider-info">
                <Skeleton variant="text" width="60%" height="12px" />
                <Skeleton variant="text" width="80%" height="10px" />
              </div>
            </div>
          </div>
        {/each}
      {:else}
      {#each imgProviders as provider (provider.name)}
        <div 
          class="provider-card" 
          class:active={provider.isActive}
          style={provider.isActive ? 'border-color: var(--accent-secondary);' : ''}
        >
          <div class="provider-header">
            <div class="provider-left">
              <span class="provider-dot" class:connected={provider.isConnected}></span>
              <span class="provider-name" class:active={provider.isActive}>{provider.name}</span>
            </div>
            <span class="provider-badge" class:active={provider.isActive} class:connected={provider.isConnected && !provider.isActive}>
              {provider.isActive ? 'Active' : provider.isConnected ? 'Connected' : 'Not Connected'}
            </span>
          </div>

          {#if provider.isActive}
            <div class="field-group">
              <span class="field-label">Model</span>
              <div class="field-input">
                <input type="text" value={provider.model} class="mono" onblur={(e) => saveProviderField(provider.name, 'model', e.currentTarget.value)} />
              </div>
            </div>

            <div class="field-group">
              <span class="field-label">API Key</span>
              <div class="field-input">
                <input type="password" placeholder="sk-..." onblur={(e) => saveProviderField(provider.name, 'api_key', e.currentTarget.value)} />
              </div>
            </div>

            <div class="field-row">
              <div class="field-group">
                <label class="field-label" for="img-width">Width</label>
                <div class="field-input">
                  <input id="img-width" type="text" bind:value={imgWidth} class="mono" />
                </div>
              </div>
              <div class="field-group">
                <label class="field-label" for="img-height">Height</label>
                <div class="field-input">
                  <input id="img-height" type="text" bind:value={imgHeight} class="mono" />
                </div>
              </div>
            </div>
          {:else if provider.url}
            <span class="provider-url">{provider.url}</span>
          {/if}

          <div class="provider-actions">
            {#if !provider.isActive}
              <button class="action-btn" onclick={() => setActive(provider.name, 'image')}>Set Active</button>
            {/if}
            <button class="action-btn" onclick={() => testConnection(provider.name)}>Test</button>
            <button class="action-btn danger" onclick={() => deleteProvider(provider.name, 'image')}>Delete</button>
          </div>
        </div>
      {/each}
      {#if imgProviders.length === 0}
        <div class="empty-provider">
          <Icon name="image" size={20} color="var(--fg-muted)" />
          <span class="empty-provider-text">No image providers configured</span>
          <span class="empty-provider-hint">Add a provider for image generation</span>
        </div>
      {/if}
      {/if}
    </div>

    <!-- Video Model Column -->
    <div class="model-column animate-fade-in-up stagger-3">
      <div class="column-header">
        <div class="column-icon" style="background: rgba(0, 242, 255, 0.12);">
          <Icon name="video" size={14} color="var(--accent-tertiary)" />
        </div>
        <span class="column-title">Video Model</span>
      </div>

      {#if isLoadingProviders}
        <div class="provider-card skeleton-provider">
          <div class="skeleton-provider-row">
            <Skeleton variant="circle" width="28px" height="28px" />
            <div class="skeleton-provider-info">
              <Skeleton variant="text" width="60%" height="12px" />
              <Skeleton variant="text" width="80%" height="10px" />
            </div>
          </div>
        </div>
      {:else}
      {#each vidProviders as provider (provider.name)}
        <div 
          class="provider-card" 
          class:active={provider.isActive}
          style={provider.isActive ? 'border-color: var(--accent-tertiary);' : ''}
        >
          <div class="provider-header">
            <div class="provider-left">
              <span class="provider-dot" class:connected={provider.isConnected}></span>
              <span class="provider-name" class:active={provider.isActive}>{provider.name}</span>
            </div>
            <span class="provider-badge active">Active</span>
          </div>

          <div class="field-group">
            <span class="field-label">Model</span>
            <div class="field-dropdown">
              <span class="field-value mono">{provider.model}</span>
              <Icon name="chevron-down" size={14} color="var(--fg-muted)" />
            </div>
          </div>

          <div class="field-row">
            <div class="field-group">
              <label class="field-label" for="vid-duration">Duration (sec)</label>
              <div class="field-input">
                <input id="vid-duration" type="text" bind:value={vidDuration} class="mono" />
              </div>
            </div>
            <div class="field-group">
              <label class="field-label" for="vid-fps">FPS</label>
              <div class="field-input">
                <input id="vid-fps" type="text" bind:value={vidFps} class="mono" />
              </div>
            </div>
          </div>

          <div class="field-group">
            <span class="field-label">Resolution</span>
            <div class="field-dropdown">
              <span class="field-value mono">{vidResolution}</span>
              <Icon name="chevron-down" size={14} color="var(--fg-muted)" />
            </div>
          </div>
        </div>
      {/each}
      {#if vidProviders.length === 0}
        <div class="empty-provider">
          <Icon name="video" size={20} color="var(--fg-muted)" />
          <span class="empty-provider-text">No video providers configured</span>
          <span class="empty-provider-hint">Add a provider for video generation</span>
        </div>
      {/if}
      {/if}

      <!-- GPU Warning -->
      <div class="warning-card">
        <Icon name="info" size={14} color="var(--warning)" />
        <span class="warning-text">Video generation requires significant GPU resources. Cloud providers are recommended for best results.</span>
      </div>
    </div>
  </div>
</div>

<style>
  .models-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
  }

  /* ── Header ── */
  .models-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 20px 28px 18px; flex-shrink: 0; position: relative;
  }
  .models-header::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .models-header-left { display: flex; flex-direction: column; gap: 3px; }
  .models-title {
    font-size: var(--text-2xl); font-weight: 800; letter-spacing: -0.5px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
  }
  .models-subtitle { font-size: var(--text-sm); color: #5a5a7a; letter-spacing: 0.3px; }

  .add-provider-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px; border: none; cursor: pointer;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); color: #fff;
    font-weight: 600; font-size: 13px; font-family: var(--font-body);
    box-shadow: 0 2px 12px rgba(139,92,246,0.25);
    transition: all 180ms ease;
  }
  .add-provider-btn:hover {
    box-shadow: 0 4px 20px rgba(139,92,246,0.4); transform: translateY(-1px);
  }
  .add-provider-btn:disabled { opacity: 0.5; pointer-events: none; }

  /* ── Grid ── */
  .models-grid {
    display: grid; grid-template-columns: repeat(3, 1fr);
    gap: 22px; padding: 28px; overflow-y: auto; flex: 1; align-items: start;
  }
  .models-grid::-webkit-scrollbar { width: 4px; }
  .models-grid::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  .model-column { display: flex; flex-direction: column; gap: 16px; }
  .column-header { display: flex; align-items: center; gap: 10px; }
  .column-icon {
    width: 32px; height: 32px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
  }
  .column-title { font-size: var(--text-lg); font-weight: 700; color: #e8e0ff; letter-spacing: -0.2px; }

  /* ── Provider Card ── */
  .provider-card {
    padding: 18px; border-radius: 14px;
    background: rgba(14,14,30,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    display: flex; flex-direction: column; gap: 14px;
    transition: border-color 200ms ease, box-shadow 250ms ease, transform 250ms cubic-bezier(0.34,1.56,0.64,1);
    position: relative;
  }
  .provider-card:hover {
    border-color: rgba(139,92,246,0.12);
    box-shadow: 0 8px 28px rgba(0,0,0,0.25), 0 0 15px rgba(139,92,246,0.05);
    transform: translateY(-2px);
  }
  .provider-card.active {
    border-color: rgba(139,92,246,0.25) !important;
    box-shadow: 0 0 20px rgba(139,92,246,0.08), inset 0 0 30px rgba(139,92,246,0.03);
  }

  .provider-header { display: flex; justify-content: space-between; align-items: center; }
  .provider-left { display: flex; align-items: center; gap: 8px; }
  .provider-dot {
    width: 8px; height: 8px; border-radius: 50%; background: #3a3a5a;
    transition: background 200ms, box-shadow 200ms;
  }
  .provider-dot.connected { background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.4); }
  .provider-name { font-size: var(--text-base); font-weight: 500; color: #8b8ba7; }
  .provider-name.active { font-weight: 700; color: #e8e0ff; }

  .provider-badge {
    padding: 3px 10px; border-radius: 99px; font-size: var(--text-xs); font-weight: 700;
    letter-spacing: 0.3px; background: rgba(90,90,120,0.15); color: #5a5a7a;
  }
  .provider-badge.active { background: rgba(16,185,129,0.12); color: #10B981; }
  .provider-badge.warning { background: rgba(245,158,11,0.12); color: #F59E0B; }
  .provider-badge.connected { background: rgba(16,185,129,0.08); color: #10B981; }

  .provider-url { font-size: 11px; color: #4a4a6a; font-family: var(--font-mono); }

  /* ── Fields ── */
  .field-group { display: flex; flex-direction: column; gap: 6px; flex: 1; }
  .field-label {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    text-transform: uppercase; letter-spacing: 1px; font-family: var(--font-mono);
  }
  .field-dropdown, .field-input {
    height: 38px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    padding: 0 12px; display: flex; align-items: center;
    transition: border-color 200ms ease;
  }
  .field-dropdown { justify-content: space-between; cursor: pointer; }
  .field-dropdown:hover, .field-input:focus-within { border-color: rgba(139,92,246,0.3); }
  .field-value { font-size: var(--text-md); color: #e0e0f0; }
  .mono { font-family: var(--font-mono); }
  .field-input input {
    width: 100%; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: 13px; font-family: var(--font-mono);
  }
  .field-input input::placeholder { color: #3a3a5a; }
  .field-row { display: flex; gap: 10px; }

  /* ── Warning ── */
  .warning-card {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 12px 14px; border-radius: 12px;
    background: rgba(245,158,11,0.04); border: 1px solid rgba(245,158,11,0.15);
  }
  .warning-text { font-size: var(--text-sm); color: #F59E0B; line-height: 1.5; }

  /* ── Provider Actions ── */
  .provider-actions { display: flex; gap: 6px; margin-top: 2px; }
  .action-btn {
    padding: 5px 12px; border-radius: 8px;
    border: 1px solid rgba(139,92,246,0.1); background: transparent;
    color: #6b6b8a; font-size: var(--text-xs); font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 150ms ease;
  }
  .action-btn:hover { background: rgba(139,92,246,0.06); color: #e0e0f0; border-color: rgba(139,92,246,0.2); }
  .action-btn:active { transform: scale(0.95); }
  .action-btn.danger { border-color: rgba(244,63,94,0.2); color: #F43F5E; }
  .action-btn.danger:hover { background: rgba(244,63,94,0.08); }

  /* ── Modal ── */
  .modal-backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,0.7);
    backdrop-filter: blur(8px); display: flex; align-items: center;
    justify-content: center; z-index: 200;
  }
  .modal-card {
    background: linear-gradient(175deg, #0e0e22, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12); border-radius: 20px;
    padding: 26px; width: 440px; max-width: 90vw;
    display: flex; flex-direction: column; gap: 16px;
    box-shadow: 0 24px 60px rgba(0,0,0,0.6), 0 0 30px rgba(139,92,246,0.08);
  }
  .modal-title { font-size: var(--text-xl); font-weight: 700; color: #e8e0ff; }
  .modal-field { display: flex; flex-direction: column; gap: 6px; flex: 1; }
  .modal-row { display: flex; gap: 12px; }
  .modal-input {
    height: 38px; padding: 0 12px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 13px; font-family: var(--font-body);
    outline: none; transition: border-color 200ms;
  }
  .modal-input:focus { border-color: rgba(139,92,246,0.35); }
  .modal-input.mono { font-family: var(--font-mono); }
  select.modal-input {
    appearance: none; cursor: pointer;
    background-image: url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b6b8a' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e");
    background-position: right 8px center; background-repeat: no-repeat; background-size: 16px;
    padding-right: 28px;
  }
  .modal-actions { display: flex; gap: 10px; justify-content: flex-end; margin-top: 4px; }

  .settings-btn.outline {
    flex: unset; background: transparent;
    border: 1px solid rgba(139,92,246,0.12); color: #8b8ba7;
    padding: 8px 16px; border-radius: 10px; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer; transition: all 150ms;
  }
  .settings-btn.outline:hover { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2); }

  /* ── Responsive ── */
  @media (max-width: 1024px) { .models-grid { grid-template-columns: 1fr; max-width: 600px; } }

  /* ── Skeleton ── */
  .skeleton-provider { padding: 14px; }
  .skeleton-provider-row { display: flex; align-items: center; gap: 10px; }
  .skeleton-provider-info { flex: 1; display: flex; flex-direction: column; gap: 6px; }

  /* ── Empty State ── */
  .empty-provider {
    display: flex; flex-direction: column; align-items: center; gap: 10px;
    padding: 36px 16px; border: 1px dashed rgba(139,92,246,0.1);
    border-radius: 14px; text-align: center;
  }
  .empty-provider-text { font-size: var(--text-md); color: #6b6b8a; font-weight: 600; }
  .empty-provider-hint { font-size: var(--text-sm); color: #4a4a6a; }

  /* ── Staggered Entrance ── */
  .animate-fade-in-up { animation: fadeInUp 400ms ease both; }
  .stagger-1 { animation-delay: 60ms; }
  .stagger-2 { animation-delay: 140ms; }
  .stagger-3 { animation-delay: 220ms; }
  @keyframes fadeInUp {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
