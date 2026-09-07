<script lang="ts">
  import Icon from './Icon.svelte';
  import WaveformBars from './WaveformBars.svelte';
  import { settings } from '$lib/stores/settings';
  import { error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { playBase64Wav, primeAudioPreviewContext } from '$lib/utils/audioPreview';
  import type { VoiceInfo, TtsDownloadProgressEvent, ProviderConfig } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let ttsEnabled = $state($settings.ttsEnabled);
  let ttsDefaultVoiceId = $state($settings.ttsDefaultVoiceId);
  /** `null` = built-in Kokoro (unchanged default); a ProviderConfig id =
   *  a cloud provider (issue #78). */
  let ttsDefaultProviderId = $state<string | null>($settings.ttsDefaultProviderId);
  let ttsProviders = $state<ProviderConfig[]>([]);

  // Persist changes back to store (debounced to avoid infinite loop) —
  // same pattern as every other Settings*Section.svelte.
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { ttsEnabled, ttsDefaultVoiceId, ttsDefaultProviderId };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  async function loadTtsProviders() {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      ttsProviders = await ipc.listProviders('tts');
    } catch (err) {
      console.error('Failed to load TTS providers:', err);
    }
  }

  /** Switching provider invalidates the previously-loaded voice list (a
   *  different provider's voice ids mean nothing under the old one) and
   *  clears whichever voice was selected, rather than silently keeping an
   *  id that no longer resolves to anything real. */
  async function handleProviderChange() {
    ttsDefaultVoiceId = null;
    await loadVoices();
  }

  let modelDownloaded = $state(false);
  let checkingStatus = $state(true);
  let downloading = $state(false);
  let downloadPhase = $state<TtsDownloadProgressEvent['phase'] | null>(null);
  let downloadPercent = $state(0);

  let voices = $state<VoiceInfo[]>([]);
  let loadingVoices = $state(false);
  let previewing = $state(false);
  let previewAnalyser = $state<AnalyserNode | null>(null);

  let unlistenProgress: (() => void) | null = null;

  async function refreshStatus() {
    if (!isTauri) { checkingStatus = false; return; }
    checkingStatus = true;
    await loadTtsProviders();
    try {
      const ipc = await import('$lib/services/ipc');
      const status = await ipc.ttsModelStatus();
      modelDownloaded = status.downloaded;
      // A cloud provider's voice list doesn't need the Kokoro model
      // downloaded at all — only load eagerly here when either the model's
      // already there, or a cloud provider is already selected.
      if (modelDownloaded || ttsDefaultProviderId) await loadVoices();
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
      voices = await ipc.ttsListVoices(ttsDefaultProviderId ?? undefined);
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
    // Must run synchronously here, before the first `await` below — see
    // the doc comment on primeAudioPreviewContext for why.
    primeAudioPreviewContext();
    previewing = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const audio = await ipc.ttsTestSpeak('Hello, this is a preview of this voice.', ttsDefaultVoiceId, ttsDefaultProviderId ?? undefined);
      const playback = await playBase64Wav(audio);
      previewAnalyser = playback.analyser;
      await playback.ended;
    } catch (err) {
      toastError('Preview failed');
      console.error(err);
    }
    previewAnalyser = null;
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

{#if ttsProviders.length > 0}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-4">
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Voice provider</span>
        <span class="setting-desc">Built-in Kokoro runs fully offline; a configured provider uses your own API key instead</span>
      </div>
    </div>
    <select class="edit-input" bind:value={ttsDefaultProviderId} onchange={handleProviderChange}>
      <option value={null}>Kokoro (built-in, offline)</option>
      {#each ttsProviders as provider (provider.id)}
        <option value={provider.id}>{provider.name}</option>
      {/each}
    </select>
  </section>
{/if}

<section class="settings-section settings-section-bounded animate-fade-in-up stagger-4">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="volume-2" size={16} color="var(--accent)" />
      <span class="section-title">Voice Model</span>
    </div>
  </div>

  {#if checkingStatus}
    <span class="setting-desc">Checking model status…</span>
  {:else if ttsDefaultProviderId}
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">{ttsProviders.find(p => p.id === ttsDefaultProviderId)?.name ?? 'Cloud provider'}</span>
        <span class="setting-desc">{voices.length} voices available</span>
      </div>
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
        {#if previewing}
          <WaveformBars analyser={previewAnalyser} active={previewing} />
        {:else}
          <Icon name="volume-2" size={13} color="var(--fg-secondary)" />
        {/if}
        <span>{previewing ? 'Playing…' : 'Preview'}</span>
      </button>
    </div>
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
        {#if previewing}
          <WaveformBars analyser={previewAnalyser} active={previewing} />
        {:else}
          <Icon name="volume-2" size={13} color="var(--fg-secondary)" />
        {/if}
        <span>{previewing ? 'Playing…' : 'Preview'}</span>
      </button>
    </div>
  {/if}
</section>

<style>
  select.edit-input {
    flex: 1;
    height: 34px;
    padding: 0 32px 0 10px;
    border-radius: 10px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0;
    font-size: 12.5px;
    font-family: var(--font-body);
    appearance: none;
    -webkit-appearance: none;
    cursor: pointer;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='%238b8ba7' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 10px center;
    background-size: 14px;
  }
</style>
