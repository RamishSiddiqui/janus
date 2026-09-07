<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import WaveformBars from '$lib/components/WaveformBars.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import { handleIpcError } from '$lib/utils/error';
  import { playBase64Wav, primeAudioPreviewContext } from '$lib/utils/audioPreview';
  import type { VoiceInfo, ProviderConfig, TtsModelStatus } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // A "source" is either the built-in Kokoro engine (id: null) or a
  // configured TTS-type ProviderConfig — the same `provider_id` shape
  // `tts_list_voices`/`tts_test_speak` already take (see issue #78).
  type Source = { id: string | null; name: string; adapter: string };

  let isLoading = $state(true);
  let kokoroStatus = $state<TtsModelStatus | null>(null);
  let ttsProviders = $state<ProviderConfig[]>([]);
  let sources = $derived<Source[]>([
    { id: null, name: 'Kokoro (built-in)', adapter: 'kokoro' },
    ...ttsProviders.map((p) => ({ id: p.id, name: p.name, adapter: p.adapter })),
  ]);

  // Per-source voice lists, loaded on demand (not eagerly for every source —
  // a cloud account can have hundreds to thousands of voices, no reason to
  // fetch them all before the user picks a source).
  let expandedSource = $state<string | null>('kokoro-builtin');
  let voicesBySource = $state<Record<string, VoiceInfo[]>>({});
  let loadingSource = $state<string | null>(null);

  let previewingVoiceKey = $state<string | null>(null);
  let previewAnalyser = $state<AnalyserNode | null>(null);

  function sourceKey(id: string | null): string {
    return id ?? 'kokoro-builtin';
  }

  onMount(async () => { await loadAll(); });

  async function loadAll() {
    isLoading = true;
    if (isTauri) {
      try {
        const ipc = await import('$lib/services/ipc');
        const [status, providers] = await Promise.all([
          ipc.ttsModelStatus(),
          ipc.listProviders('tts'),
        ]);
        kokoroStatus = status;
        ttsProviders = providers;
      } catch (err) {
        handleIpcError('load voice sources', err);
      }
    }
    isLoading = false;
  }

  async function toggleSource(source: Source) {
    const key = sourceKey(source.id);
    if (expandedSource === key) {
      expandedSource = null;
      return;
    }
    expandedSource = key;
    if (voicesBySource[key]) return; // already loaded
    // Kokoro's voices require the model downloaded first — send the user to
    // Settings rather than trying (and failing) to load here; cloud
    // providers have nothing to download.
    if (source.id === null && !kokoroStatus?.downloaded) return;
    loadingSource = key;
    try {
      const ipc = await import('$lib/services/ipc');
      const voices = await ipc.ttsListVoices(source.id ?? undefined);
      voicesBySource = { ...voicesBySource, [key]: voices };
    } catch (err) {
      handleIpcError(`load ${source.name}'s voices`, err);
    }
    loadingSource = null;
  }

  async function previewVoice(source: Source, voice: VoiceInfo) {
    const key = `${sourceKey(source.id)}::${voice.id}`;
    if (previewingVoiceKey) return;
    primeAudioPreviewContext();
    previewingVoiceKey = key;
    try {
      const ipc = await import('$lib/services/ipc');
      const audio = await ipc.ttsTestSpeak(
        'Hello, this is a preview of this voice.',
        voice.id,
        source.id ?? undefined,
      );
      const playback = await playBase64Wav(audio);
      previewAnalyser = playback.analyser;
      await playback.ended;
    } catch (err) {
      handleIpcError('preview voice', err);
    }
    previewAnalyser = null;
    previewingVoiceKey = null;
  }

  let totalVoiceCount = $derived(
    Object.values(voicesBySource).reduce((sum, v) => sum + v.length, 0),
  );
</script>

<svelte:head><title>Voice Models — Janus</title></svelte:head>

<div class="page">
  <header class="hdr">
    <div class="hdr-left">
      <h1 class="hdr-title"><SplitHeading text="Voice Models" /></h1>
      <div class="hdr-stats">
        {#if isLoading}
          <span class="stat">Loading…</span>
        {:else}
          <span class="stat">{sources.length} <span class="stat-label">{sources.length === 1 ? 'source' : 'sources'}</span></span>
          {#if totalVoiceCount > 0}
            <span class="stat-sep">·</span>
            <span class="stat stat-enabled">{totalVoiceCount} <span class="stat-label">voices loaded</span></span>
          {/if}
        {/if}
      </div>
    </div>
    <button class="btn-refresh" onclick={loadAll} disabled={isLoading} aria-label="Refresh voice sources">
      <Icon name="refresh-cw" size={13} color={isLoading ? '#4a4a6a' : '#8B5CF6'} />
      Refresh
    </button>
  </header>

  <div class="body">
    {#if isLoading}
      <div class="skeleton-list">
        {#each Array(3) as _}
          <Skeleton height="72px" variant="card" />
        {/each}
      </div>
    {:else}
      <div class="source-list">
        {#each sources as source (sourceKey(source.id))}
          {@const key = sourceKey(source.id)}
          {@const isExpanded = expandedSource === key}
          {@const voices = voicesBySource[key]}
          {@const isKokoroUndownloaded = source.id === null && !kokoroStatus?.downloaded}
          <div class="source-card" class:expanded={isExpanded}>
            <button class="source-header" onclick={() => toggleSource(source)}>
              <span class="source-icon">
                <Icon name="volume-2" size={15} color={isExpanded ? '#c4a1ff' : '#6b6b8a'} />
              </span>
              <span class="source-name">{source.name}</span>
              <span class="source-adapter">{source.adapter}</span>
              {#if isKokoroUndownloaded}
                <span class="source-badge badge-warn">Not downloaded</span>
              {:else if voices}
                <span class="source-badge">{voices.length} voices</span>
              {/if}
              <span class="source-chevron" class:open={isExpanded}>
                <Icon name="chevron-down" size={14} color="#5a5a7a" />
              </span>
            </button>

            {#if isExpanded}
              <div class="source-body">
                {#if isKokoroUndownloaded}
                  <p class="empty-note">
                    The built-in voice model hasn't been downloaded yet — head to
                    <a href="/settings">Settings → Voice</a> to fetch it (~90MB, one-time).
                  </p>
                {:else if loadingSource === key}
                  <div class="voice-grid">
                    {#each Array(6) as _}
                      <Skeleton height="40px" variant="card" />
                    {/each}
                  </div>
                {:else if voices && voices.length > 0}
                  <div class="voice-grid">
                    {#each voices as voice (voice.id)}
                      {@const voiceKey = `${key}::${voice.id}`}
                      {@const isPreviewing = previewingVoiceKey === voiceKey}
                      <button
                        class="voice-chip"
                        disabled={!!previewingVoiceKey}
                        onclick={() => previewVoice(source, voice)}
                        title="Preview {voice.name}"
                      >
                        <span class="voice-chip-name">{voice.name}</span>
                        {#if isPreviewing}
                          <WaveformBars analyser={previewAnalyser} active={true} color="#c4a1ff" />
                        {:else}
                          <Icon name="volume-2" size={12} color="#5a5a7a" />
                        {/if}
                      </button>
                    {/each}
                  </div>
                {:else if voices}
                  <p class="empty-note">No voices found for this source.</p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}

        {#if ttsProviders.length === 0}
          <p class="empty-note hint">
            Add an ElevenLabs or Google Cloud TTS key under
            <a href="/providers">Providers</a> to browse cloud voices here too.
          </p>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0b0b1e 0%, #080814 40%, #06060f 100%);
  }

  .hdr {
    display: flex; align-items: flex-start; justify-content: space-between;
    padding: 24px 28px 20px; flex-shrink: 0; position: relative;
  }
  .hdr-left { display: flex; flex-direction: column; gap: 6px; }
  .hdr-title { font-size: 24px; font-weight: 600; letter-spacing: -0.5px; margin: 0; }
  .hdr-stats { display: flex; align-items: center; gap: 6px; }
  .stat { font-size: 13px; font-weight: 700; color: #c0c0d8; }
  .stat-label { font-weight: 400; color: #4a4a6a; }
  .stat-sep { color: #2a2a4a; }
  .stat-enabled { color: #10B981; }

  .btn-refresh {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.15); background: rgba(139,92,246,0.06);
    color: #8B5CF6; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms ease;
  }
  .btn-refresh:hover { background: rgba(139,92,246,0.14); border-color: rgba(139,92,246,0.3); transform: translateY(-1px); }
  .btn-refresh:disabled { opacity: 0.4; pointer-events: none; }

  .body { flex: 1; overflow-y: auto; padding: 0 28px 32px; }
  .skeleton-list, .source-list { display: flex; flex-direction: column; gap: 10px; max-width: 780px; }

  .source-card {
    border-radius: 12px; border: 1px solid rgba(139,92,246,0.1);
    background: rgba(255,255,255,0.02); overflow: hidden;
  }
  .source-card.expanded { border-color: rgba(139,92,246,0.25); background: rgba(139,92,246,0.03); }

  .source-header {
    width: 100%; display: flex; align-items: center; gap: 10px;
    padding: 14px 16px; background: none; border: none; cursor: pointer;
    text-align: left; font-family: var(--font-body);
  }
  .source-icon { display: flex; flex-shrink: 0; }
  .source-name { font-size: 13.5px; font-weight: 600; color: #e0e0f0; }
  .source-adapter {
    font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.04em; text-transform: uppercase;
    color: #5a5a7a; background: rgba(255,255,255,0.03); border-radius: 5px; padding: 2px 6px;
  }
  .source-badge {
    margin-left: auto; font-size: 11px; color: #8b8ba7;
    background: rgba(255,255,255,0.03); border-radius: 999px; padding: 3px 10px;
  }
  .source-badge.badge-warn { color: #f0a15c; background: rgba(240,161,92,0.08); }
  .source-chevron { display: flex; transition: transform 150ms ease; flex-shrink: 0; }
  .source-chevron.open { transform: rotate(180deg); }

  .source-body { padding: 0 16px 16px; }
  .empty-note { font-size: 12.5px; color: #6b6b8a; margin: 4px 0; }
  .empty-note a { color: #a78bfa; }
  .empty-note.hint { max-width: 780px; }

  .voice-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px;
  }
  .voice-chip {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 9px 12px; border-radius: 9px;
    border: 1px solid rgba(255,255,255,0.05); background: rgba(255,255,255,0.02);
    color: #c0c0d8; font-size: 12px; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms ease; text-align: left;
  }
  .voice-chip:hover:not(:disabled) { border-color: rgba(139,92,246,0.25); background: rgba(139,92,246,0.06); }
  .voice-chip:disabled { opacity: 0.5; cursor: default; }
  .voice-chip-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-transform: capitalize; }
</style>
