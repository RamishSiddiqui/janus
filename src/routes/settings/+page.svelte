<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '$lib/components/Icon.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import SelectCombobox from '$lib/components/SelectCombobox.svelte';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError, info as toastInfo } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { loadConversations } from '$lib/stores/chat';
  import { HORDE_SAMPLERS } from '$lib/constants/aiHorde';
  import { frontendLogs, formatFrontendLogsAsText, clearFrontendLogs } from '$lib/stores/logs';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // ── Sidebar navigation ──
  // Settings grew to 8 sections crammed into a two-column masonry layout
  // that kept getting more cluttered as features were added (Image Presets'
  // quality knobs, the reasoning toggle, etc). A single active-section panel
  // with sidebar nav (the VS Code / Linear / macOS System Settings pattern)
  // scales to any number of sections without the page just getting taller.
  type SettingsSection = 'appearance' | 'chat' | 'context' | 'privacy' | 'image' | 'prompts' | 'logging';
  let activeSection = $state<SettingsSection>('appearance');
  const NAV_ITEMS: { id: SettingsSection; label: string; icon: string; accent: string }[] = [
    { id: 'appearance', label: 'Appearance', icon: 'palette', accent: '#9075f2' },
    { id: 'chat', label: 'Chat Behavior', icon: 'message-circle', accent: '#22d3ee' },
    { id: 'context', label: 'Context & Memory', icon: 'network', accent: '#e879f9' },
    { id: 'image', label: 'Image Generation', icon: 'image', accent: '#fbbf24' },
    { id: 'prompts', label: 'Prompts', icon: 'file-text', accent: '#34d399' },
    { id: 'privacy', label: 'Data & Privacy', icon: 'shield', accent: '#fb7185' },
    { id: 'logging', label: 'Logging', icon: 'terminal', accent: '#94a3b8' },
  ];
  // Each section carries its own accent — driven into the panel below as a
  // CSS custom property, so every glass card/button/progress-bar re-tints
  // to match without hand-coding a per-section colour on each one.
  let sectionAccent = $derived(NAV_ITEMS.find(i => i.id === activeSection)?.accent ?? '#9075f2');

  // Bind to store values
  let theme = $state($settings.theme);
  let fontSize = $state($settings.fontSize);
  let streamingEnabled = $state($settings.streamingEnabled);
  let showThinking = $state($settings.showThinking);
  let autoGenerateImages = $state($settings.autoGenerateImages);
  let allowMatureContent = $state($settings.allowMatureContent);
  let autoGenerateNpcPortraits = $state($settings.autoGenerateNpcPortraits);
  let autoApproveNpcPortraits = $state($settings.autoApproveNpcPortraits);
  let autoSaveMemories = $state($settings.autoSaveMemories);
  let localStorageOnly = $state($settings.localStorageOnly);
  let systemPrompt = $state($settings.systemPrompt);
  let postHistoryInstructions = $state($settings.postHistoryInstructions);
  let profileRefreshPrompt = $state($settings.profileRefreshPrompt);
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
    loadImagePresets();
    loadEnabledImageModels();

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

  let showFontDropdown = $state(false);
  let dropdownStyle = $state('');
  let showClearConfirm = $state(false);
  let showPrivacyConfirm = $state(false);
  let isExporting = $state(false);
  let isImporting = $state(false);

  // --- Image Generation Presets ---

  interface PresetRow {
    id: string;
    name: string;
    model: string | null;
    sampler_name: string;
    cfg_scale: number;
    steps: number;
    karras: boolean;
    style: string | null;
    negative_prompt: string | null;
    is_default: boolean;
    clip_skip: number | null;
    post_processing: string[];
    hires_fix: boolean;
    hires_fix_denoising_strength: number | null;
    isExpanded?: boolean;
  }

  // Face-fixers and upscalers are both AI Horde `post_processing` entries —
  // split into two independent pickers here for a simpler UI, recombined
  // into the ordered array (face-fix first, then upscale) on save.
  const FACE_FIXERS = [
    { value: '', label: 'None' },
    { value: 'GFPGAN', label: 'GFPGAN' },
    { value: 'CodeFormers', label: 'CodeFormers' },
  ];
  const UPSCALERS = [
    { value: '', label: 'None' },
    { value: 'RealESRGAN_x4plus', label: 'RealESRGAN x4 (realistic)' },
    { value: 'RealESRGAN_x4plus_anime_6B', label: 'RealESRGAN x4 (anime)' },
    { value: '4x_AnimeSharp', label: '4x AnimeSharp' },
  ];
  function faceFixerOf(pp: string[]): string {
    return pp.find(v => v === 'GFPGAN' || v === 'CodeFormers') ?? '';
  }
  function upscalerOf(pp: string[]): string {
    return pp.find(v => v !== 'GFPGAN' && v !== 'CodeFormers') ?? '';
  }
  function composePostProcessing(faceFixer: string, upscaler: string): string[] {
    return [faceFixer, upscaler].filter(Boolean);
  }

  let imagePresets = $state<PresetRow[]>([]);
  let isLoadingPresets = $state(false);
  let showAddPresetForm = $state(false);
  let isSavingPreset = $state(false);

  // Models a preset's "Model" field can pick from — only image models the
  // user has explicitly enabled on the Models page (mirrors how chat model
  // selection works: enable it there first, pick it here).
  let enabledImageModels = $state<{ model_id: string; provider_name: string; description: string | null }[]>([]);

  // Community-favorite checkpoints (Civitai + AI Horde usage) worth surfacing
  // above the raw list — matched fuzzily since AI Horde's exact registered
  // name can vary slightly from the Civitai listing name.
  const RECOMMENDED_MODEL_PATTERNS = ['pony', 'aam', 'juggernaut', 'realvis', 'albedo'];
  function isRecommendedModel(modelId: string): boolean {
    const lower = modelId.toLowerCase();
    return RECOMMENDED_MODEL_PATTERNS.some(p => lower.includes(p));
  }

  /** Enabled models, plus the preset's current model tacked on if it's not
   *  (or no longer) in the enabled list — so an existing preset never shows
   *  a silently-blank dropdown just because its model got disabled later. */
  function modelOptionsFor(currentModel: string | null): { model_id: string; label: string }[] {
    const base = enabledImageModels.map(m => ({
      model_id: m.model_id,
      label: (isRecommendedModel(m.model_id) ? '★ ' : '') + m.model_id + (m.description ? ` (${m.description})` : ''),
    })).sort((a, b) => {
      const ra = isRecommendedModel(a.model_id), rb = isRecommendedModel(b.model_id);
      if (ra !== rb) return ra ? -1 : 1;
      return a.model_id.localeCompare(b.model_id);
    });
    if (currentModel && !enabledImageModels.some(m => m.model_id === currentModel)) {
      base.unshift({ model_id: currentModel, label: `${currentModel} (not currently enabled)` });
    }
    return base;
  }

  async function loadEnabledImageModels() {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const models = await ipc.listEnabledModels();
      enabledImageModels = models
        .filter(m => m.model_type === 'image')
        .map(m => ({ model_id: m.model_id, provider_name: m.provider_name, description: m.description }));
    } catch (err) {
      console.error('[Settings] Failed to load enabled image models:', err);
    }
  }

  let newPresetName = $state('');
  let newPresetModel = $state('');
  let newPresetSampler = $state('k_euler_a');
  let newPresetCfgScale = $state(7.5);
  let newPresetSteps = $state(30);
  let newPresetKarras = $state(true);
  let newPresetStyle = $state('');
  let newPresetNegativePrompt = $state('');
  let newPresetClipSkip = $state<number | null>(null);
  let newPresetFaceFixer = $state('');
  let newPresetUpscaler = $state('');
  let newPresetHiresFix = $state(false);
  let newPresetHiresFixDenoising = $state(0.65);

  async function loadImagePresets() {
    if (!isTauri) return;
    isLoadingPresets = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const rows = await ipc.listImagePresets();
      imagePresets = rows.map(p => ({
        id: p.id, name: p.name, model: p.model, sampler_name: p.sampler_name,
        cfg_scale: p.cfg_scale ?? 7.5, steps: p.steps, karras: p.karras,
        style: p.style, negative_prompt: p.negative_prompt, is_default: p.is_default,
        clip_skip: p.clip_skip ?? null, post_processing: p.post_processing ?? [],
        hires_fix: p.hires_fix ?? false, hires_fix_denoising_strength: p.hires_fix_denoising_strength ?? null,
      }));
    } catch (err) { console.error('[Settings] Failed to load image presets:', err); }
    isLoadingPresets = false;
  }

  async function addImagePreset() {
    if (!newPresetName.trim() || !isTauri) return;
    isSavingPreset = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const p = await ipc.createImagePreset(newPresetName, {
        model: newPresetModel || undefined,
        samplerName: newPresetSampler,
        cfgScale: newPresetCfgScale,
        steps: newPresetSteps,
        karras: newPresetKarras,
        style: newPresetStyle || undefined,
        negativePrompt: newPresetNegativePrompt || undefined,
        isDefault: imagePresets.length === 0,
        clipSkip: newPresetClipSkip ?? undefined,
        postProcessing: composePostProcessing(newPresetFaceFixer, newPresetUpscaler),
        hiresFix: newPresetHiresFix,
        hiresFixDenoisingStrength: newPresetHiresFix ? newPresetHiresFixDenoising : undefined,
      });
      imagePresets = [...imagePresets, {
        id: p.id, name: p.name, model: p.model, sampler_name: p.sampler_name,
        cfg_scale: p.cfg_scale ?? 7.5, steps: p.steps, karras: p.karras,
        style: p.style, negative_prompt: p.negative_prompt, is_default: p.is_default,
        clip_skip: p.clip_skip ?? null, post_processing: p.post_processing ?? [],
        hires_fix: p.hires_fix ?? false, hires_fix_denoising_strength: p.hires_fix_denoising_strength ?? null,
      }];
      if (p.is_default) imagePresets = imagePresets.map(r => ({ ...r, is_default: r.id === p.id }));
      showAddPresetForm = false;
      newPresetName = ''; newPresetModel = ''; newPresetStyle = ''; newPresetNegativePrompt = '';
      newPresetSampler = 'k_euler_a'; newPresetCfgScale = 7.5; newPresetSteps = 30; newPresetKarras = true;
      newPresetClipSkip = null; newPresetFaceFixer = ''; newPresetUpscaler = '';
      newPresetHiresFix = false; newPresetHiresFixDenoising = 0.65;
      success(`Added preset "${p.name}"`);
    } catch (err) { toastError('Failed to add preset'); console.error(err); }
    isSavingPreset = false;
  }

  async function savePresetField(p: PresetRow, field: string, value: string | number | boolean | string[]) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.updateImagePreset(p.id, { [field]: value } as Record<string, unknown>);
    } catch (err) { toastError('Failed to save preset'); console.error(err); }
  }

  async function setDefaultPreset(p: PresetRow) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.setDefaultImagePreset(p.id);
      imagePresets = imagePresets.map(r => ({ ...r, is_default: r.id === p.id }));
      success(`"${p.name}" set as default preset`);
    } catch (err) { toastError('Failed to set default preset'); console.error(err); }
  }

  async function deleteImagePresetRow(p: PresetRow) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteImagePreset(p.id);
      imagePresets = imagePresets.filter(r => r.id !== p.id);
      success(`Deleted preset "${p.name}"`);
    } catch (err) { toastError('Failed to delete preset'); console.error(err); }
  }

  const fontSizes = ['Small', 'Medium', 'Large'] as const;

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    // Read all reactive locals to track them
    const snapshot = {
      theme,
      fontSize,
      streamingEnabled,
      showThinking,
      autoGenerateImages,
      allowMatureContent,
      autoGenerateNpcPortraits,
      autoApproveNpcPortraits,
      autoSaveMemories,
      localStorageOnly,
      systemPrompt,
      postHistoryInstructions,
      profileRefreshPrompt,
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
    profileRefreshPrompt = $settings.profileRefreshPrompt;
    success('Prompts reset to defaults');
  }

  // ── Logging ──
  // Backend lines are paged in from the log file (newest page first, older
  // pages prepended as the user scrolls up) instead of reading the whole
  // file at once — a long-running session's log can grow large, and there's
  // no reason to hold all of it in memory just to show the tail.
  const BACKEND_LOG_PAGE_SIZE = 150;

  let logSubTab = $state<'backend' | 'frontend'>('backend');
  let backendLogLines = $state<string[]>([]);
  let backendLogCursor = $state<number | null>(null);
  let backendLogPath = $state('');
  let isLoadingBackendLogs = $state(false);
  let isLoadingMoreBackendLogs = $state(false);
  let isExportingLogs = $state(false);
  let logSearch = $state('');
  let logViewerEl = $state<HTMLDivElement | undefined>(undefined);

  async function scrollLogViewerToBottom() {
    await tick();
    if (logViewerEl) logViewerEl.scrollTop = logViewerEl.scrollHeight;
  }

  async function loadBackendLogs() {
    if (!isTauri) return;
    isLoadingBackendLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const page = await ipc.getBackendLogsPage(undefined, BACKEND_LOG_PAGE_SIZE);
      backendLogLines = page.lines;
      backendLogCursor = page.nextCursor;
      if (!backendLogPath) backendLogPath = await ipc.getBackendLogPath();
    } catch {
      toastError('Failed to load backend logs');
    }
    isLoadingBackendLogs = false;
    scrollLogViewerToBottom();
  }

  async function loadOlderBackendLogs() {
    if (!isTauri || backendLogCursor == null || isLoadingMoreBackendLogs) return;
    isLoadingMoreBackendLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const page = await ipc.getBackendLogsPage(backendLogCursor, BACKEND_LOG_PAGE_SIZE);
      const el = logViewerEl;
      const prevScrollHeight = el?.scrollHeight ?? 0;
      const prevScrollTop = el?.scrollTop ?? 0;
      backendLogLines = [...page.lines, ...backendLogLines];
      backendLogCursor = page.nextCursor;
      await tick();
      // Loading older lines prepends above the current view — restore the
      // reader's position relative to the content they were looking at,
      // instead of letting the prepend silently scroll them to the top.
      if (el) el.scrollTop = prevScrollTop + (el.scrollHeight - prevScrollHeight);
    } catch {
      toastError('Failed to load more logs');
    }
    isLoadingMoreBackendLogs = false;
  }

  function handleLogViewerScroll() {
    if (logSubTab !== 'backend' || !logViewerEl) return;
    if (logViewerEl.scrollTop < 100 && backendLogCursor != null && !isLoadingMoreBackendLogs) {
      loadOlderBackendLogs();
    }
  }

  function selectLogSubTab(tab: 'backend' | 'frontend') {
    logSubTab = tab;
    scrollLogViewerToBottom();
  }

  // Load once when the Logging tab is first opened, not on every render —
  // but re-scroll to bottom every time it's revisited, since the panel is
  // torn down and remounted when the user navigates away and back.
  let hasLoadedBackendLogs = false;
  $effect(() => {
    if (activeSection === 'logging') {
      if (isTauri && !hasLoadedBackendLogs) {
        hasLoadedBackendLogs = true;
        loadBackendLogs();
      } else {
        scrollLogViewerToBottom();
      }
    }
  });

  let filteredBackendLines = $derived(
    logSearch.trim()
      ? backendLogLines.filter(l => l.toLowerCase().includes(logSearch.trim().toLowerCase()))
      : backendLogLines
  );
  let filteredFrontendEntries = $derived(
    logSearch.trim()
      ? $frontendLogs.filter(e => e.message.toLowerCase().includes(logSearch.trim().toLowerCase()))
      : $frontendLogs
  );

  function backendLogLineClass(line: string): string {
    if (/\bERROR\b/.test(line)) return 'log-line-error';
    if (/\bWARN\b/.test(line)) return 'log-line-warn';
    if (/\bDEBUG\b/.test(line)) return 'log-line-debug';
    return '';
  }

  async function handleExportLogs() {
    isExportingLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { writeTextFile } = await import('@tauri-apps/plugin-fs');
      const fullBackendLog = await ipc.getBackendLogs(5000);
      const combined = [
        `Janus log export — ${new Date().toISOString()}`,
        '',
        '===== BACKEND LOG =====',
        fullBackendLog || '(empty)',
        '',
        '===== FRONTEND LOG =====',
        formatFrontendLogsAsText($frontendLogs) || '(empty)',
      ].join('\n');
      const savePath = await save({
        filters: [{ name: 'Log File', extensions: ['log', 'txt'] }],
        defaultPath: `mythic-logs-${Date.now()}.log`,
      });
      if (savePath) {
        await writeTextFile(savePath, combined);
        success('Logs exported');
      }
    } catch (err) {
      toastError('Failed to export logs');
      console.error('Log export failed:', err);
    }
    isExportingLogs = false;
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

  // Live progress line shown under the Export/Import buttons while busy —
  // both operations can involve hundreds of sequential IPC calls for a
  // large library, so silence for 10+ seconds would look hung.
  let backupStatus = $state('');

  /**
   * Exports the full local library as a self-contained JSON backup:
   * every character (+ its lorebook), every conversation (+ its full
   * message history and group-cast roster), and every pinned/canon memory.
   * Settings are included too, matching the pre-existing behavior.
   *
   * list_conversations caps at 200/page server-side, so this paginates
   * rather than assuming a single call returns everything (the previous
   * version silently exported only the 50 most recent conversations).
   * Memories are fetched per-character rather than via the no-args
   * list_memories call, which caps at 100 rows globally.
   */
  async function handleExport() {
    if (!isTauri) return;
    isExporting = true;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ipc = await import('$lib/services/ipc');

      backupStatus = 'Gathering conversations...';
      const conversations: Awaited<ReturnType<typeof ipc.listConversations>> = [];
      let offset = 0;
      while (true) {
        const page = await ipc.listConversations(200, offset);
        conversations.push(...page);
        if (page.length < 200) break;
        offset += 200;
      }

      const characters = await ipc.listCharacters();

      const messagesByConversation: Record<string, unknown> = {};
      const castByConversation: Record<string, unknown> = {};
      for (const [i, conv] of conversations.entries()) {
        backupStatus = `Exporting conversation ${i + 1}/${conversations.length}...`;
        messagesByConversation[conv.id] = await ipc.getConversationMessages(conv.id);
        try {
          castByConversation[conv.id] = await ipc.listConversationCharacters(conv.id);
        } catch {
          castByConversation[conv.id] = [];
        }
      }

      const lorebookByCharacter: Record<string, unknown> = {};
      const memoriesByCharacter: Record<string, unknown> = {};
      for (const [i, char] of characters.entries()) {
        backupStatus = `Exporting character ${i + 1}/${characters.length}...`;
        lorebookByCharacter[char.id] = await ipc.listLorebookEntries(char.id);
        // Per-character (not the no-args global call) so this isn't capped at 100 rows.
        memoriesByCharacter[char.id] = await ipc.listMemories(char.id);
      }

      const exportData = {
        version: '2.0',
        exportedAt: new Date().toISOString(),
        settings: $settings,
        characters,
        lorebookByCharacter,
        conversations,
        messagesByConversation,
        castByConversation,
        memoriesByCharacter,
      };

      backupStatus = '';
      const savePath = await save({
        filters: [{ name: 'Janus Export', extensions: ['json'] }],
        defaultPath: `mythic-export-${Date.now()}.json`,
      });

      if (savePath) {
        const { writeTextFile } = await import('@tauri-apps/plugin-fs');
        await writeTextFile(savePath, JSON.stringify(exportData, null, 2));
        success(`Exported ${characters.length} characters, ${conversations.length} conversations`);
      }
    } catch (err) {
      toastError('Failed to export data');
      console.error('Export failed:', err);
    }
    backupStatus = '';
    isExporting = false;
  }

  /**
   * Restores a backup written by handleExport. Every record is recreated
   * with a fresh ID (never reusing the file's original IDs), so this is
   * additive and safe to run against a non-empty library — nothing existing
   * is touched or overwritten. All cross-references (character_id,
   * conversation_id, parent_id, ...) are remapped through old→new ID maps
   * built up as each record is created.
   *
   * Recreates, in dependency order: characters → lorebook entries →
   * conversations → messages (oldest-first per conversation, so a message's
   * parent always exists before it does, then the active/tip pointer is
   * restored) → group-cast rosters → memories (promoting canon ones and
   * restoring non-default importance afterward, since create_memory takes
   * neither directly).
   *
   * Branch-to-branch links (parent_conversation_id / branch_point_message_id)
   * are intentionally NOT restored — each conversation comes back as a
   * flattened, standalone conversation with its own message tree intact.
   * Reconstructing cross-conversation branch ancestry correctly would need
   * import ordered by original creation time with forward-reference
   * handling for branches created from each other, which isn't worth the
   * complexity for a backup/restore feature.
   *
   * Files exported before this fix (version "1.0") only ever contained
   * settings — nothing else to restore from those.
   */
  async function handleImport() {
    if (!isTauri) return;
    isImporting = true;
    backupStatus = '';
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Janus Export', extensions: ['json'] }],
      });
      if (!selected) { isImporting = false; return; }

      const { readTextFile } = await import('@tauri-apps/plugin-fs');
      const raw = await readTextFile(selected as string);
      const data = JSON.parse(raw);
      const ipc = await import('$lib/services/ipc');

      if (data.settings) {
        settings.set({ ...$settings, ...data.settings });
        theme = $settings.theme;
        fontSize = $settings.fontSize;
        streamingEnabled = $settings.streamingEnabled;
        showThinking = $settings.showThinking;
        autoGenerateImages = $settings.autoGenerateImages;
        allowMatureContent = $settings.allowMatureContent;
        autoGenerateNpcPortraits = $settings.autoGenerateNpcPortraits;
        autoApproveNpcPortraits = $settings.autoApproveNpcPortraits;
        autoSaveMemories = $settings.autoSaveMemories;
        localStorageOnly = $settings.localStorageOnly;
        systemPrompt = $settings.systemPrompt;
        postHistoryInstructions = $settings.postHistoryInstructions;
        profileRefreshPrompt = $settings.profileRefreshPrompt;
      }

      if (data.version !== '2.0' || !Array.isArray(data.characters)) {
        success('Settings imported successfully');
        isImporting = false;
        return;
      }

      const charIdMap = new Map<string, string>();
      const convIdMap = new Map<string, string>();
      let charsOk = 0, charsFailed = 0;
      let loreOk = 0, loreFailed = 0;
      let convsOk = 0, convsFailed = 0;
      let msgsOk = 0, msgsFailed = 0;
      let castFailed = 0;
      let memOk = 0, memFailed = 0;

      // 1. Characters
      for (const [i, char] of data.characters.entries()) {
        backupStatus = `Importing character ${i + 1}/${data.characters.length}...`;
        try {
          const created = await ipc.createCharacter(char.name, char.data as Record<string, unknown>);
          charIdMap.set(char.id, created.id);
          charsOk++;
        } catch (err) {
          charsFailed++;
          console.error('Import: failed to create character', char.name, err);
        }
      }

      // 2. Lorebook entries, keyed by the original character id
      const lorebookByChar: Record<string, any[]> = data.lorebookByCharacter ?? {};
      for (const [oldCharId, entries] of Object.entries(lorebookByChar)) {
        const newCharId = charIdMap.get(oldCharId);
        if (!newCharId) continue;
        for (const entry of entries) {
          try {
            await ipc.createLorebookEntry(
              newCharId,
              entry.name ?? entry.title ?? '',
              entry.keys ?? [],
              entry.content ?? '',
              entry.always_active ?? entry.alwaysActive ?? false,
            );
            loreOk++;
          } catch (err) {
            loreFailed++;
            console.error('Import: failed to create lorebook entry', err);
          }
        }
      }

      // 3. Conversations (flattened — see doc comment above on branch links)
      const conversations: any[] = data.conversations ?? [];
      for (const [i, conv] of conversations.entries()) {
        backupStatus = `Importing conversation ${i + 1}/${conversations.length}...`;
        const newCharId = conv.character_id ? charIdMap.get(conv.character_id) : undefined;
        try {
          const created = await ipc.createConversation(newCharId, conv.title);
          convIdMap.set(conv.id, created.id);
          convsOk++;
          if (conv.memory_scope && conv.memory_scope !== 'character') {
            await ipc.setMemoryScope(created.id, conv.memory_scope);
          }
        } catch (err) {
          convsFailed++;
          console.error('Import: failed to create conversation', conv.title, err);
        }
      }

      // 4. Messages, oldest-first per conversation so parents exist before children
      const messagesByConv: Record<string, any[]> = data.messagesByConversation ?? {};
      for (const [oldConvId, msgs] of Object.entries(messagesByConv)) {
        const newConvId = convIdMap.get(oldConvId);
        if (!newConvId) continue;
        const msgIdMap = new Map<string, string>();
        const sorted = [...msgs].sort((a, b) => (a.created_at ?? '').localeCompare(b.created_at ?? ''));
        let lastCreatedId: string | undefined;
        for (const msg of sorted) {
          const newParentId = msg.parent_id ? msgIdMap.get(msg.parent_id) : undefined;
          try {
            const created = await ipc.createMessage(newConvId, msg.role, msg.content, newParentId, msg.metadata ?? undefined);
            msgIdMap.set(msg.id, created.id);
            lastCreatedId = created.id;
            msgsOk++;
          } catch (err) {
            msgsFailed++;
            console.error('Import: failed to create message', err);
          }
        }
        // Restore the active/tip pointer (falls back to the last message created)
        const origConv = conversations.find((c) => c.id === oldConvId);
        const activeNewId = (origConv?.active_message_id && msgIdMap.get(origConv.active_message_id)) || lastCreatedId;
        if (activeNewId) {
          try { await ipc.setActiveMessage(newConvId, activeNewId); } catch { /* non-fatal */ }
        }
      }

      // 5. Group cast rosters (multi-character conversations)
      const castByConv: Record<string, any[]> = data.castByConversation ?? {};
      for (const [oldConvId, cast] of Object.entries(castByConv)) {
        const newConvId = convIdMap.get(oldConvId);
        if (!newConvId) continue;
        for (const member of cast) {
          const newMemberCharId = charIdMap.get(member.character_id);
          if (!newMemberCharId) continue;
          try {
            await ipc.addConversationCharacter(newConvId, newMemberCharId, member.character_name, member.role, member.talkativeness);
            if (member.is_active === false) {
              await ipc.toggleCharacterActive(newConvId, newMemberCharId, false);
            }
          } catch (err) {
            castFailed++;
            console.error('Import: failed to add group cast member', err);
          }
        }
      }

      // 6. Memories (canon status and non-default importance need follow-up calls —
      // create_memory takes neither directly)
      const memoriesByChar: Record<string, any[]> = data.memoriesByCharacter ?? {};
      for (const [oldCharId, mems] of Object.entries(memoriesByChar)) {
        const newCharId = charIdMap.get(oldCharId);
        if (!newCharId) continue;
        for (const mem of mems) {
          const newConvId = mem.conversation_id ? convIdMap.get(mem.conversation_id) : undefined;
          try {
            const created = await ipc.createMemory(mem.content, newCharId, newConvId, mem.source);
            if (mem.is_canon) await ipc.promoteToCanon(created.id);
            if (typeof mem.importance === 'number' && mem.importance !== 5) {
              await ipc.setMemoryImportance(created.id, mem.importance);
            }
            memOk++;
          } catch (err) {
            memFailed++;
            console.error('Import: failed to create memory', err);
          }
        }
      }

      await loadConversations();

      const failed = charsFailed + loreFailed + convsFailed + msgsFailed + castFailed + memFailed;
      success(`Imported ${charsOk} characters, ${convsOk} conversations, ${msgsOk} messages, ${loreOk} lorebook entries, ${memOk} memories`);
      if (failed > 0) {
        toastError(`${failed} item(s) failed to import — check the console for details`);
      }
    } catch (err) {
      toastError('Failed to import data');
      console.error('Import failed:', err);
    }
    backupStatus = '';
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
  <title>Settings — Janus</title>
</svelte:head>

<div class="settings-page" style="--accent: {sectionAccent}">
  <!-- Header -->
  <header class="settings-header">
    <div class="settings-header-left">
      <h1 class="settings-title"><SplitHeading text="Settings" /></h1>
      <span class="settings-subtitle">Customize your Janus experience</span>
    </div>
    <div class="settings-header-about">
      <span class="about-name">Janus v0.1.0</span>
      <span class="about-dot" aria-hidden="true">·</span>
      <span class="about-desc">{localStorageOnly ? '🔒 Private' : '⚠️ Privacy Relaxed'}</span>
      <button class="about-link-btn" title="GitHub">
        <Icon name="github" size={14} color="var(--fg-secondary)" />
      </button>
      <button class="about-link-btn" title="Star on GitHub">
        <Icon name="star" size={14} color="var(--fg-secondary)" />
      </button>
    </div>
  </header>

  <!-- Section nav — a floating carousel of chips instead of a second
       sidebar (the app's own nav rail is already the one persistent rail).
       Each chip carries its own accent; the active one steps forward and
       lights up while the rest recede, and that same accent drives the
       glass panel below via --accent. -->
  <div class="settings-carousel" role="tablist" aria-label="Settings sections">
    {#each NAV_ITEMS as item (item.id)}
      <button
        class="carousel-chip"
        class:active={activeSection === item.id}
        style="--chip-accent: {item.accent}"
        onclick={() => activeSection = item.id}
        role="tab"
        aria-selected={activeSection === item.id}
      >
        <Icon name={item.icon} size={13} color={activeSection === item.id ? '#0a0812' : 'var(--fg-muted)'} />
        <span>{item.label}</span>
      </button>
    {/each}
  </div>

  <div class="settings-body">
    {#key activeSection}
    <div class="settings-panel">
    <div class="panel-glow" aria-hidden="true"></div>
    {#if activeSection === 'appearance'}
      <div class="panel-heading animate-fade-in-up stagger-1">
        <span class="panel-heading-title">Appearance</span>
        <span class="panel-heading-desc">How Janus looks on your screen</span>
      </div>
      <div class="settings-card-grid animate-fade-in-up stagger-1">
        <div class="settings-card">
          <div class="settings-card-icon"><Icon name="palette" size={18} color="var(--accent-primary)" /></div>
          <span class="settings-card-name">Theme</span>
          <span class="settings-card-desc">Choose your color scheme</span>
          <div class="theme-toggle theme-toggle-lg">
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

        <div class="settings-card">
          <div class="settings-card-icon"><Icon name="file-text" size={18} color="var(--accent-primary)" /></div>
          <span class="settings-card-name">Font Size</span>
          <span class="settings-card-desc">Adjust text size across the app</span>
          <div class="font-dropdown-wrapper">
            <button class="setting-dropdown setting-dropdown-lg" onclick={toggleFontDropdown}>
              <span>{fontSize}</span>
              <Icon name="chevron-down" size={13} color="var(--fg-muted)" />
            </button>
          </div>
        </div>
      </div>
    {/if}

    {#if activeSection === 'chat'}
      <div class="panel-heading animate-fade-in-up stagger-2">
        <span class="panel-heading-title">Chat Behavior</span>
        <span class="panel-heading-desc">How responses stream in and what runs automatically after each message</span>
      </div>
      <div class="settings-toggle-grid animate-fade-in-up stagger-2">
        <div class="toggle-card">
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

        <div class="toggle-card">
          <div class="setting-label">
            <span class="setting-name">Show Model Thinking</span>
            <span class="setting-desc">{showThinking ? 'Reasoning models show a collapsible "Thinking" trace above their reply' : 'Reasoning is hidden entirely — only the in-character reply is shown'}</span>
          </div>
          <button
            class="toggle-switch"
            class:on={showThinking}
            onclick={() => {
              showThinking = !showThinking;
              success(showThinking ? 'Model thinking will be shown (collapsed by default)' : 'Model thinking hidden');
            }}
            role="switch"
            aria-checked={showThinking}
            aria-label="Toggle showing model reasoning/thinking"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        <div class="toggle-card">
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

        <div class="toggle-card">
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
      </div>
    {/if}

    {#if activeSection === 'context'}
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
    {/if}

    {#if activeSection === 'privacy'}
      <div class="panel-heading animate-fade-in-up stagger-3">
        <span class="panel-heading-title">Data & Privacy</span>
        <span class="panel-heading-desc">What Janus stores, and how to back it up or wipe it</span>
      </div>
      <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
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
          <button class="settings-btn outline" onclick={handleExport} disabled={isExporting || isImporting}>
            <Icon name="download" size={14} color="var(--fg-secondary)" />
            <span>{isExporting ? 'Exporting...' : 'Export Data'}</span>
          </button>
          <button class="settings-btn outline" onclick={handleImport} disabled={isExporting || isImporting}>
            <Icon name="upload" size={14} color="var(--fg-secondary)" />
            <span>{isImporting ? 'Importing...' : 'Import Data'}</span>
          </button>
        </div>
        {#if backupStatus}
          <span class="backup-status">{backupStatus}</span>
        {/if}

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
    {/if}

    {#if activeSection === 'image'}
      <div class="panel-heading animate-fade-in-up stagger-3b">
        <span class="panel-heading-title">Image Generation</span>
        <span class="panel-heading-desc">Reusable sampler/style bundles for scene generation — pick one per chat in the Scene panel, or mark one as the default for every conversation.</span>
      </div>

      <div class="settings-toggle-grid animate-fade-in-up stagger-3b">
        <div class="toggle-card">
          <div class="setting-label">
            <span class="setting-name">Allow Mature Content</span>
            <span class="setting-desc">{allowMatureContent ? "Won't false-positive block ordinary character descriptions that brush an overzealous NSFW classifier" : 'Strict filtering — AI Horde may block/censor borderline generations'}</span>
          </div>
          <button
            class="toggle-switch"
            class:on={allowMatureContent}
            onclick={() => {
              allowMatureContent = !allowMatureContent;
              success(allowMatureContent ? 'Mature content allowed — fewer false-positive blocks' : 'Strict content filtering enabled');
            }}
            role="switch"
            aria-checked={allowMatureContent}
            aria-label="Toggle allowing mature content in scene generation"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        <div class="toggle-card">
          <div class="setting-label">
            <span class="setting-name">Auto-Generate NPC Portraits</span>
            <span class="setting-desc">{autoGenerateNpcPortraits ? 'Auto-detected characters get a generated portrait via your configured image provider' : 'New characters show a placeholder until you generate a portrait manually'}</span>
          </div>
          <button
            class="toggle-switch"
            class:on={autoGenerateNpcPortraits}
            onclick={() => {
              autoGenerateNpcPortraits = !autoGenerateNpcPortraits;
              success(autoGenerateNpcPortraits ? 'NPC portraits will be auto-generated' : 'NPC portraits will not be auto-generated');
            }}
            role="switch"
            aria-checked={autoGenerateNpcPortraits}
            aria-label="Toggle auto-generating NPC portraits"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>

        {#if autoGenerateNpcPortraits}
          <div class="toggle-card">
            <div class="setting-label">
              <span class="setting-name">Auto-Approve NPC Portraits</span>
              <span class="setting-desc">{autoApproveNpcPortraits ? 'Generated portraits are used immediately' : 'Generated portraits wait for your approval in the Cast panel'}</span>
            </div>
            <button
              class="toggle-switch"
              class:on={autoApproveNpcPortraits}
              onclick={() => {
                autoApproveNpcPortraits = !autoApproveNpcPortraits;
                success(autoApproveNpcPortraits ? 'NPC portraits auto-approve' : 'NPC portraits require your approval');
              }}
              role="switch"
              aria-checked={autoApproveNpcPortraits}
              aria-label="Toggle auto-approving NPC portraits"
            >
              <span class="toggle-knob"></span>
            </button>
          </div>
        {/if}
      </div>

      <section class="settings-section animate-fade-in-up stagger-3b">

        {#if isLoadingPresets}
          <span class="setting-desc">Loading…</span>
        {:else if imagePresets.length > 0}
          <div class="preset-list">
            {#each imagePresets as p (p.id)}
              <div class="preset-card" class:preset-card-default={p.is_default}>
                <div class="preset-card-hdr">
                  <div class="preset-card-hdr-left">
                    <span class="preset-name">{p.name}</span>
                    {#if p.is_default}<span class="badge-default">Default</span>{/if}
                  </div>
                  <button class="icon-btn-sm" onclick={() => { p.isExpanded = !p.isExpanded; imagePresets = [...imagePresets]; }} aria-label="Toggle preset details">
                    <Icon name={p.isExpanded ? 'chevron-up' : 'chevron-down'} size={13} color="#5a5a7a" />
                  </button>
                </div>

                {#if p.isExpanded}
                  <div class="preset-card-body">
                    <div class="preset-field-row">
                      <div class="preset-field">
                        <span class="preset-flabel">Model (optional)</span>
                        <select class="preset-finput mono" value={p.model ?? ''}
                          onchange={(e) => { const v = e.currentTarget.value; p.model = v || null; imagePresets = [...imagePresets]; savePresetField(p, 'model', v); }}>
                          <option value="">None — let style/default decide</option>
                          {#each modelOptionsFor(p.model) as opt}
                            <option value={opt.model_id}>{opt.label}</option>
                          {/each}
                        </select>
                      </div>
                      <div class="preset-field">
                        <span class="preset-flabel">Sampler</span>
                        <select class="preset-finput mono" value={p.sampler_name}
                          onchange={(e) => { const v = e.currentTarget.value; p.sampler_name = v; imagePresets = [...imagePresets]; savePresetField(p, 'samplerName', v); }}>
                          {#each HORDE_SAMPLERS as s}<option value={s}>{s}</option>{/each}
                        </select>
                      </div>
                    </div>
                    <div class="preset-field-row">
                      <div class="preset-field">
                        <span class="preset-flabel">CFG Scale</span>
                        <input class="preset-finput mono" type="number" step="0.5" min="1" max="30" value={p.cfg_scale}
                          onblur={(e) => { const v = parseFloat(e.currentTarget.value); p.cfg_scale = v; imagePresets = [...imagePresets]; savePresetField(p, 'cfgScale', v); }} />
                      </div>
                      <div class="preset-field">
                        <span class="preset-flabel">Steps</span>
                        <input class="preset-finput mono" type="number" step="1" min="1" max="150" value={p.steps}
                          onblur={(e) => { const v = parseInt(e.currentTarget.value, 10); p.steps = v; imagePresets = [...imagePresets]; savePresetField(p, 'steps', v); }} />
                      </div>
                      <div class="preset-field preset-field-checkbox">
                        <span class="preset-flabel">Karras</span>
                        <label class="checkbox-wrap-preset">
                          <input type="checkbox" checked={p.karras}
                            onchange={(e) => { const v = e.currentTarget.checked; p.karras = v; imagePresets = [...imagePresets]; savePresetField(p, 'karras', v); }} />
                          <span>Smoother noise schedule</span>
                        </label>
                      </div>
                    </div>
                    <div class="preset-field">
                      <span class="preset-flabel">Style (optional — overrides sampler/model/resolution above)</span>
                      <input class="preset-finput mono" value={p.style ?? ''} placeholder="e.g. raw-png, pixel-art"
                        onblur={(e) => { const v = e.currentTarget.value; p.style = v || null; imagePresets = [...imagePresets]; savePresetField(p, 'style', v); }} />
                      <a href="https://artbot.site/" target="_blank" class="hint-link-sm">Browse styles →</a>
                    </div>
                    <div class="preset-field">
                      <span class="preset-flabel">Negative Prompt (optional, ignored if a style is set)</span>
                      <input class="preset-finput mono" value={p.negative_prompt ?? ''} placeholder="Leave blank for the built-in default"
                        onblur={(e) => { const v = e.currentTarget.value; p.negative_prompt = v || null; imagePresets = [...imagePresets]; savePresetField(p, 'negativePrompt', v); }} />
                    </div>
                    <div class="preset-field-row">
                      <div class="preset-field">
                        <span class="preset-flabel">Face Fix</span>
                        <select class="preset-finput mono" value={faceFixerOf(p.post_processing)}
                          onchange={(e) => {
                            const v = e.currentTarget.value;
                            p.post_processing = composePostProcessing(v, upscalerOf(p.post_processing));
                            imagePresets = [...imagePresets];
                            savePresetField(p, 'postProcessing', p.post_processing);
                          }}>
                          {#each FACE_FIXERS as f}<option value={f.value}>{f.label}</option>{/each}
                        </select>
                      </div>
                      <div class="preset-field">
                        <span class="preset-flabel">Upscaler</span>
                        <select class="preset-finput mono" value={upscalerOf(p.post_processing)}
                          onchange={(e) => {
                            const v = e.currentTarget.value;
                            p.post_processing = composePostProcessing(faceFixerOf(p.post_processing), v);
                            imagePresets = [...imagePresets];
                            savePresetField(p, 'postProcessing', p.post_processing);
                          }}>
                          {#each UPSCALERS as u}<option value={u.value}>{u.label}</option>{/each}
                        </select>
                      </div>
                      <div class="preset-field">
                        <span class="preset-flabel">CLIP Skip (blank = model default)</span>
                        <input class="preset-finput mono" type="number" step="1" min="1" max="12" placeholder="1"
                          value={p.clip_skip ?? ''}
                          onblur={(e) => {
                            const raw = e.currentTarget.value.trim();
                            const v = raw === '' ? 0 : parseInt(raw, 10);
                            p.clip_skip = v || null;
                            imagePresets = [...imagePresets];
                            savePresetField(p, 'clipSkip', v);
                          }} />
                      </div>
                    </div>
                    <div class="preset-field preset-field-checkbox">
                      <label class="checkbox-wrap-preset">
                        <input type="checkbox" checked={p.hires_fix}
                          onchange={(e) => { const v = e.currentTarget.checked; p.hires_fix = v; imagePresets = [...imagePresets]; savePresetField(p, 'hiresFix', v); }} />
                        <span>Hi-Res Fix — re-processes at higher resolution (best detail/anatomy fix, ~2x generation time &amp; kudos cost)</span>
                      </label>
                      {#if p.hires_fix}
                        <input class="preset-finput mono" type="number" step="0.05" min="0.01" max="1"
                          value={p.hires_fix_denoising_strength ?? 0.65}
                          onblur={(e) => { const v = parseFloat(e.currentTarget.value); p.hires_fix_denoising_strength = v; imagePresets = [...imagePresets]; savePresetField(p, 'hiresFixDenoisingStrength', v); }} />
                      {/if}
                    </div>
                  </div>
                {/if}

                <div class="preset-card-actions">
                  {#if !p.is_default}
                    <button class="settings-btn outline sm" onclick={() => setDefaultPreset(p)}>Set Default</button>
                  {/if}
                  <button class="settings-btn danger sm" onclick={() => deleteImagePresetRow(p)}>Delete</button>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        {#if showAddPresetForm}
          <div class="preset-add-form">
            <div class="preset-field-row">
              <div class="preset-field">
                <span class="preset-flabel">Name</span>
                <input class="preset-finput" bind:value={newPresetName} placeholder="e.g. Fantasy Painting" />
              </div>
              <div class="preset-field">
                <span class="preset-flabel">Model (optional)</span>
                <select class="preset-finput mono" bind:value={newPresetModel}>
                  <option value="">None — let style/default decide</option>
                  {#each modelOptionsFor(null) as opt}
                    <option value={opt.model_id}>{opt.label}</option>
                  {/each}
                </select>
                {#if enabledImageModels.length === 0}
                  <a href="/models" class="hint-link-sm">Enable image models →</a>
                {/if}
              </div>
            </div>
            <div class="preset-field-row">
              <div class="preset-field">
                <span class="preset-flabel">Sampler</span>
                <select class="preset-finput mono" bind:value={newPresetSampler}>
                  {#each HORDE_SAMPLERS as s}<option value={s}>{s}</option>{/each}
                </select>
              </div>
              <div class="preset-field">
                <span class="preset-flabel">CFG Scale</span>
                <input class="preset-finput mono" type="number" step="0.5" min="1" max="30" bind:value={newPresetCfgScale} />
              </div>
              <div class="preset-field">
                <span class="preset-flabel">Steps</span>
                <input class="preset-finput mono" type="number" step="1" min="1" max="150" bind:value={newPresetSteps} />
              </div>
              <div class="preset-field preset-field-checkbox">
                <span class="preset-flabel">Karras</span>
                <label class="checkbox-wrap-preset">
                  <input type="checkbox" bind:checked={newPresetKarras} />
                  <span>Smoother noise schedule</span>
                </label>
              </div>
            </div>
            <div class="preset-field">
              <span class="preset-flabel">Style (optional)</span>
              <input class="preset-finput mono" bind:value={newPresetStyle} placeholder="e.g. raw-png, pixel-art" />
            </div>
            <div class="preset-field">
              <span class="preset-flabel">Negative Prompt (optional)</span>
              <input class="preset-finput mono" bind:value={newPresetNegativePrompt} placeholder="Leave blank for the built-in default" />
            </div>
            <div class="preset-field-row">
              <div class="preset-field">
                <span class="preset-flabel">Face Fix</span>
                <select class="preset-finput mono" bind:value={newPresetFaceFixer}>
                  {#each FACE_FIXERS as f}<option value={f.value}>{f.label}</option>{/each}
                </select>
              </div>
              <div class="preset-field">
                <span class="preset-flabel">Upscaler</span>
                <select class="preset-finput mono" bind:value={newPresetUpscaler}>
                  {#each UPSCALERS as u}<option value={u.value}>{u.label}</option>{/each}
                </select>
              </div>
              <div class="preset-field">
                <span class="preset-flabel">CLIP Skip (blank = model default)</span>
                <input class="preset-finput mono" type="number" step="1" min="1" max="12" placeholder="1"
                  value={newPresetClipSkip ?? ''}
                  oninput={(e) => { const raw = e.currentTarget.value.trim(); newPresetClipSkip = raw === '' ? null : parseInt(raw, 10); }} />
              </div>
            </div>
            <div class="preset-field preset-field-checkbox">
              <label class="checkbox-wrap-preset">
                <input type="checkbox" bind:checked={newPresetHiresFix} />
                <span>Hi-Res Fix — re-processes at higher resolution (best detail/anatomy fix, ~2x generation time &amp; kudos cost)</span>
              </label>
              {#if newPresetHiresFix}
                <input class="preset-finput mono" type="number" step="0.05" min="0.01" max="1" bind:value={newPresetHiresFixDenoising} />
              {/if}
            </div>
            <div class="button-row">
              <button class="settings-btn outline" onclick={() => showAddPresetForm = false}>Cancel</button>
              <button class="settings-btn primary" onclick={addImagePreset} disabled={isSavingPreset || !newPresetName.trim()}>
                {isSavingPreset ? 'Adding…' : 'Add Preset'}
              </button>
            </div>
          </div>
        {:else}
          <button class="settings-btn outline" onclick={() => showAddPresetForm = true}>
            <Icon name="plus" size={14} color="var(--fg-secondary)" />
            <span>Add Preset</span>
          </button>
        {/if}
      </section>
    {/if}

    {#if activeSection === 'prompts'}
      <div class="panel-heading animate-fade-in-up stagger-4">
        <span class="panel-heading-title">Prompts</span>
        <span class="panel-heading-desc">The system-level instructions injected into every generation</span>
      </div>
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

      <!-- Character Profile Refresh -->
      <section class="settings-section animate-fade-in-up stagger-4b">
        <div class="section-header">
          <div class="section-header-left">
            <Icon name="refresh-cw" size={16} color="var(--accent-primary)" />
            <span class="section-title">Character Profile Refresh</span>
          </div>
          <button class="reset-btn" onclick={resetSystemPrompt}>Reset</button>
        </div>

        <span class="phi-description">Used by "Refresh from Story" — updates an auto-detected character's description, personality, and scenario to match how they've actually appeared, instead of leaving them stuck on the placeholder written when they were first spotted.</span>

        <textarea
          class="system-prompt-input"
          bind:value={profileRefreshPrompt}
          rows="6"
          aria-label="Character profile refresh prompt"
        ></textarea>

        <span class="prompt-hint">Must keep asking for JSON with description/personality/scenario only — editing that part away will break refreshes</span>
      </section>
    {/if}

    {#if activeSection === 'logging'}
      <div class="panel-heading animate-fade-in-up stagger-4">
        <span class="panel-heading-title">Logging</span>
        <span class="panel-heading-desc">Backend and frontend activity, for diagnosing issues without guessing</span>
      </div>

      <section class="settings-section animate-fade-in-up stagger-4">
        <div class="section-header">
          <div class="section-header-left">
            <Icon name="terminal" size={16} color="var(--accent-primary)" />
            <span class="section-title">Application Logs</span>
          </div>
          <button class="reset-btn" onclick={loadBackendLogs} disabled={isLoadingBackendLogs}>
            {isLoadingBackendLogs ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>

        <div class="log-toolbar">
          <div class="log-subtabs">
            <button class="log-subtab" class:active={logSubTab === 'backend'} onclick={() => selectLogSubTab('backend')}>
              Backend
            </button>
            <button class="log-subtab" class:active={logSubTab === 'frontend'} onclick={() => selectLogSubTab('frontend')}>
              Frontend <span class="log-subtab-count">{$frontendLogs.length}</span>
            </button>
          </div>
          <input class="log-search" type="text" placeholder="Search logs…" bind:value={logSearch} />
        </div>

        {#if logSubTab === 'backend'}
          {#if backendLogPath}
            <span class="prompt-hint log-path" title={backendLogPath}>{backendLogPath}</span>
          {/if}
          <div class="log-viewer" bind:this={logViewerEl} onscroll={handleLogViewerScroll}>
            {#if isLoadingMoreBackendLogs}
              <div class="log-loading-more">Loading older lines…</div>
            {/if}
            {#if filteredBackendLines.length === 0}
              <div class="log-empty">{backendLogLines.length ? 'No lines match your search.' : (isLoadingBackendLogs ? 'Loading…' : 'No backend logs yet.')}</div>
            {:else}
              {#each filteredBackendLines as line, i (i)}
                <div class="log-line {backendLogLineClass(line)}">{line}</div>
              {/each}
            {/if}
          </div>
        {:else}
          <div class="log-viewer" bind:this={logViewerEl}>
            {#if filteredFrontendEntries.length === 0}
              <div class="log-empty">{$frontendLogs.length ? 'No lines match your search.' : 'No frontend activity captured yet.'}</div>
            {:else}
              {#each filteredFrontendEntries as entry (entry.timestamp + entry.message)}
                <div class="log-line log-line-{entry.level}">
                  [{new Date(entry.timestamp).toLocaleTimeString()}] {entry.level.toUpperCase()} {entry.message}
                </div>
              {/each}
            {/if}
          </div>
          <button class="reset-btn" onclick={clearFrontendLogs}>Clear frontend logs</button>
        {/if}

        <div class="log-actions">
          <button class="settings-btn primary" onclick={handleExportLogs} disabled={isExportingLogs}>
            <Icon name="download" size={14} color="#fff" />
            <span>{isExportingLogs ? 'Exporting…' : 'Export Logs'}</span>
          </button>
        </div>
      </section>
    {/if}
    </div>
    {/key}
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
    display: flex; align-items: flex-end; justify-content: space-between; gap: 16px;
    padding: 28px 36px 20px; flex-shrink: 0; position: relative;
  }
  .settings-header::after {
    content: ''; position: absolute; bottom: 0; left: 36px; right: 36px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .settings-header-left { display: flex; flex-direction: column; gap: 4px; }
  .settings-title {
    font-size: 30px; font-weight: 600; letter-spacing: -0.6px;
  }
  .settings-subtitle { font-size: var(--text-lg); color: #5a5a7a; letter-spacing: 0.3px; }

  .settings-header-about {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding-bottom: 4px;
  }
  .settings-header-about .about-name { font-size: var(--text-sm); font-weight: 700; color: #8b8ba7; }
  .settings-header-about .about-dot { color: #3a3a52; }
  .settings-header-about .about-desc { font-size: 11px; color: #4a4a6a; font-family: var(--font-mono); letter-spacing: 0.3px; }

  /* ── Section nav: floating carousel, not a second sidebar ── */
  .settings-carousel {
    display: flex; align-items: center; justify-content: center; gap: 8px; flex-wrap: wrap;
    padding: 18px 36px; flex-shrink: 0;
    border-bottom: 1px solid rgba(255,255,255,0.05);
  }
  .carousel-chip {
    display: flex; align-items: center; gap: 7px;
    padding: 9px 16px; border-radius: 999px;
    background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.09);
    color: #a8a3c0; font-size: 12.5px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 220ms cubic-bezier(0.16,1,0.3,1);
  }
  .carousel-chip:hover { background: rgba(255,255,255,0.08); border-color: rgba(255,255,255,0.16); color: #e8e5f5; }
  .carousel-chip.active {
    transform: scale(1.06); color: #0a0812; font-weight: 700;
    background: var(--chip-accent);
    border-color: var(--chip-accent);
    box-shadow: 0 8px 26px -8px var(--chip-accent);
  }

  .settings-body { display: flex; flex: 1; overflow: hidden; min-height: 0; }

  .settings-panel {
    position: relative;
    flex: 1; overflow-y: auto; min-width: 0;
    padding: 32px 36px 48px; display: flex; flex-direction: column; gap: 22px;
  }
  .settings-panel::-webkit-scrollbar { width: 4px; }
  .settings-panel::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  /* Ambient light wash behind the panel content, tinted to the active
     section's accent — the "light through a prism" effect from the
     approved concept, applied for real instead of a flat backdrop. */
  .panel-glow {
    position: absolute; top: -60px; left: 50%; transform: translateX(-50%);
    width: 520px; height: 360px; border-radius: 50%;
    background: radial-gradient(circle, var(--accent), transparent 70%);
    filter: blur(120px); opacity: 0.16; pointer-events: none; z-index: 0;
    transition: background 400ms ease;
  }
  .settings-panel > :not(.panel-glow) { position: relative; z-index: 1; }

  /* ── Panel heading (replaces the old per-card section-header as the page-level title) ── */
  .panel-heading { display: flex; flex-direction: column; gap: 4px; }
  .panel-heading-title { font-size: 20px; font-weight: 800; color: #e8e0ff; letter-spacing: -0.2px; }
  .panel-heading-desc { font-size: var(--text-sm); color: #6b6b8a; max-width: 640px; line-height: 1.5; }

  /* Sections whose content is a short action/toggle list read better at a
     comfortable measure than stretched edge-to-edge on a wide window. */
  .settings-section-bounded { max-width: 640px; }

  /* ── Appearance: side-by-side setting cards instead of one narrow list ── */
  .settings-card-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px; max-width: 900px;
  }
  .settings-card {
    display: flex; flex-direction: column; gap: 10px;
    padding: 22px; border-radius: 16px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  .settings-card:hover { border-color: color-mix(in srgb, var(--accent) 45%, transparent); box-shadow: 0 4px 24px -6px var(--accent); }
  .settings-card-icon {
    width: 36px; height: 36px; border-radius: 10px; display: flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .settings-card-name { font-size: var(--text-md); font-weight: 700; color: #e8e0ff; }
  .settings-card-desc { font-size: var(--text-sm); color: #5a5a7a; margin-top: -6px; }
  .theme-toggle-lg { margin-top: 4px; }
  .setting-dropdown-lg { width: 100%; justify-content: space-between; margin-top: 4px; }

  /* ── Chat Behavior: toggle rows as a responsive card grid ── */
  .settings-toggle-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 14px;
  }
  .toggle-card {
    display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 18px 20px; border-radius: 14px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  .toggle-card:hover { border-color: color-mix(in srgb, var(--accent) 45%, transparent); box-shadow: 0 4px 24px -6px var(--accent); }

  /* ── Section Card ── */
  .settings-section {
    padding: 20px; border-radius: 16px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    display: flex; flex-direction: column; gap: 16px;
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  .settings-section:hover {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
    box-shadow: 0 4px 24px -6px var(--accent);
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
  /* Floating pill thumb inset within a padded pill track (macOS/iOS segmented-
     control pattern) — both fully rounded, so there's no radius mismatch
     between the track and the active segment like a flush 10px-radius track
     with a square-cornered active button produced. */
  .theme-toggle {
    display: flex; gap: 2px; border-radius: 999px; padding: 3px;
    border: 1px solid rgba(139,92,246,0.1);
    background: rgba(9,9,26,0.6);
  }
  .theme-btn {
    flex: 1; padding: 6px 14px; background: transparent; border: none; border-radius: 999px;
    color: #5a5a7a; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer; text-align: center;
    transition: all 200ms ease;
  }
  .theme-btn:hover { color: #8b8ba7; }
  .theme-btn.active {
    background: var(--accent);
    color: #0a0812;
    box-shadow: 0 2px 12px -2px var(--accent);
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
  .dropdown-item.active { color: var(--accent); font-weight: 700; }

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
    background: var(--accent);
    justify-content: flex-end;
    box-shadow: 0 0 12px -2px var(--accent);
  }
  .toggle-knob {
    width: 18px; height: 18px; border-radius: 50%; background: #fff;
    transition: transform 250ms cubic-bezier(0.34,1.56,0.64,1);
    box-shadow: 0 1px 4px rgba(0,0,0,0.3);
  }

  /* ── Buttons ── */
  .button-row { display: flex; gap: 10px; }
  .backup-status {
    display: block; margin-top: 8px;
    font-size: var(--text-xs); color: var(--fg-muted);
    font-family: var(--font-mono);
  }
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
  .settings-btn.primary {
    background: var(--accent); color: #0a0812;
    box-shadow: 0 2px 12px -2px var(--accent); flex: 1;
  }
  .settings-btn.primary:hover { transform: translateY(-1px); box-shadow: 0 4px 20px -4px var(--accent); }
  .settings-btn.primary:disabled { opacity: 0.5; pointer-events: none; }
  .settings-btn.sm { padding: 6px 12px; font-size: var(--text-xs); flex: none; }

  /* ── Image Generation Presets ── */
  .preset-list { display: flex; flex-direction: column; gap: 10px; margin: 4px 0; }
  .preset-card {
    border-radius: 12px; padding: 12px 14px;
    background: rgba(12,12,26,0.5); border: 1px solid rgba(139,92,246,0.07);
    display: flex; flex-direction: column; gap: 10px;
  }
  .preset-card-default { border-color: rgba(139,92,246,0.2); }
  .preset-card-hdr { display: flex; align-items: center; justify-content: space-between; }
  .preset-card-hdr-left { display: flex; align-items: center; gap: 8px; }
  .preset-name { font-size: 13px; font-weight: 600; color: #d0d0e8; }
  .badge-default {
    padding: 2px 8px; border-radius: 99px; font-size: 10px; font-weight: 700;
    background: rgba(16,185,129,0.12); color: #10B981;
  }
  .icon-btn-sm {
    width: 24px; height: 24px; border-radius: 7px; border: 1px solid rgba(139,92,246,0.08);
    background: transparent; cursor: pointer; display: flex; align-items: center; justify-content: center;
  }
  .icon-btn-sm:hover { background: rgba(139,92,246,0.08); }
  .preset-card-body { display: flex; flex-direction: column; gap: 10px; padding-top: 2px; border-top: 1px solid rgba(139,92,246,0.06); }
  .preset-field-row { display: flex; gap: 10px; flex-wrap: wrap; }
  .preset-field { display: flex; flex-direction: column; gap: 5px; flex: 1; min-width: 120px; }
  .preset-field-checkbox { justify-content: flex-end; }
  .preset-flabel { font-size: 10px; font-weight: 700; letter-spacing: 0.5px; text-transform: uppercase; color: #4a4a6a; font-family: var(--font-mono); }
  .preset-finput {
    height: 32px; padding: 0 10px; border-radius: 8px;
    background: rgba(10,10,22,0.7); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 12px; font-family: var(--font-body); outline: none;
  }
  .preset-finput:focus { border-color: rgba(139,92,246,0.35); }
  .preset-finput.mono { font-family: var(--font-mono); }
  .checkbox-wrap-preset {
    display: flex; align-items: center; gap: 6px; height: 32px;
    font-size: 11px; color: #6b6b8a; cursor: pointer;
  }
  .checkbox-wrap-preset input { accent-color: #8B5CF6; width: 14px; height: 14px; cursor: pointer; }
  .hint-link-sm { font-size: 11px; color: #a78bfa; font-weight: 600; text-decoration: none; }
  .preset-card-actions { display: flex; gap: 6px; justify-content: flex-end; }
  .preset-add-form {
    display: flex; flex-direction: column; gap: 10px; margin-top: 10px;
    padding: 12px 14px; border-radius: 12px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.1);
  }

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
    color: var(--accent); font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); transition: opacity 150ms;
  }
  .reset-btn:hover { opacity: 0.7; }

  /* ── Logging ── */
  .log-toolbar {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    margin-bottom: 10px; flex-wrap: wrap;
  }
  .log-subtabs {
    display: flex; gap: 4px; padding: 3px; border-radius: 10px;
    background: rgba(0,0,0,0.2); border: 1px solid rgba(139,92,246,0.08);
  }
  .log-subtab {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 7px; border: none; background: none;
    color: #8b8ba7; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer; transition: all 150ms;
  }
  .log-subtab:hover { color: #c8c8e0; }
  .log-subtab.active { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); }
  .log-subtab-count {
    font-size: 10px; font-family: var(--font-mono); color: inherit; opacity: 0.7;
  }
  .log-search {
    flex: 1; min-width: 160px; max-width: 280px;
    padding: 7px 12px; border-radius: 8px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.1);
    color: #c8c8e0; font-size: var(--text-sm); font-family: var(--font-body);
    outline: none; transition: border-color 150ms;
  }
  .log-search:focus { border-color: rgba(139,92,246,0.3); }
  .log-path {
    display: block; margin-bottom: 8px; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; opacity: 0.7;
  }
  .log-viewer {
    max-height: 420px; overflow-y: auto; padding: 10px 12px; border-radius: 10px;
    background: rgba(7,7,18,0.7); border: 1px solid rgba(139,92,246,0.08);
    font-family: var(--font-mono); font-size: 11px; line-height: 1.6;
  }
  .log-loading-more {
    text-align: center; padding: 6px 0 10px; color: #6a6a86; font-size: 10.5px;
    font-family: var(--font-mono); letter-spacing: 0.02em;
  }
  .log-line {
    white-space: pre-wrap; word-break: break-word; color: #7d7d99;
    padding: 1px 0;
  }
  .log-line-error { color: #fb7185; }
  .log-line-warn { color: #fbbf24; }
  .log-line-debug { color: #5a5a7a; }
  .log-empty {
    padding: 20px 0; text-align: center; color: #4a4a6a;
    font-family: var(--font-body); font-size: var(--text-sm);
  }
  .log-actions { display: flex; justify-content: flex-end; margin-top: 12px; }

  /* ── Nav footer links (About, moved into the sidebar) ── */
  .about-link-btn {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: all 150ms;
  }
  .about-link-btn:hover { background: rgba(139,92,246,0.06); }

  /* ── Responsive ── */
  @media (max-width: 768px) {
    .settings-header { flex-direction: column; align-items: flex-start; }
    .settings-header-about { padding-bottom: 0; }
    .settings-carousel { padding: 10px 16px; overflow-x: auto; flex-wrap: nowrap; justify-content: flex-start; }
    .carousel-chip span { display: none; }
    .settings-panel { padding: 20px 16px 40px; }
  }

  /* ── Staggered Entrance ── */
  .animate-fade-in-up { animation: fadeInUp 400ms ease both; }
  .stagger-1 { animation-delay: 40ms; }
  .stagger-2 { animation-delay: 100ms; }
  .stagger-2b { animation-delay: 140ms; }
  .stagger-2c { animation-delay: 160ms; }
  .stagger-3 { animation-delay: 180ms; }
  .stagger-3b { animation-delay: 210ms; }
  .stagger-4 { animation-delay: 240ms; }
  .stagger-4b { animation-delay: 280ms; }

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
