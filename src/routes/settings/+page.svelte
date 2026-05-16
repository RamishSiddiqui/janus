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

  let showFontDropdown = $state(false);
  let showClearConfirm = $state(false);
  let isExporting = $state(false);
  let isImporting = $state(false);

  const fontSizes = ['Small', 'Medium', 'Large'] as const;

  // Persist changes back to store
  $effect(() => {
    settings.set({
      theme,
      fontSize,
      streamingEnabled,
      autoGenerateImages,
      autoSaveMemories,
      localStorageOnly,
      systemPrompt,
    });
  });

  function resetSystemPrompt() {
    settings.reset();
    systemPrompt = $settings.systemPrompt;
    success('System prompt reset to default');
  }

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
            <button class="setting-dropdown" onclick={() => showFontDropdown = !showFontDropdown}>
              <span>{fontSize}</span>
              <Icon name="chevron-down" size={12} color="var(--fg-muted)" />
            </button>
            {#if showFontDropdown}
              <div class="dropdown-menu">
                {#each fontSizes as size}
                  <button class="dropdown-item" class:active={fontSize === size} onclick={() => selectFontSize(size)}>{size}</button>
                {/each}
              </div>
            {/if}
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
            <span class="setting-desc">Show text as it generates</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={streamingEnabled}
            onclick={() => streamingEnabled = !streamingEnabled}
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
            <span class="setting-desc">Automatically extract key events</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={autoSaveMemories}
            onclick={() => autoSaveMemories = !autoSaveMemories}
            role="switch"
            aria-checked={autoSaveMemories}
            aria-label="Toggle auto-save memories"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
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
            <span class="setting-desc">All data stays on your device</span>
          </div>
          <button 
            class="toggle-switch" 
            class:on={localStorageOnly}
            onclick={() => localStorageOnly = !localStorageOnly}
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

      <!-- About -->
      <div class="about-card animate-fade-in-up stagger-5">
        <div class="about-left">
          <span class="about-name">Mythic v0.1.0</span>
          <span class="about-desc">Open Source • Local First • Privacy Focused</span>
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
    color: #5a5a7a; font-size: 11px; font-weight: 600;
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
    position: absolute; top: 100%; right: 0; margin-top: 6px; width: 120px;
    background: linear-gradient(175deg, #12122a, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12); border-radius: 12px;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5); z-index: 50; padding: 4px;
    display: flex; flex-direction: column;
  }
  .dropdown-item {
    padding: 7px 12px; border-radius: 8px; border: none; background: transparent;
    color: #8b8ba7; font-size: 12px; font-weight: 500;
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
  .clear-warn { font-size: 12px; color: #F43F5E; line-height: 1.5; }

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
    padding: 9px 16px; border-radius: 10px; font-size: 12px; font-weight: 600;
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
    color: #bf40ff; font-size: 11px; font-weight: 600;
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
  .about-name { font-size: 13px; font-weight: 700; color: #e8e0ff; }
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
  .stagger-3 { animation-delay: 160ms; }
  .stagger-4 { animation-delay: 220ms; }
  .stagger-5 { animation-delay: 280ms; }
  @keyframes fadeInUp {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
