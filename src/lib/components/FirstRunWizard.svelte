<script lang="ts">
  import { goto } from '$app/navigation';
  import { settings } from '#lib/stores/settings.js';
  import { MODEL_SOURCES, rankModels, type ModelSource } from '#lib/utils/onboarding.js';
  import { STARTER_CHARACTERS } from '#lib/data/starterCharacters.js';

  let { onclose }: { onclose: () => void } = $props();

  type Step = 'welcome' | 'source' | 'connect' | 'model' | 'character';
  let step = $state<Step>('welcome');

  let source = $state<ModelSource>(MODEL_SOURCES[0]);
  let apiKey = $state('');
  let baseUrl = $state(MODEL_SOURCES[0].defaultBaseUrl);
  let providerId = $state<string | null>(null);
  let models = $state<string[]>([]);
  let model = $state('');
  let busy = $state(false);
  let error = $state('');

  const ipcPromise = () => import('#lib/services/ipc.js');

  function pickSource(s: ModelSource) {
    source = s;
    baseUrl = s.defaultBaseUrl;
    apiKey = '';
    error = '';
    step = 'connect';
  }

  function configFor(extraModel?: string): Record<string, unknown> {
    const config: Record<string, unknown> = {};
    if (apiKey.trim()) config.api_key = apiKey.trim();
    if (source.needsBaseUrl || source.defaultBaseUrl) {
      if (baseUrl.trim()) config.base_url = baseUrl.trim();
    }
    if (extraModel) config.model = extraModel;
    return config;
  }

  async function connect() {
    error = '';
    if (source.needsKey && !apiKey.trim()) {
      error = 'Paste your key first.';
      return;
    }
    busy = true;
    try {
      const ipc = await ipcPromise();
      if (providerId) {
        // Retrying after a failed test: update the same provider.
        await ipc.updateProvider(providerId, undefined, configFor());
      } else {
        const p = await ipc.createProvider(source.label, 'llm', source.adapter, configFor(), true);
        providerId = p.id;
      }
      const result = await ipc.testProviderConnection(providerId);
      if (!result.ok) {
        error = result.detail ?? 'Could not reach the model source.';
        return;
      }
      models = rankModels(await ipc.listProviderModels(providerId));
      model = models[0] ?? '';
      step = 'model';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function saveModel() {
    if (!providerId) return;
    error = '';
    if (!model.trim()) {
      error = 'Pick or type a model name.';
      return;
    }
    busy = true;
    try {
      const ipc = await ipcPromise();
      await ipc.updateProvider(providerId, undefined, configFor(model.trim()));
      step = 'character';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function finish() {
    settings.update((s) => ({ ...s, onboardingDone: true }));
    onclose();
  }

  async function startWith(key: string) {
    const starter = STARTER_CHARACTERS.find((c) => c.key === key);
    if (!starter) return;
    error = '';
    busy = true;
    try {
      const ipc = await ipcPromise();
      const ch = await ipc.createCharacter(String(starter.data.name), starter.data);
      const { createConversation } = await import('#lib/stores/chat.js');
      await createConversation(ch.id, ch.name);
      finish();
      goto('/');
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !busy) finish();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="wiz-title">
    {#if step === 'welcome'}
      <h2 id="wiz-title">Welcome to Janus</h2>
      <p class="lead">
        Janus is a private roleplay chat app. Everything stays on your computer. Setup takes about a
        minute: connect a model, then meet your first character.
      </p>
      <div class="row">
        <button class="btn primary" onclick={() => (step = 'source')}>Set up now</button>
        <button class="btn ghost" onclick={finish}>Skip, I will do it later</button>
      </div>
    {:else if step === 'source'}
      <h2 id="wiz-title">Where will the AI come from?</h2>
      <p class="lead">Janus needs a language model to write replies. Pick one; you can add more later.</p>
      <div class="options">
        {#each MODEL_SOURCES as s (s.id)}
          <button class="option" onclick={() => pickSource(s)}>
            <strong>{s.label}</strong>
            <span>{s.blurb}</span>
          </button>
        {/each}
      </div>
      <div class="row">
        <button class="btn ghost" onclick={() => (step = 'welcome')}>Back</button>
      </div>
    {:else if step === 'connect'}
      <h2 id="wiz-title">Connect {source.label}</h2>
      {#if source.needsBaseUrl}
        <label class="flabel" for="wiz-url">Address</label>
        <input id="wiz-url" class="finput" bind:value={baseUrl} />
      {/if}
      {#if source.needsKey || source.id === 'open_ai_compatible'}
        <label class="flabel" for="wiz-key">API key{source.needsKey ? '' : ' (optional)'}</label>
        <input
          id="wiz-key"
          class="finput"
          type="password"
          autocomplete="off"
          bind:value={apiKey}
          placeholder={source.keyHint}
        />
        {#if source.keyHint}<p class="hint">{source.keyHint}</p>{/if}
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row">
        <button class="btn ghost" disabled={busy} onclick={() => (step = 'source')}>Back</button>
        <button class="btn primary" disabled={busy} onclick={connect}>
          {busy ? 'Testing...' : 'Connect and test'}
        </button>
      </div>
    {:else if step === 'model'}
      <h2 id="wiz-title">Connected. Choose a model.</h2>
      {#if models.length}
        <label class="flabel" for="wiz-model">Model</label>
        <select id="wiz-model" class="finput" bind:value={model}>
          {#each models as m (m)}<option value={m}>{m}</option>{/each}
        </select>
      {:else}
        <p class="hint">No models were listed. Type the exact model name instead.</p>
        <input class="finput" aria-label="Model name" bind:value={model} />
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row">
        <button class="btn primary" disabled={busy} onclick={saveModel}>Continue</button>
      </div>
    {:else}
      <h2 id="wiz-title">Meet your first character</h2>
      <p class="lead">Pick one to start chatting. You can import or create more in Characters.</p>
      <div class="options">
        {#each STARTER_CHARACTERS as c (c.key)}
          <button class="option" disabled={busy} onclick={() => startWith(c.key)}>
            <strong>{String(c.data.name)}</strong>
            <span>{c.tagline}</span>
          </button>
        {/each}
      </div>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row">
        <button class="btn ghost" disabled={busy} onclick={finish}>Finish without a chat</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: grid;
    place-items: center;
    padding: 16px;
    background: rgba(5, 5, 15, 0.72);
    backdrop-filter: blur(6px);
  }
  .dialog {
    width: min(520px, 100%);
    max-height: 90vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 28px;
    background: var(--surface-sidebar);
    border: 1px solid var(--border-active);
    border-radius: var(--rounded-xl);
    box-shadow: var(--shadow-lg);
    color: var(--fg-primary);
  }
  h2 {
    margin: 0;
    font-family: var(--font-heading);
    font-size: var(--text-lg);
  }
  .lead,
  .hint {
    margin: 0;
    color: var(--fg-secondary);
    font-size: var(--text-md);
    line-height: 1.5;
  }
  .hint {
    font-size: var(--text-sm);
  }
  .err {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-md);
  }
  .flabel {
    font-size: var(--text-sm);
    color: var(--fg-secondary);
  }
  .finput {
    padding: 10px 12px;
    background: var(--surface-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--rounded-md);
    color: var(--fg-primary);
    font: inherit;
  }
  .finput:focus-visible,
  .btn:focus-visible,
  .option:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 2px;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .option {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    text-align: left;
    background: var(--surface-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--rounded-lg);
    color: var(--fg-primary);
    font: inherit;
    cursor: pointer;
    transition: border-color 0.18s ease, background 0.18s ease;
  }
  .option span {
    color: var(--fg-secondary);
    font-size: var(--text-sm);
  }
  .option:hover:not(:disabled) {
    border-color: var(--border-active);
    background: var(--surface-hover);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .btn {
    padding: 10px 16px;
    border-radius: var(--rounded-md);
    border: 1px solid var(--border-active);
    font: inherit;
    cursor: pointer;
  }
  .btn.primary {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: #fff;
  }
  .btn.ghost {
    background: transparent;
    color: var(--fg-secondary);
  }
  .btn:disabled,
  .option:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (prefers-reduced-motion: reduce) {
    .option {
      transition: none;
    }
  }
</style>
