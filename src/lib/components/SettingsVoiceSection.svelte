<script lang="ts">
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { playBase64Wav } from '$lib/utils/audioPreview';
  import type { VoiceInfo, TtsDownloadProgressEvent } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let ttsEnabled = $state($settings.ttsEnabled);
  let ttsDefaultVoiceId = $state($settings.ttsDefaultVoiceId);

  // Persist changes back to store (debounced to avoid infinite loop) —
  // same pattern as every other Settings*Section.svelte.
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { ttsEnabled, ttsDefaultVoiceId };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  let modelDownloaded = $state(false);
  let checkingStatus = $state(true);
  let downloading = $state(false);
  let downloadPhase = $state<TtsDownloadProgressEvent['phase'] | null>(null);
  let downloadPercent = $state(0);

  let voices = $state<VoiceInfo[]>([]);
  let loadingVoices = $state(false);
  let previewing = $state(false);

  let unlistenProgress: (() => void) | null = null;

  async function refreshStatus() {
    if (!isTauri) { checkingStatus = false; return; }
    checkingStatus = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const status = await ipc.ttsModelStatus();
      modelDownloaded = status.downloaded;
      if (modelDownloaded) await loadVoices();
    } catch (err) {
      console.error('Failed to check TTS model status:', err);
    }
    checkingStatus = false;
  }

  async function loadVoices() {
    if (!isTauri || loadingVoices) return;
    loadingVoices = true;
    try {
      const ipc = await import('$lib/services/ipc');
      voices = await ipc.ttsListVoices();
    } catch (err) {
      toastError('Failed to load voice list');
      console.error(err);
    }
    loadingVoices = false;
  }

  async function handleDownload() {
    if (!isTauri || downloading) return;
    downloading = true;
    downloadPhase = 'model';
    downloadPercent = 0;
    try {
      const ipc = await import('$lib/services/ipc');
      unlistenProgress = await ipc.onTtsDownloadProgress((event) => {
        downloadPhase = event.phase;
        downloadPercent = event.percent;
      });
      await ipc.ttsDownloadModel();
      modelDownloaded = true;
      await loadVoices();
    } catch (err) {
      toastError('Failed to download voice model');
      console.error(err);
    } finally {
      unlistenProgress?.();
      unlistenProgress = null;
      downloading = false;
      downloadPhase = null;
    }
  }

  async function handlePreview() {
    if (!isTauri || !ttsDefaultVoiceId || previewing) return;
    previewing = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const audio = await ipc.ttsTestSpeak('Hello, this is a preview of this voice.', ttsDefaultVoiceId);
      await playBase64Wav(audio);
    } catch (err) {
      toastError('Preview failed');
      console.error(err);
    }
    previewing = false;
  }

  const phaseLabel: Record<string, string> = { model: 'model', voices: 'voice pack', runtime: 'runtime' };

  refreshStatus();
</script>

<div class="panel-heading animate-fade-in-up stagger-3">
  <span class="panel-heading-title">Voice</span>
  <span class="panel-heading-desc">Local, offline text-to-speech — assistant responses are spoken aloud as they stream</span>
</div>

<section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Enable Voice Output</span>
      <span class="setting-desc">Off by default — responses only speak once you turn this on</span>
    </div>
    <button
      class="toggle-switch"
      class:on={ttsEnabled}
      onclick={() => ttsEnabled = !ttsEnabled}
      role="switch"
      aria-checked={ttsEnabled}
      aria-label="Toggle voice output"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>
</section>

<section class="settings-section settings-section-bounded animate-fade-in-up stagger-4">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="volume-2" size={16} color="var(--accent)" />
      <span class="section-title">Voice Model</span>
    </div>
  </div>

  {#if checkingStatus}
    <span class="setting-desc">Checking model status…</span>
  {:else if !modelDownloaded}
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Kokoro voice model</span>
        <span class="setting-desc">Not downloaded — ~90MB, runs fully offline once fetched</span>
      </div>
      <button class="settings-btn primary sm" onclick={handleDownload} disabled={downloading}>
        <Icon name="download" size={13} color="#0a0812" />
        <span>{downloading ? 'Downloading…' : 'Download'}</span>
      </button>
    </div>
    {#if downloading}
      <span class="backup-status">
        Downloading {phaseLabel[downloadPhase ?? 'model']}… {downloadPercent}%
      </span>
    {/if}
  {:else}
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Kokoro voice model</span>
        <span class="setting-desc">Downloaded — {voices.length} voices available</span>
      </div>
      <Icon name="check" size={16} color="var(--accent)" />
    </div>

    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Default voice</span>
        <span class="setting-desc">Used for any character without their own voice assigned</span>
      </div>
    </div>
    <div class="button-row">
      <select class="edit-input" bind:value={ttsDefaultVoiceId} disabled={loadingVoices}>
        <option value={null}>None</option>
        {#each voices as voice (voice.id)}
          <option value={voice.id}>{voice.name}</option>
        {/each}
      </select>
      <button class="settings-btn outline sm" onclick={handlePreview} disabled={!ttsDefaultVoiceId || previewing}>
        <Icon name="volume-2" size={13} color="var(--fg-secondary)" />
        <span>{previewing ? 'Playing…' : 'Preview'}</span>
      </button>
    </div>
  {/if}
</section>

<style>
  select.edit-input {
    flex: 1;
    height: 34px;
    padding: 0 10px;
    border-radius: 10px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0;
    font-size: 12.5px;
    font-family: var(--font-body);
  }
</style>
