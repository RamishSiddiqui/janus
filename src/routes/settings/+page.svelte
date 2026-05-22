<script lang="ts">
  import Icon from '$lib/components/Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError, info as toastInfo } from '$lib/stores/toast';
  import { browser } from '$app/environment';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Bind to store values
  let theme = $state($settings.theme);
  let fontSize = $state($settings.fontSize);
  let streamingEnabled = $state($settings.streamingEnabled);
  let autoGenerateImages = $state($settings.autoGenerateImages);
  let autoSaveMemories = $state($settings.autoSaveMemories);
  let localStorageOnly = $state($settings.localStorageOnly);
  let systemPrompt = $state($settings.systemPrompt);
  let postHistoryInstructions = $state($settings.postHistoryInstructions);
  let maxContextTokens = $state($settings.maxContextTokens);
  let autoSummarize = $state($settings.autoSummarize);

  // Memory / RAG settings
  let ragEnabled = $state($settings.ragEnabled ?? false);
  let ragEmbeddingModel = $state($settings.ragEmbeddingModel ?? 'openai/text-embedding-3-small');
  let ragTopK = $state($settings.ragTopK ?? 5);
  let ragMinSimilarity = $state($settings.ragMinSimilarity ?? 0.7);

  // Embedding index status (loaded from backend)
  let indexStatus = $state<{
    total_messages: number;
    embedded_messages: number;
    index_model: string | null;
    needs_rebuild: boolean;
    coverage_percent: number;
  } | null>(null);
  let isLoadingIndex = $state(false);
  let isRebuilding = $state(false);

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

  $effect(() => {
    if (ragEnabled && isTauri) {
      loadIndexStatus();
    }
  });

  let showFontDropdown = $state(false);
  let dropdownStyle = $state('');
  let showClearConfirm = $state(false);
  let showPrivacyConfirm = $state(false);
  let isExporting = $state(false);
  let isImporting = $state(false);

  const fontSizes = ['Small', 'Medium', 'Large'] as const;

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    // Read all reactive locals to track them
    const snapshot = {
      theme,
      fontSize,
      streamingEnabled,
      autoGenerateImages,
      autoSaveMemories,
      localStorageOnly,
      systemPrompt,
      postHistoryInstructions,
      maxContextTokens,
      autoSummarize,
      ragEnabled,
      ragEmbeddingModel,
      ragTopK,
      ragMinSimilarity,
    };
    // Debounce the store write to break the reactive cycle
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  function resetSystemPrompt() {
    settings.reset();
    systemPrompt = $settings.systemPrompt;
    postHistoryInstructions = $settings.postHistoryInstructions;
    success('Prompts reset to defaults');
  }

  function toggleFontDropdown() {
    showFontDropdown = !showFontDropdown;
    if (showFontDropdown) {
      const btn = document.querySelector('.setting-dropdown') as HTMLElement | null;
      if (btn) {
        const rect = btn.getBoundingClientRect();
        dropdownStyle = `top:${rect.bottom + 6}px;right:${window.innerWidth - rect.right}px;width:120px;`;
      }
    }
  }

  // Close dropdown on click outside, scroll, or resize
  $effect(() => {
    if (showFontDropdown) {
      const handler = (e: MouseEvent) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.dropdown-menu') && !target.closest('.setting-dropdown')) {
          showFontDropdown = false;
        }
      };
      const dismissOnScroll = () => { showFontDropdown = false; };
      const timer = setTimeout(() => document.addEventListener('click', handler), 0);
      window.addEventListener('scroll', dismissOnScroll, { capture: true, passive: true });
      window.addEventListener('resize', dismissOnScroll, { passive: true });
      return () => {
        clearTimeout(timer);
        document.removeEventListener('click', handler);
        window.removeEventListener('scroll', dismissOnScroll, { capture: true } as EventListenerOptions);
        window.removeEventListener('resize', dismissOnScroll);
      };
    }
  });

  function selectFontSize(size: string) {
    fontSize = size;
    showFontDropdown = false;
    success(`Font size set to ${size}`);
  }

  /** Export all conversations + characters as a JSON file */
  async function handleExport() {
    if (!isTauri) return;
    isExporting = true;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ipc = await import('$lib/services/ipc');

      const conversations = await ipc.listConversations();
      const characters = await ipc.listCharacters();

      const exportData = {
        version: '1.0',
        exportedAt: new Date().toISOString(),
        conversations,
        characters,
        settings: $settings,
      };

      const savePath = await save({
        filters: [{ name: 'Mythic Export', extensions: ['json'] }],
        defaultPath: `mythic-export-${Date.now()}.json`,
      });

      if (savePath) {
        const { writeTextFile } = await import('@tauri-apps/plugin-fs');
        await writeTextFile(savePath, JSON.stringify(exportData, null, 2));
        success('Data exported successfully');
      }
    } catch (err) {
      toastError('Failed to export data');
      console.error('Export failed:', err);
    }
    isExporting = false;
  }

  /** Import data from a previously exported JSON file */
  async function handleImport() {
    if (!isTauri) return;
    isImporting = true;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Mythic Export', extensions: ['json'] }],
      });

      if (selected) {
        const { readTextFile } = await import('@tauri-apps/plugin-fs');
        const raw = await readTextFile(selected as string);
        const data = JSON.parse(raw);

        if (data.settings) {
          settings.set({ ...$settings, ...data.settings });
          theme = $settings.theme;
          fontSize = $settings.fontSize;
          streamingEnabled = $settings.streamingEnabled;
          autoGenerateImages = $settings.autoGenerateImages;
          autoSaveMemories = $settings.autoSaveMemories;
          localStorageOnly = $settings.localStorageOnly;
          systemPrompt = $settings.systemPrompt;
          postHistoryInstructions = $settings.postHistoryInstructions;
        }

        success('Settings imported successfully');
        toastInfo(`Found ${data.conversations?.length ?? 0} conversations, ${data.characters?.length ?? 0} characters`);
      }
    } catch (err) {
      toastError('Failed to import data');
      console.error('Import failed:', err);
    }
    isImporting = false;
  }

  /** Clear all conversations after user confirmation */
  async function clearAllConversations() {
    if (!isTauri) { showClearConfirm = false; return; }
    try {
      const ipc = await import('$lib/services/ipc');
      const convs = await ipc.listConversations();
      let cleared = 0;
      for (const conv of convs) {
        await ipc.deleteConversation(conv.id);
        cleared++;
      }
      showClearConfirm = false;
      success(`Cleared ${cleared} conversation${cleared !== 1 ? 's' : ''}`);
    } catch (err) {
      toastError('Failed to clear conversations');
      console.error('Clear failed:', err);
    }
  }
</script>

<svelte:head>
  <title>Settings — Mythic</title>
</svelte:head>

<div class="settings-page">
  <!-- Header -->
  <header class="settings-header">
    <h1 class="settings-title">Settings</h1>
    <span class="settings-subtitle">Customize your Mythic experience</span>
  </header>

  <!-- Two Column Layout -->
  <div class="settings-grid">
    <!-- Left Column -->
    <div class="settings-column">
      <!-- Appearance -->
      <section class="settings-section animate-fade-in-up stagger-1">
        <div class="section-header">
          <Icon name="palette" size={16} color="var(--accent-primary)" />
          <span class="section-title">Appearance</span>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Theme</span>
            <span class="setting-desc">Choose your color scheme</span>
          </div>
          <div class="theme-toggle">
            <button 
              class="theme-btn" 
              class:active={theme === 'dark'}
              onclick={() => theme = 'dark'}
            >Dark</button>
            <button 
              class="theme-btn" 
              class:active={theme === 'light'}
              onclick={() => theme = 'light'}
            >Light</button>
            <button 
              class="theme-btn" 
              class:active={theme === 'system'}
              onclick={() => theme = 'system'}
            >System</button>
          </div>
        </div>

        <div class="setting-row">
          <span class="setting-name">Font Size</span>
          <div class="font-dropdown-wrapper">
            <button class="setting-dropdown" onclick={toggleFontDropdown}>
              <span>{fontSize}</span>
              <Icon name="chevron-down" size={12} color="var(--fg-muted)" />
            </button>
          </div>
        </div>
      </section>

      <!-- Chat Behavior -->
      <section class="settings-section animate-fade-in-up stagger-2">
        <div class="section-header">
          <Icon name="message-circle" size={16} color="var(--accent-primary)" />
          <span class="section-title">Chat Behavior</span>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Streaming Responses</span>
            <span class="setting-desc">{streamingEnabled ? 'Text appears word-by-word as it generates' : 'Full response appears at once when complete'}</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={streamingEnabled}
            onclick={() => {
              streamingEnabled = !streamingEnabled;
              success(streamingEnabled ? 'Streaming enabled — responses stream in real-time' : 'Streaming disabled — responses appear when complete');
            }}
            role="switch"
            aria-checked={streamingEnabled}
            aria-label="Toggle streaming responses"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Auto-Generate Scene Images</span>
            <span class="setting-desc">Generate images from scene context</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={autoGenerateImages}
            onclick={() => autoGenerateImages = !autoGenerateImages}
            role="switch"
            aria-checked={autoGenerateImages}
            aria-label="Toggle auto-generate scene images"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Auto-Save Memories</span>
            <span class="setting-desc">{autoSaveMemories ? 'Key events are extracted every few messages' : 'Memories are only saved when pinned manually'}</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={autoSaveMemories}
            onclick={() => {
              autoSaveMemories = !autoSaveMemories;
              success(autoSaveMemories ? 'Auto-save enabled — key events will be remembered' : 'Auto-save disabled');
            }}
            role="switch"
            aria-checked={autoSaveMemories}
            aria-label="Toggle auto-save memories"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
      </section>

      <!-- Context Management -->
      <section class="settings-section animate-fade-in-up stagger-2b">
        <div class="section-header">
          <Icon name="layers" size={16} color="var(--accent-primary)" />
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
      <section class="settings-section animate-fade-in-up stagger-2c">
        <div class="section-header">
          <Icon name="database" size={16} color="var(--accent-primary)" />
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
            <!-- Embedding Model (read-only — configured in Embedders page) -->
            <div class="setting-row">
              <div class="setting-label">
                <span class="setting-name">Embedder Model</span>
                <span class="setting-desc">Configured in <a href="/embedders" class="settings-link">AI Studio → Embedders</a></span>
              </div>
              <div class="font-dropdown-wrapper">
                <span class="setting-value-readonly mono">{ragEmbeddingModel || 'Not configured'}</span>
              </div>
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
                <button class="index-refresh-btn" onclick={loadIndexStatus} disabled={isLoadingIndex}>
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

                <!-- Rebuild Warning -->
                {#if indexStatus.needs_rebuild}
                  <div class="rebuild-warning">
                    <Icon name="alert-triangle" size={13} color="#F59E0B" />
                    <div class="rebuild-text">
                      <span class="rebuild-title">Model Mismatch</span>
                      <span class="rebuild-desc">
                        Index built with <code>{indexStatus.index_model}</code>, but <code>{ragEmbeddingModel}</code> is selected.
                        Rebuild to use the new model.
                      </span>
                    </div>
                  </div>
                {/if}

                <!-- Rebuild Button -->
                <button
                  class="rebuild-btn"
                  class:rebuilding={isRebuilding}
                  onclick={rebuildIndex}
                  disabled={isRebuilding}
                >
                  {#if isRebuilding}
                    <div class="btn-spinner"></div>
                    Rebuilding...
                  {:else}
                    <Icon name="refresh-cw" size={13} color="#e0e0f0" />
                    {indexStatus.needs_rebuild ? 'Rebuild Index' : 'Rebuild Index'}
                  {/if}
                </button>
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
    </div>

    <!-- Right Column -->
    <div class="settings-column">
      <!-- Data & Privacy -->
      <section class="settings-section animate-fade-in-up stagger-3">
        <div class="section-header">
          <Icon name="shield" size={16} color="var(--accent-primary)" />
          <span class="section-title">Data & Privacy</span>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Local Storage Only</span>
            <span class="setting-desc">All data stays on your device — no cloud sync or telemetry</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={localStorageOnly}
            onclick={() => {
              if (localStorageOnly) {
                // Turning OFF privacy mode — confirm
                showPrivacyConfirm = true;
              } else {
                localStorageOnly = true;
                success('Privacy mode enabled — all data stays local');
              }
            }}
            role="switch"
            aria-checked={localStorageOnly}
            aria-label="Toggle local storage only"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        <div class="button-row">
          <button class="settings-btn outline" onclick={handleExport} disabled={isExporting}>
            <Icon name="download" size={14} color="var(--fg-secondary)" />
            <span>{isExporting ? 'Exporting...' : 'Export Data'}</span>
          </button>
          <button class="settings-btn outline" onclick={handleImport} disabled={isImporting}>
            <Icon name="upload" size={14} color="var(--fg-secondary)" />
            <span>{isImporting ? 'Importing...' : 'Import Data'}</span>
          </button>
        </div>

        {#if showClearConfirm}
          <div class="clear-confirm">
            <span class="clear-warn">This will permanently delete all conversations. Are you sure?</span>
            <div class="button-row">
              <button class="settings-btn outline" onclick={() => showClearConfirm = false}>Cancel</button>
              <button class="settings-btn danger" onclick={clearAllConversations}>
                <Icon name="trash-2" size={14} color="var(--danger)" />
                <span>Yes, Delete All</span>
              </button>
            </div>
          </div>
        {:else}
          <button class="settings-btn danger" onclick={() => showClearConfirm = true}>
            <Icon name="trash-2" size={14} color="var(--danger)" />
            <span>Clear All Conversations</span>
          </button>
        {/if}

        {#if showPrivacyConfirm}
          <div class="clear-confirm">
            <span class="clear-warn">Disabling privacy mode may allow future cloud features to sync your data externally. Continue?</span>
            <div class="button-row">
              <button class="settings-btn outline" onclick={() => showPrivacyConfirm = false}>Keep Private</button>
              <button class="settings-btn danger" onclick={() => { localStorageOnly = false; showPrivacyConfirm = false; success('Privacy mode disabled'); }}>
                <Icon name="shield-off" size={14} color="var(--danger)" />
                <span>Disable Privacy</span>
              </button>
            </div>
          </div>
        {/if}
      </section>

      <!-- System Prompt -->
      <section class="settings-section animate-fade-in-up stagger-4">
        <div class="section-header">
          <div class="section-header-left">
            <Icon name="file-text" size={16} color="var(--accent-primary)" />
            <span class="section-title">Default System Prompt</span>
          </div>
          <button class="reset-btn" onclick={resetSystemPrompt}>Reset</button>
        </div>

        <textarea 
          class="system-prompt-input"
          bind:value={systemPrompt}
          rows="6"
          aria-label="Default system prompt"
        ></textarea>

        <span class="prompt-hint">Use {`{{char}}`} for character name • {`{{user}}`} for player name</span>
      </section>

      <!-- Post-History Instructions (PHI) -->
      <section class="settings-section animate-fade-in-up stagger-4b">
        <div class="section-header">
          <div class="section-header-left">
            <Icon name="compass" size={16} color="var(--accent-primary)" />
            <span class="section-title">Narrative Direction</span>
          </div>
          <button class="reset-btn" onclick={() => { settings.reset(); postHistoryInstructions = $settings.postHistoryInstructions; success('Narrative direction reset'); }}>Reset</button>
        </div>

        <span class="phi-description">Injected after conversation history to shape how the AI structures responses — narrative hooks, scene transitions, and pacing.</span>

        <textarea 
          class="system-prompt-input"
          bind:value={postHistoryInstructions}
          rows="6"
          aria-label="Post-history instructions"
        ></textarea>

        <span class="prompt-hint">Controls story momentum • scene transitions • prevents dead-end conversations</span>
      </section>

      <!-- About -->
      <div class="about-card animate-fade-in-up stagger-5">
        <div class="about-left">
          <span class="about-name">Mythic v0.1.0</span>
          <span class="about-desc">Open Source • Local First • {localStorageOnly ? '🔒 Private' : '⚠️ Privacy Relaxed'}</span>
        </div>
        <div class="about-links">
          <button class="about-link-btn" title="GitHub">
            <Icon name="github" size={16} color="var(--fg-secondary)" />
          </button>
          <button class="about-link-btn" title="Star on GitHub">
            <Icon name="star" size={16} color="var(--fg-secondary)" />
          </button>
        </div>
      </div>
    </div>
  </div>

  {#if showFontDropdown}
    <div class="dropdown-menu" style={dropdownStyle}>
      {#each fontSizes as size}
        <button class="dropdown-item" class:active={fontSize === size} onclick={() => selectFontSize(size)}>{size}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .settings-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
  }

  /* ── Header ── */
  .settings-header {
    display: flex; flex-direction: column; gap: 3px;
    padding: 20px 28px 18px; flex-shrink: 0; position: relative;
  }
  .settings-header::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .settings-title {
    font-size: var(--text-2xl); font-weight: 800; letter-spacing: -0.5px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
  }
  .settings-subtitle { font-size: var(--text-sm); color: #5a5a7a; letter-spacing: 0.3px; }

  /* ── Grid ── */
  .settings-grid {
    display: grid; grid-template-columns: 1fr 1fr;
    gap: 22px; padding: 28px; overflow-y: auto; flex: 1; align-items: start;
  }
  .settings-grid::-webkit-scrollbar { width: 4px; }
  .settings-grid::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }
  .settings-column { display: flex; flex-direction: column; gap: 20px; }

  /* ── Section Card ── */
  .settings-section {
    padding: 20px; border-radius: 16px;
    background: rgba(14,14,30,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    display: flex; flex-direction: column; gap: 16px;
    transition: border-color 200ms, box-shadow 250ms;
  }
  .settings-section:hover {
    border-color: rgba(139,92,246,0.1);
    box-shadow: 0 4px 20px rgba(0,0,0,0.2);
  }

  .section-header { display: flex; align-items: center; gap: 10px; }
  .section-header-left { display: flex; align-items: center; gap: 10px; flex: 1; }
  .section-title { font-size: var(--text-lg); font-weight: 700; color: #e8e0ff; }

  /* ── Setting Row ── */
  .setting-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 0;
  }
  .setting-label { display: flex; flex-direction: column; gap: 2px; }
  .setting-name { font-size: var(--text-md); color: #c8c8e0; font-weight: 500; }
  .setting-desc { font-size: var(--text-sm); color: #5a5a7a; }

  /* ── Theme Toggle ── */
  .theme-toggle {
    display: flex; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.1); overflow: hidden;
    background: rgba(14,14,30,0.4);
  }
  .theme-btn {
    padding: 6px 14px; background: transparent; border: none;
    color: #5a5a7a; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 200ms ease;
  }
  .theme-btn:hover { color: #8b8ba7; }
  .theme-btn.active {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    color: #fff;
    box-shadow: 0 2px 8px rgba(139,92,246,0.3);
  }

  /* ── Font Dropdown ── */
  .font-dropdown-wrapper { position: relative; }
  .setting-dropdown {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    width: 120px; height: 34px; padding: 0 12px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    font-size: 12px; font-weight: 600; font-family: var(--font-body);
    color: #e0e0f0; cursor: pointer; transition: border-color 200ms;
  }
  .setting-dropdown:hover { border-color: rgba(139,92,246,0.25); }
  .dropdown-menu {
    position: fixed; z-index: 50;
    background: linear-gradient(175deg, #12122a, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12); border-radius: 12px;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5); padding: 4px;
    display: flex; flex-direction: column;
  }
  .dropdown-item {
    padding: 7px 12px; border-radius: 8px; border: none; background: transparent;
    color: #8b8ba7; font-size: var(--text-sm); font-weight: 500;
    font-family: var(--font-body); text-align: left; cursor: pointer;
    transition: all 120ms;
  }
  .dropdown-item:hover { background: rgba(139,92,246,0.06); color: #e0e0f0; }
  .dropdown-item.active { color: #bf40ff; font-weight: 700; }

  /* ── Clear Confirm ── */
  .clear-confirm {
    display: flex; flex-direction: column; gap: 10px; padding: 12px;
    border-radius: 12px; background: rgba(244,63,94,0.04);
    border: 1px solid rgba(244,63,94,0.15);
  }
  .clear-warn { font-size: var(--text-sm); color: #F43F5E; line-height: 1.5; }

  /* ── Toggle Switch ── */
  .toggle-switch {
    width: 44px; height: 24px; border-radius: 99px;
    background: #2a2a4a; border: none; padding: 3px;
    display: flex; align-items: center; cursor: pointer;
    transition: background 250ms ease; flex-shrink: 0;
  }
  .toggle-switch.on {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    justify-content: flex-end;
    box-shadow: 0 0 10px rgba(139,92,246,0.3);
  }
  .toggle-knob {
    width: 18px; height: 18px; border-radius: 50%; background: #fff;
    transition: transform 250ms cubic-bezier(0.34,1.56,0.64,1);
    box-shadow: 0 1px 4px rgba(0,0,0,0.3);
  }

  /* ── Buttons ── */
  .button-row { display: flex; gap: 10px; }
  .settings-btn {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 9px 16px; border-radius: 10px; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); border: none; cursor: pointer;
    transition: all 180ms ease;
  }
  .settings-btn.outline {
    flex: 1; background: transparent;
    border: 1px solid rgba(139,92,246,0.12); color: #8b8ba7;
  }
  .settings-btn.outline:hover { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2); }
  .settings-btn.danger {
    background: rgba(244,63,94,0.06); border: 1px solid rgba(244,63,94,0.15);
    color: #F43F5E; width: 100%;
  }
  .settings-btn.danger:hover { background: rgba(244,63,94,0.12); }

  /* ── System Prompt ── */
  .system-prompt-input {
    width: 100%; min-height: 140px; padding: 14px 16px; border-radius: 12px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #c8c8e0; font-size: 12px; font-family: var(--font-body);
    line-height: 1.7; resize: vertical; outline: none;
    transition: border-color 200ms;
  }
  .system-prompt-input:focus { border-color: rgba(139,92,246,0.3); }
  .prompt-hint { font-size: 10px; color: #4a4a6a; font-family: var(--font-mono); }
  .reset-btn {
    background: none; border: none; cursor: pointer;
    color: #bf40ff; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); transition: opacity 150ms;
  }
  .reset-btn:hover { opacity: 0.7; }

  /* ── About Card ── */
  .about-card {
    display: flex; justify-content: space-between; align-items: center;
    padding: 14px 18px; border-radius: 14px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.06);
  }
  .about-left { display: flex; flex-direction: column; gap: 3px; }
  .about-name { font-size: var(--text-md); font-weight: 700; color: #e8e0ff; }
  .about-desc { font-size: 10px; color: #4a4a6a; font-family: var(--font-mono); letter-spacing: 0.5px; }
  .about-links { display: flex; gap: 8px; }
  .about-link-btn {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: all 150ms;
  }
  .about-link-btn:hover { background: rgba(139,92,246,0.06); }

  /* ── Responsive ── */
  @media (max-width: 768px) { .settings-grid { grid-template-columns: 1fr; } }

  /* ── Staggered Entrance ── */
  .animate-fade-in-up { animation: fadeInUp 400ms ease both; }
  .stagger-1 { animation-delay: 40ms; }
  .stagger-2 { animation-delay: 100ms; }
  .stagger-2b { animation-delay: 140ms; }
  .stagger-2c { animation-delay: 160ms; }
  .stagger-3 { animation-delay: 180ms; }
  .stagger-4 { animation-delay: 240ms; }
  .stagger-4b { animation-delay: 280ms; }
  .stagger-5 { animation-delay: 320ms; }

  .phi-description {
    font-size: var(--text-sm); color: #5a5a7a; line-height: 1.6;
  }
  @keyframes fadeInUp {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }
  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-12px); }
    to   { opacity: 1; transform: translateY(0); }
  }

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

  .setting-input {
    width: 220px; height: 34px; padding: 0 12px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    font-size: 12px; font-weight: 600; font-family: var(--font-body);
    color: #e0e0f0; outline: none; transition: border-color 200ms;
  }
  .setting-input:focus { border-color: rgba(139,92,246,0.3); }
  .setting-input.mono { font-family: var(--font-mono); }

  .setting-value-readonly {
    display: inline-flex; align-items: center;
    height: 34px; padding: 0 14px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px dashed rgba(139,92,246,0.1);
    font-size: 12px; font-weight: 600; color: #a78bfa;
    letter-spacing: 0.2px; user-select: all;
  }
  .setting-value-readonly.mono { font-family: var(--font-mono); }
  .settings-link {
    color: #8B5CF6; text-decoration: none; font-weight: 600;
    transition: color 150ms;
  }
  .settings-link:hover { color: #c4a1ff; text-decoration: underline; }

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
    background: linear-gradient(90deg, #8B5CF6, #bf40ff);
    transition: width 500ms cubic-bezier(0.34,1.56,0.64,1);
  }

  /* Rebuild Warning */
  .rebuild-warning {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 10px 14px; border-radius: 10px;
    background: rgba(245,158,11,0.06);
    border: 1px solid rgba(245,158,11,0.15);
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
