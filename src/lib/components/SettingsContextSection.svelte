<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import SelectCombobox from './SelectCombobox.svelte';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let maxContextTokens = $state($settings.maxContextTokens);
  let autoSummarize = $state($settings.autoSummarize);
  let ragEnabled = $state($settings.ragEnabled ?? false);
  let ragEmbeddingModel = $state($settings.ragEmbeddingModel ?? 'openai/text-embedding-3-small');
  let ragTopK = $state($settings.ragTopK ?? 5);
  let ragMinSimilarity = $state($settings.ragMinSimilarity ?? 0.7);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { maxContextTokens, autoSummarize, ragEnabled, ragEmbeddingModel, ragTopK, ragMinSimilarity };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  // Embedding index status (loaded from backend)
  let indexStatus = $state<{
    total_messages: number;
    embedded_messages: number;
    index_model: string | null;
    needs_rebuild: boolean;
    coverage_percent: number;
    index_dimension: number | null;
    selected_dimension: number | null;
    dimension_mismatch: boolean;
  } | null>(null);
  let isLoadingIndex = $state(false);
  let isRebuilding = $state(false);
  let isBackfilling = $state(false);

  async function loadIndexStatus() {
    if (!isTauri) return;
    isLoadingIndex = true;
    try {
      const ipc = await import('$lib/services/ipc');
      indexStatus = await ipc.getEmbeddingIndexStatus(null, ragEmbeddingModel);
    } catch (err) {
      console.warn('[Memory] Failed to load index status:', err);
    }
    isLoadingIndex = false;
  }

  async function rebuildIndex() {
    if (!isTauri || isRebuilding) return;
    isRebuilding = true;
    try {
      const ipc = await import('$lib/services/ipc');
      indexStatus = await ipc.rebuildEmbeddingIndex(null, ragEmbeddingModel);
      success('Embedding index rebuilt successfully');
    } catch (err) {
      toastError('Failed to rebuild index');
      console.error('[Memory] Rebuild failed:', err);
    }
    isRebuilding = false;
  }

  async function backfillIndex() {
    if (!isTauri || isBackfilling) return;
    isBackfilling = true;
    try {
      const ipc = await import('$lib/services/ipc');
      indexStatus = await ipc.backfillMissingEmbeddings(null);
      success('Missing embeddings filled successfully');
    } catch (err) {
      toastError('Failed to backfill embeddings');
      console.error('[Memory] Backfill failed:', err);
    }
    isBackfilling = false;
  }

  // All embedding models across providers — populates the picker below.
  let allEmbeddingModels = $state<{ model_id: string; provider_id: string; provider_name: string; enabled: boolean }[]>([]);
  let isSwitchingEmbeddingModel = $state(false);

  async function loadEnabledEmbeddingModel() {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const models = await ipc.listEmbeddingModels();
      allEmbeddingModels = models.map(m => ({ model_id: m.model_id, provider_id: m.provider_id, provider_name: m.provider_name, enabled: m.enabled }));
      const enabled = models.filter(m => m.enabled);
      if (enabled.length > 0) {
        ragEmbeddingModel = enabled[0].model_id;
      }
    } catch (err) {
      console.warn('[Settings] Failed to load enabled embedding model:', err);
    }
  }

  // Only one embedding model can be "active" at a time (live chat embedding
  // and index rebuilds both resolve to whichever one is enabled) — so
  // picking a different one here disables whatever was enabled before and
  // enables the new choice, keeping that invariant true from the UI.
  async function switchEmbeddingModel(modelId: string) {
    if (!isTauri || modelId === ragEmbeddingModel || isSwitchingEmbeddingModel) return;
    const next = allEmbeddingModels.find(m => m.model_id === modelId);
    if (!next) return;
    isSwitchingEmbeddingModel = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const previous = allEmbeddingModels.find(m => m.enabled && m.model_id !== modelId);
      if (previous) {
        await ipc.toggleModelEnabled(previous.provider_id, previous.model_id, 'embedding', false);
      }
      await ipc.toggleModelEnabled(next.provider_id, next.model_id, 'embedding', true);
      ragEmbeddingModel = modelId;
      allEmbeddingModels = allEmbeddingModels.map(m => ({ ...m, enabled: m.model_id === modelId }));
      success(`Embedder set to ${modelId}`);
    } catch (err) {
      toastError('Failed to switch embedding model');
      console.error('[Settings] Failed to switch embedding model:', err);
    }
    isSwitchingEmbeddingModel = false;
  }

  onMount(() => {
    loadEnabledEmbeddingModel();
    if (ragEnabled) loadIndexStatus();

    // Listen for real-time embedding updates from the backend
    let embedCleanup: (() => void) | null = null;
    let debounceTimer: ReturnType<typeof setTimeout> | null = null;
    if (isTauri) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen('embedding_updated', () => {
          // Debounce: avoid spamming backend when multiple embeds fire rapidly
          if (debounceTimer) clearTimeout(debounceTimer);
          debounceTimer = setTimeout(() => {
            if (ragEnabled) loadIndexStatus();
          }, 500);
        }).then(unlisten => {
          embedCleanup = unlisten;
        });
      });
    }

    return () => {
      if (embedCleanup) embedCleanup();
      if (debounceTimer) clearTimeout(debounceTimer);
    };
  });

  $effect(() => {
    if (ragEnabled && isTauri) {
      loadIndexStatus();
    }
  });
</script>

<div class="panel-heading animate-fade-in-up stagger-2b">
  <span class="panel-heading-title">Context & Memory</span>
  <span class="panel-heading-desc">What the model sees each turn, and what gets remembered long-term</span>
</div>
<section class="settings-section settings-section-bounded animate-fade-in-up stagger-2b">
  <div class="section-header">
    <Icon name="network" size={16} color="var(--accent-primary)" />
    <span class="section-title">Context Management</span>
  </div>

  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Context Window Size</span>
      <span class="setting-desc">Max tokens the model can see (match your model's limit)</span>
    </div>
    <div class="font-dropdown-wrapper">
      <select
        class="setting-dropdown"
        bind:value={maxContextTokens}
        aria-label="Context window size"
      >
        <option value={4096}>4K</option>
        <option value={8192}>8K</option>
        <option value={16384}>16K</option>
        <option value={32768}>32K</option>
        <option value={65536}>64K</option>
        <option value={131072}>128K</option>
      </select>
    </div>
  </div>

  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Auto-Summarize</span>
      <span class="setting-desc">{autoSummarize ? 'Evicted messages are summarized to preserve context' : 'Evicted messages are silently dropped'}</span>
    </div>
    <button
      class="toggle-switch"
      class:on={autoSummarize}
      onclick={() => {
        autoSummarize = !autoSummarize;
        success(autoSummarize ? 'Auto-summarize enabled — evicted context will be preserved' : 'Auto-summarize disabled');
      }}
      role="switch"
      aria-checked={autoSummarize}
      aria-label="Toggle auto-summarize"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>
</section>

<!-- Memory (Vector RAG) -->
<section class="settings-section settings-section-bounded animate-fade-in-up stagger-2c">
  <div class="section-header">
    <Icon name="server" size={16} color="var(--accent-primary)" />
    <span class="section-title">Memory</span>
    <span class="memory-badge" class:memory-active={ragEnabled}>{ragEnabled ? 'Active' : 'Disabled'}</span>
  </div>

  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Semantic Memory</span>
      <span class="setting-desc">{ragEnabled ? 'Messages are embedded and searchable by meaning' : 'Enable to index conversations for intelligent recall'}</span>
    </div>
    <button
      class="toggle-switch"
      class:on={ragEnabled}
      onclick={() => {
        ragEnabled = !ragEnabled;
        success(ragEnabled ? 'Semantic memory enabled — messages will be indexed' : 'Semantic memory disabled');
      }}
      role="switch"
      aria-checked={ragEnabled}
      aria-label="Toggle semantic memory"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>

  {#if ragEnabled}
    <div class="memory-config" style="animation: slideDown 220ms cubic-bezier(0.34,1.56,0.64,1)">
      <!-- Embedding Model — picks among models enabled on the
           Embedding Models page; selecting one here enables it there
           too (only one can be active at a time). -->
      <div class="setting-row">
        <div class="setting-label">
          <span class="setting-name">Embedder Model</span>
          <span class="setting-desc">Manage the full list in <a href="/embedders" class="settings-link">AI Studio → Embedding Models</a></span>
        </div>
        {#if allEmbeddingModels.length > 0}
          <div class="embedder-combo-wrap">
            <SelectCombobox
              bind:value={ragEmbeddingModel}
              disabled={isSwitchingEmbeddingModel}
              ariaLabel="Embedder model"
              emptyText="No matches"
              onChange={switchEmbeddingModel}
              options={allEmbeddingModels.map(m => ({ value: m.model_id, label: m.model_id, sublabel: m.provider_name }))}
            />
          </div>
        {:else}
          <span class="setting-value-readonly mono">No embedding models enabled</span>
        {/if}
      </div>

      <!-- Retrieval Settings -->
      <div class="retrieval-row">
        <div class="retrieval-field">
          <span class="retrieval-label">Top-K Results</span>
          <select class="setting-dropdown" bind:value={ragTopK} aria-label="Top-K results">
            <option value={3}>3</option>
            <option value={5}>5</option>
            <option value={8}>8</option>
            <option value={10}>10</option>
          </select>
        </div>
        <div class="retrieval-field">
          <span class="retrieval-label">Min Similarity</span>
          <select class="setting-dropdown" bind:value={ragMinSimilarity} aria-label="Minimum similarity">
            <option value={0.5}>50%</option>
            <option value={0.6}>60%</option>
            <option value={0.7}>70%</option>
            <option value={0.8}>80%</option>
            <option value={0.9}>90%</option>
          </select>
        </div>
      </div>

      <!-- Index Status Panel -->
      <div class="index-panel">
        <div class="index-header">
          <span class="index-title">Index Status</span>
          <button class="index-refresh-btn" onclick={loadIndexStatus} disabled={isLoadingIndex} aria-label="Refresh index status" title="Refresh index status">
            <Icon name="refresh-cw" size={11} color={isLoadingIndex ? '#4a4a6a' : '#a78bfa'} />
          </button>
        </div>

        {#if isLoadingIndex}
          <div class="index-loading">
            <div class="index-spinner"></div>
            <span>Checking index...</span>
          </div>
        {:else if indexStatus}
          <div class="index-stats">
            <div class="index-stat">
              <span class="stat-value">{indexStatus.embedded_messages}</span>
              <span class="stat-label">Indexed</span>
            </div>
            <div class="index-stat">
              <span class="stat-value">{indexStatus.total_messages}</span>
              <span class="stat-label">Total</span>
            </div>
            <div class="index-stat">
              <span class="stat-value">{indexStatus.coverage_percent.toFixed(0)}%</span>
              <span class="stat-label">Coverage</span>
            </div>
          </div>

          <!-- Progress bar -->
          <div class="index-progress">
            <div class="index-progress-fill" style="width: {indexStatus.coverage_percent}%"></div>
          </div>

          <!-- Dimension Mismatch Warning -->
          {#if indexStatus.dimension_mismatch && indexStatus.index_dimension && indexStatus.selected_dimension}
            <div class="dim-mismatch-alert">
              <div class="dim-mismatch-glow"></div>
              <div class="dim-mismatch-content">
                <div class="dim-mismatch-header">
                  <div class="dim-mismatch-icon-wrap">
                    <svg class="dim-mismatch-icon" viewBox="0 0 24 24" fill="none">
                      <path d="M12 9v4m0 4h.01M4.93 19h14.14c1.34 0 2.18-1.45 1.51-2.6L13.51 4.24a1.73 1.73 0 00-3.02 0L3.42 16.4C2.75 17.55 3.59 19 4.93 19z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </div>
                  <div class="dim-mismatch-title-group">
                    <span class="dim-mismatch-title">Dimension Mismatch</span>
                    <span class="dim-mismatch-severity">Incompatible</span>
                  </div>
                </div>

                <div class="dim-mismatch-body">
                  <p class="dim-mismatch-desc">
                    Your stored embeddings use a different vector size than the currently selected model.
                    Existing embeddings cannot be used for similarity search with the new model.
                  </p>

                  <div class="dim-compare">
                    <div class="dim-compare-item dim-old">
                      <span class="dim-compare-label">Stored Index</span>
                      <div class="dim-compare-value-wrap">
                        <span class="dim-compare-value">{indexStatus.index_dimension}</span>
                        <span class="dim-compare-unit">dims</span>
                      </div>
                      {#if indexStatus.index_model}
                        <span class="dim-compare-model">{indexStatus.index_model}</span>
                      {/if}
                    </div>

                    <div class="dim-compare-arrow">
                      <svg viewBox="0 0 24 24" fill="none" width="20" height="20">
                        <path d="M5 12h14m0 0l-4-4m4 4l-4 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                      </svg>
                    </div>

                    <div class="dim-compare-item dim-new">
                      <span class="dim-compare-label">Selected Model</span>
                      <div class="dim-compare-value-wrap">
                        <span class="dim-compare-value">{indexStatus.selected_dimension}</span>
                        <span class="dim-compare-unit">dims</span>
                      </div>
                      <span class="dim-compare-model">{ragEmbeddingModel}</span>
                    </div>
                  </div>

                  <div class="dim-mismatch-resolution">
                    <span class="dim-resolution-title">How to resolve</span>
                    <span class="dim-resolution-text">Click <strong>Rebuild Index</strong> below to re-embed all messages with the new model. This will delete existing embeddings and create new ones with the correct dimensions.</span>
                  </div>
                </div>
              </div>
            </div>

          <!-- Model Mismatch (same dimensions, different model) -->
          {:else if indexStatus.needs_rebuild && !indexStatus.dimension_mismatch}
            <div class="rebuild-warning">
              <Icon name="alert-triangle" size={13} color="#F59E0B" />
              <div class="rebuild-text">
                <span class="rebuild-title">Model Changed</span>
                <span class="rebuild-desc">
                  Index built with <code>{indexStatus.index_model}</code>, but <code>{ragEmbeddingModel}</code> is selected.
                  Rebuild to use the new model for consistent results.
                </span>
              </div>
            </div>
          {/if}

          <!-- Rebuild Button -->
          <button
            class="rebuild-btn"
            class:rebuild-urgent={indexStatus.dimension_mismatch}
            class:rebuilding={isRebuilding}
            onclick={rebuildIndex}
            disabled={isRebuilding}
          >
            {#if isRebuilding}
              <div class="btn-spinner"></div>
              Rebuilding…
            {:else}
              <Icon name="refresh-cw" size={13} color={indexStatus.dimension_mismatch ? '#F59E0B' : '#e0e0f0'} />
              {indexStatus.dimension_mismatch ? 'Rebuild Index (Required)' : indexStatus.needs_rebuild ? 'Rebuild Index' : 'Rebuild Index'}
            {/if}
          </button>

          <!-- Backfill Button — shown when coverage < 100% and no rebuild needed -->
          {#if indexStatus.coverage_percent < 100 && !indexStatus.needs_rebuild && !indexStatus.dimension_mismatch}
            <button
              class="rebuild-btn backfill-btn"
              class:rebuilding={isBackfilling}
              onclick={backfillIndex}
              disabled={isBackfilling || isRebuilding}
            >
              {#if isBackfilling}
                <div class="btn-spinner"></div>
                Catching up…
              {:else}
                <Icon name="zap" size={13} color="#34D399" />
                Catch Up ({Math.round(indexStatus.total_messages - indexStatus.embedded_messages)} missing)
              {/if}
            </button>
          {/if}
        {:else}
          <div class="index-empty">
            <span>No index data available</span>
            <span class="index-empty-hint">Start chatting to begin building the index</span>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  /* ── Memory Section ── */
  .memory-badge {
    margin-left: auto; padding: 2px 8px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
    font-family: var(--font-mono);
    background: rgba(74,74,106,0.15); color: #4a4a6a;
    transition: all 250ms;
  }
  .memory-badge.memory-active {
    background: rgba(16,185,129,0.12); color: #10B981;
  }

  .memory-config {
    display: flex; flex-direction: column; gap: 14px;
  }

  .embedder-combo-wrap { width: 280px; flex-shrink: 0; }

  .setting-value-readonly {
    display: inline-flex; align-items: center;
    height: 34px; padding: 0 14px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px dashed rgba(139,92,246,0.1);
    font-size: 12px; font-weight: 600; color: #a78bfa;
    letter-spacing: 0.2px; user-select: all;
  }
  .setting-value-readonly.mono { font-family: var(--font-mono); }
  .settings-link {
    color: var(--accent); text-decoration: none; font-weight: 600;
    transition: color 150ms, opacity 150ms;
  }
  .settings-link:hover { opacity: 0.8; text-decoration: underline; }

  .retrieval-row {
    display: flex; gap: 12px;
  }
  .retrieval-field {
    display: flex; flex-direction: column; gap: 5px; flex: 1;
  }
  .retrieval-label {
    font-size: 10px; font-weight: 700; letter-spacing: 0.8px;
    text-transform: uppercase; color: #4a4a6a;
    font-family: var(--font-mono);
  }

  /* Index Status Panel */
  .index-panel {
    padding: 14px 16px; border-radius: 12px;
    background: rgba(10,10,24,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    display: flex; flex-direction: column; gap: 12px;
  }
  .index-header {
    display: flex; align-items: center; justify-content: space-between;
  }
  .index-title {
    font-size: 11px; font-weight: 700; letter-spacing: 0.8px;
    text-transform: uppercase; color: #6b6b8a;
    font-family: var(--font-mono);
  }
  .index-refresh-btn {
    width: 26px; height: 26px; border-radius: 7px;
    border: 1px solid rgba(139,92,246,0.08);
    background: transparent; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: all 150ms;
  }
  .index-refresh-btn:hover { background: rgba(139,92,246,0.08); }
  .index-refresh-btn:disabled { opacity: 0.4; cursor: default; }

  .index-stats {
    display: flex; gap: 12px;
  }
  .index-stat {
    flex: 1; display: flex; flex-direction: column; align-items: center;
    gap: 2px; padding: 8px; border-radius: 8px;
    background: rgba(139,92,246,0.04);
  }
  .stat-value {
    font-size: 18px; font-weight: 800; color: #e0e0f0;
    font-family: var(--font-mono); letter-spacing: -0.5px;
  }
  .stat-label {
    font-size: 9px; font-weight: 700; letter-spacing: 0.8px;
    text-transform: uppercase; color: #4a4a6a;
    font-family: var(--font-mono);
  }

  .index-progress {
    height: 4px; border-radius: 99px;
    background: rgba(139,92,246,0.08); overflow: hidden;
  }
  .index-progress-fill {
    height: 100%; border-radius: 99px;
    background: var(--accent);
    transition: width 500ms cubic-bezier(0.34,1.56,0.64,1);
  }

  /* ── Dimension Mismatch Alert ── */
  .dim-mismatch-alert {
    position: relative; border-radius: 14px; overflow: hidden;
    animation: dimAlertIn 350ms cubic-bezier(0.34,1.56,0.64,1) both;
  }
  @keyframes dimAlertIn {
    from { opacity: 0; transform: translateY(-8px) scale(0.97); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  .dim-mismatch-glow {
    position: absolute; inset: 0; border-radius: 14px; z-index: 0;
    background: conic-gradient(from 180deg at 50% 50%,
      rgba(245,158,11,0.25), rgba(239,68,68,0.2), rgba(245,158,11,0.25),
      rgba(239,68,68,0.2), rgba(245,158,11,0.25));
    animation: glowRotate 6s linear infinite;
    filter: blur(1px);
  }
  @keyframes glowRotate {
    to { transform: rotate(360deg); }
  }
  .dim-mismatch-content {
    position: relative; z-index: 1; margin: 1px; border-radius: 13px;
    background: linear-gradient(175deg, rgba(20,16,10,0.97), rgba(12,10,8,0.98));
    backdrop-filter: blur(20px);
  }
  .dim-mismatch-header {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px 0;
  }
  .dim-mismatch-icon-wrap {
    width: 32px; height: 32px; border-radius: 9px;
    background: rgba(245,158,11,0.12);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .dim-mismatch-icon {
    width: 18px; height: 18px; color: #F59E0B;
    animation: iconPulse 2s ease-in-out infinite;
  }
  @keyframes iconPulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }
  .dim-mismatch-title-group {
    display: flex; flex-direction: column; gap: 1px;
  }
  .dim-mismatch-title {
    font-size: 13px; font-weight: 800; color: #F59E0B;
    letter-spacing: -0.2px;
  }
  .dim-mismatch-severity {
    font-size: 9px; font-weight: 700; letter-spacing: 1px;
    text-transform: uppercase; color: #EF4444;
    font-family: var(--font-mono);
  }
  .dim-mismatch-body {
    padding: 10px 14px 14px;
    display: flex; flex-direction: column; gap: 12px;
  }
  .dim-mismatch-desc {
    font-size: 11px; color: #8b8ba7; line-height: 1.6; margin: 0;
  }

  /* ── Dimension Comparison ── */
  .dim-compare {
    display: flex; align-items: center; gap: 0;
  }
  .dim-compare-item {
    flex: 1; display: flex; flex-direction: column; align-items: center;
    gap: 4px; padding: 10px 8px; border-radius: 10px;
    transition: all 200ms;
  }
  .dim-old {
    background: rgba(239,68,68,0.06);
    border: 1px solid rgba(239,68,68,0.12);
  }
  .dim-new {
    background: rgba(16,185,129,0.06);
    border: 1px solid rgba(16,185,129,0.12);
  }
  .dim-compare-label {
    font-size: 8px; font-weight: 700; letter-spacing: 1px;
    text-transform: uppercase; font-family: var(--font-mono);
  }
  .dim-old .dim-compare-label { color: #EF4444; }
  .dim-new .dim-compare-label { color: #10B981; }
  .dim-compare-value-wrap {
    display: flex; align-items: baseline; gap: 3px;
  }
  .dim-compare-value {
    font-size: 22px; font-weight: 900; letter-spacing: -1px;
    font-family: var(--font-mono);
  }
  .dim-old .dim-compare-value { color: #F87171; }
  .dim-new .dim-compare-value { color: #34D399; }
  .dim-compare-unit {
    font-size: 9px; font-weight: 700; color: #4a4a6a;
    font-family: var(--font-mono); letter-spacing: 0.5px;
  }
  .dim-compare-model {
    font-size: 9px; font-family: var(--font-mono); color: #5a5a7a;
    max-width: 120px; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; text-align: center;
  }
  .dim-compare-arrow {
    color: #3a3a5a; flex-shrink: 0; padding: 0 4px;
    animation: arrowBounce 1.5s ease-in-out infinite;
  }
  @keyframes arrowBounce {
    0%, 100% { transform: translateX(0); }
    50% { transform: translateX(3px); }
  }

  /* ── Resolution Box ── */
  .dim-mismatch-resolution {
    padding: 8px 10px; border-radius: 8px;
    background: rgba(139,92,246,0.04);
    border-left: 2px solid rgba(139,92,246,0.3);
    display: flex; flex-direction: column; gap: 3px;
  }
  .dim-resolution-title {
    font-size: 9px; font-weight: 700; letter-spacing: 1px;
    text-transform: uppercase; color: #8B5CF6;
    font-family: var(--font-mono);
  }
  .dim-resolution-text {
    font-size: 11px; color: #6b6b8a; line-height: 1.5;
  }
  .dim-resolution-text strong {
    color: #a78bfa; font-weight: 700;
  }

  /* ── Rebuild Warning (model-only mismatch) ── */
  .rebuild-warning {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 10px 14px; border-radius: 10px;
    background: rgba(245,158,11,0.06);
    border: 1px solid rgba(245,158,11,0.15);
    animation: dimAlertIn 250ms ease both;
  }
  .rebuild-text {
    display: flex; flex-direction: column; gap: 3px;
  }
  .rebuild-title {
    font-size: 12px; font-weight: 700; color: #F59E0B;
  }
  .rebuild-desc {
    font-size: 11px; color: #8b8ba7; line-height: 1.5;
  }
  .rebuild-desc code {
    padding: 1px 5px; border-radius: 4px;
    background: rgba(139,92,246,0.1); color: #a78bfa;
    font-size: 10px; font-family: var(--font-mono);
  }

  /* ── Rebuild Button ── */
  .rebuild-btn {
    display: flex; align-items: center; justify-content: center; gap: 8px;
    width: 100%; padding: 10px; border-radius: 10px;
    background: rgba(139,92,246,0.08); border: 1px solid rgba(139,92,246,0.12);
    color: #e0e0f0; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 180ms ease;
  }
  .rebuild-btn:hover { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.22); }
  .rebuild-btn:disabled { opacity: 0.5; cursor: default; }
  .rebuild-btn.rebuilding { color: #a78bfa; }
  .rebuild-btn.rebuild-urgent {
    background: rgba(245,158,11,0.1);
    border-color: rgba(245,158,11,0.25);
    color: #F59E0B;
    animation: urgentPulse 2s ease-in-out infinite;
  }
  .rebuild-btn.rebuild-urgent:hover {
    background: rgba(245,158,11,0.18);
    border-color: rgba(245,158,11,0.35);
  }
  @keyframes urgentPulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(245,158,11,0); }
    50% { box-shadow: 0 0 0 4px rgba(245,158,11,0.08); }
  }

  /* ── Backfill / Catch-Up Button ── */
  .backfill-btn {
    margin-top: 6px;
    background: rgba(52,211,153,0.08);
    border-color: rgba(52,211,153,0.15);
    color: #34D399;
  }
  .backfill-btn:hover { background: rgba(52,211,153,0.14); border-color: rgba(52,211,153,0.25); }
  .backfill-btn.rebuilding { color: #6EE7B7; }

  .btn-spinner {
    width: 14px; height: 14px; border-radius: 50%;
    border: 2px solid rgba(139,92,246,0.2);
    border-top-color: #a78bfa;
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .index-loading {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 0; color: #6b6b8a; font-size: 12px;
  }
  .index-spinner {
    width: 16px; height: 16px; border-radius: 50%;
    border: 2px solid rgba(139,92,246,0.15);
    border-top-color: #a78bfa;
    animation: spin 700ms linear infinite;
  }

  .index-empty {
    display: flex; flex-direction: column; align-items: center;
    gap: 4px; padding: 16px 0; text-align: center;
    color: #4a4a6a; font-size: 12px;
  }
  .index-empty-hint {
    font-size: 10px; color: #3a3a5a;
  }
</style>
