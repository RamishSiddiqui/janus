<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { Scene } from '$lib/services/ipc';
  import { sceneGenerations, getSceneGenerationState, describeProgress } from '$lib/stores/sceneGeneration';

  let { conversationId, onClose }: { conversationId: string; onClose: () => void } = $props();

  let scenes = $state<Scene[]>([]);
  let thumbUrls = $state<Record<string, string>>({});
  let isLoadingList = $state(true);
  let enlargedId = $state<string | null>(null);
  let enlarged = $derived(scenes.find(s => s.id === enlargedId) ?? null);

  /** Shape of the generation-details JSON stored on each scene — see
   *  `generate_via_ai_horde`'s metadata construction in scenes.rs. Every
   *  field is optional since older scenes were saved before newer knobs
   *  (post-processing, hires-fix, clip_skip) existed. */
  interface SceneMetadata {
    provider?: string;
    model?: string | null;
    worker_name?: string | null;
    seed?: string | number | null;
    negative_prompt?: string | null;
    style?: string | null;
    sampler_name?: string;
    cfg_scale?: number;
    steps?: number;
    karras?: boolean;
    width?: number;
    height?: number;
    clip_skip?: number;
    post_processing?: string[];
    hires_fix?: boolean;
    img2img?: boolean;
    denoising_strength?: number;
  }
  let enlargedMeta = $derived((enlarged?.metadata ?? null) as SceneMetadata | null);
  let copiedField = $state<string | null>(null);

  async function copyText(text: string, field: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedField = field;
      setTimeout(() => { if (copiedField === field) copiedField = null; }, 1500);
    } catch (err) {
      console.error('Failed to copy to clipboard:', err);
    }
  }

  // Live generation status for this conversation — shared with the Scene
  // panel, so a generation kicked off there shows up here as a placeholder
  // tile even though this modal never started it.
  let genState = $derived(getSceneGenerationState($sceneGenerations, conversationId));
  let lastHandledCompletion: number | null = $state(null);
  $effect(() => {
    if (genState.completedAt && genState.completedAt !== lastHandledCompletion) {
      lastHandledCompletion = genState.completedAt;
      load();
    }
  });

  onMount(() => {
    load();
    return () => {
      for (const url of Object.values(thumbUrls)) URL.revokeObjectURL(url);
    };
  });

  async function load() {
    isLoadingList = true;
    try {
      const ipc = await import('$lib/services/ipc');
      scenes = await ipc.listScenes(conversationId);
      const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
      const urls: Record<string, string> = {};
      for (const s of scenes) {
        try {
          urls[s.id] = await loadFileAsBlobUrl(s.file_path, 'image/png');
        } catch (err) {
          console.error(`Failed to load thumbnail for scene ${s.id}:`, err);
        }
      }
      // Revoke the previous batch — load() re-runs whenever a generation
      // completes while this modal is open, and the old map was otherwise
      // only ever cleaned up on modal close, leaking one batch per regenerate.
      for (const url of Object.values(thumbUrls)) URL.revokeObjectURL(url);
      thumbUrls = urls;
    } catch (err) {
      console.error('Failed to load scene gallery:', err);
    }
    isLoadingList = false;
  }

  async function handleDelete(s: Scene, e: MouseEvent) {
    e.stopPropagation();
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteScene(s.id);
      scenes = scenes.filter(x => x.id !== s.id);
      if (thumbUrls[s.id]) {
        URL.revokeObjectURL(thumbUrls[s.id]);
        const { [s.id]: _removed, ...rest } = thumbUrls;
        thumbUrls = rest;
      }
      if (enlargedId === s.id) enlargedId = null;
    } catch (err) {
      console.error('Failed to delete scene:', err);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (enlargedId) enlargedId = null;
      else onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="gallery-overlay"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  role="presentation"
>
  <div class="gallery-modal" role="dialog" aria-modal="true" aria-label="Scene gallery">
    <div class="gallery-hdr">
      <span class="gallery-title">Scene Gallery</span>
      <button class="gallery-close" onclick={onClose} aria-label="Close gallery">
        <Icon name="x" size={16} color="#8b8ba7" />
      </button>
    </div>

    {#if isLoadingList}
      <div class="gallery-empty"><span>Loading…</span></div>
    {:else if enlarged}
      <div class="gallery-enlarged">
        <button class="gallery-back" onclick={() => enlargedId = null}>
          <Icon name="chevron-left" size={13} color="#8b8ba7" />
          <span>Back</span>
        </button>
        <div class="gallery-enlarged-body">
          {#if thumbUrls[enlarged.id]}
            <img src={thumbUrls[enlarged.id]} alt={enlarged.caption ?? enlarged.prompt} class="gallery-enlarged-img" />
          {/if}

          <div class="gallery-details">
            <div class="gallery-details-row">
              <div class="gallery-details-row-hdr">
                <span class="gallery-details-label">Prompt</span>
                <button class="gallery-copy-btn" onclick={() => copyText(enlarged.prompt, 'prompt')} title="Copy prompt">
                  <Icon name={copiedField === 'prompt' ? 'check' : 'copy'} size={11} color={copiedField === 'prompt' ? 'var(--success)' : 'var(--fg-muted)'} />
                </button>
              </div>
              <p class="gallery-details-value gallery-details-prose">{enlarged.prompt}</p>
            </div>

            {#if enlargedMeta?.negative_prompt}
              <div class="gallery-details-row">
                <div class="gallery-details-row-hdr">
                  <span class="gallery-details-label">Negative Prompt</span>
                  <button class="gallery-copy-btn" onclick={() => copyText(enlargedMeta?.negative_prompt ?? '', 'negative')} title="Copy negative prompt">
                    <Icon name={copiedField === 'negative' ? 'check' : 'copy'} size={11} color={copiedField === 'negative' ? 'var(--success)' : 'var(--fg-muted)'} />
                  </button>
                </div>
                <p class="gallery-details-value gallery-details-prose">{enlargedMeta.negative_prompt}</p>
              </div>
            {/if}

            <div class="gallery-details-chips">
              {#if enlargedMeta?.model}
                <div class="gallery-chip"><span class="gallery-chip-k">Model</span><span class="gallery-chip-v">{enlargedMeta.model}</span></div>
              {/if}
              {#if enlargedMeta?.style}
                <div class="gallery-chip"><span class="gallery-chip-k">Style</span><span class="gallery-chip-v">{enlargedMeta.style}</span></div>
              {/if}
              {#if enlargedMeta?.sampler_name}
                <div class="gallery-chip"><span class="gallery-chip-k">Sampler</span><span class="gallery-chip-v">{enlargedMeta.sampler_name}</span></div>
              {/if}
              {#if enlargedMeta?.cfg_scale !== undefined}
                <div class="gallery-chip"><span class="gallery-chip-k">CFG Scale</span><span class="gallery-chip-v">{enlargedMeta.cfg_scale}</span></div>
              {/if}
              {#if enlargedMeta?.steps !== undefined}
                <div class="gallery-chip"><span class="gallery-chip-k">Steps</span><span class="gallery-chip-v">{enlargedMeta.steps}</span></div>
              {/if}
              {#if enlargedMeta?.karras !== undefined}
                <div class="gallery-chip"><span class="gallery-chip-k">Karras</span><span class="gallery-chip-v">{enlargedMeta.karras ? 'On' : 'Off'}</span></div>
              {/if}
              {#if enlargedMeta?.clip_skip}
                <div class="gallery-chip"><span class="gallery-chip-k">Clip Skip</span><span class="gallery-chip-v">{enlargedMeta.clip_skip}</span></div>
              {/if}
              {#if enlargedMeta?.width && enlargedMeta?.height}
                <div class="gallery-chip"><span class="gallery-chip-k">Size</span><span class="gallery-chip-v">{enlargedMeta.width}×{enlargedMeta.height}</span></div>
              {/if}
              {#if enlargedMeta?.post_processing?.length}
                <div class="gallery-chip"><span class="gallery-chip-k">Post-Processing</span><span class="gallery-chip-v">{enlargedMeta.post_processing.join(', ')}</span></div>
              {/if}
              {#if enlargedMeta?.hires_fix}
                <div class="gallery-chip"><span class="gallery-chip-k">Hi-Res Fix</span><span class="gallery-chip-v">On</span></div>
              {/if}
              {#if enlargedMeta?.img2img}
                <div class="gallery-chip"><span class="gallery-chip-k">Img2Img Strength</span><span class="gallery-chip-v">{enlargedMeta.denoising_strength}</span></div>
              {/if}
              {#if enlargedMeta?.seed}
                <div class="gallery-chip"><span class="gallery-chip-k">Seed</span><span class="gallery-chip-v">{enlargedMeta.seed}</span></div>
              {/if}
              {#if enlargedMeta?.provider}
                <div class="gallery-chip"><span class="gallery-chip-k">Provider</span><span class="gallery-chip-v">{enlargedMeta.provider}</span></div>
              {/if}
            </div>

            {#if !enlargedMeta}
              <span class="gallery-details-hint">No generation details saved for this scene.</span>
            {/if}
          </div>
        </div>
      </div>
    {:else if scenes.length === 0 && !genState.isLoading}
      <div class="gallery-empty">
        <Icon name="image" size={28} color="#4a4a6a" />
        <span>No scenes generated in this conversation yet</span>
      </div>
    {:else}
      <div class="gallery-grid">
        {#if genState.isLoading}
          <div class="gallery-thumb gallery-thumb-generating">
            <div class="gallery-gen-spinner"></div>
            <span class="gallery-gen-label">{describeProgress(genState.progress)}</span>
          </div>
        {/if}
        {#each scenes as s (s.id)}
          <div
            class="gallery-thumb"
            onclick={() => enlargedId = s.id}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (enlargedId = s.id)}
          >
            {#if thumbUrls[s.id]}
              <img src={thumbUrls[s.id]} alt={s.caption ?? s.prompt} />
            {/if}
            <button class="gallery-thumb-delete" onclick={(e) => handleDelete(s, e)} aria-label="Delete scene">
              <Icon name="trash-2" size={12} color="#F43F5E" />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .gallery-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(6,6,15,0.7); backdrop-filter: blur(4px);
    display: flex; align-items: center; justify-content: center;
    animation: overlayIn 180ms ease;
  }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }

  .gallery-modal {
    width: min(720px, 92vw); max-height: 82vh;
    display: flex; flex-direction: column; gap: 14px;
    padding: 20px 22px; border-radius: 16px;
    background: linear-gradient(175deg, #0e0e22, #0a0a18);
    border: 1px solid rgba(139,92,246,0.15);
    box-shadow: 0 20px 60px rgba(0,0,0,0.5);
    animation: modalIn 220ms cubic-bezier(0.34,1.56,0.64,1);
  }
  @keyframes modalIn {
    from { opacity: 0; transform: translateY(12px) scale(0.97); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .gallery-hdr { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
  .gallery-title {
    font-size: 16px; font-weight: 700; letter-spacing: -0.2px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent;
  }
  .gallery-close {
    width: 28px; height: 28px; border-radius: 8px; border: 1px solid rgba(139,92,246,0.1);
    background: transparent; cursor: pointer; display: flex; align-items: center; justify-content: center;
  }
  .gallery-close:hover { background: rgba(139,92,246,0.08); }

  .gallery-empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; padding: 60px 20px; color: #5a5a7a; font-size: 13px;
  }

  .gallery-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 10px; overflow-y: auto; padding-right: 2px;
  }

  .gallery-thumb {
    position: relative; aspect-ratio: 1; border-radius: 10px; overflow: hidden;
    cursor: pointer; background: rgba(20,20,40,0.5); border: 1px solid rgba(139,92,246,0.08);
    transition: border-color 150ms;
  }
  .gallery-thumb:hover { border-color: rgba(139,92,246,0.3); }
  /* contain, not cover — scenes aren't guaranteed to be square (a style
     preset or hires-fix can produce a different aspect ratio), and cover
     would silently crop part of the composition to force-fill the tile.
     Square generations (the common case) look identical either way. */
  .gallery-thumb img { width: 100%; height: 100%; object-fit: contain; display: block; }

  .gallery-thumb-delete {
    position: absolute; top: 5px; right: 5px; width: 22px; height: 22px;
    border-radius: 6px; border: none; cursor: pointer;
    background: rgba(10,10,20,0.75); display: flex; align-items: center; justify-content: center;
    opacity: 0; transition: opacity 150ms;
  }
  .gallery-thumb:hover .gallery-thumb-delete { opacity: 1; }
  .gallery-thumb-delete:hover { background: rgba(244,63,94,0.25); }

  .gallery-thumb-generating {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; padding: 10px; cursor: default;
    background: linear-gradient(135deg, #1a0a2e 0%, #2d1b69 40%, #8B5CF620 70%, #1a1a2e 100%);
    background-size: 200% 200%;
    animation: galleryGenShimmer 2.4s ease-in-out infinite;
    border-color: rgba(139,92,246,0.2);
  }
  @keyframes galleryGenShimmer {
    0%, 100% { background-position: 0% 50%; }
    50% { background-position: 100% 50%; }
  }
  .gallery-gen-spinner {
    width: 20px; height: 20px; border-radius: 50%;
    border: 2px solid rgba(255,255,255,0.15); border-top-color: #c4a1ff;
    animation: gallerySpin 800ms linear infinite;
  }
  @keyframes gallerySpin { to { transform: rotate(360deg); } }
  .gallery-gen-label {
    font-size: 10px; text-align: center; color: rgba(255,255,255,0.65);
    line-height: 1.3; padding: 0 6px;
  }

  .gallery-enlarged {
    display: flex; flex-direction: column; gap: 10px; overflow-y: auto;
  }
  .gallery-back {
    display: flex; align-items: center; gap: 4px; width: fit-content;
    padding: 5px 10px; border-radius: 8px; border: 1px solid rgba(139,92,246,0.1);
    background: transparent; color: #8b8ba7; font-size: 12px; cursor: pointer;
  }
  .gallery-back:hover { background: rgba(139,92,246,0.08); }

  .gallery-enlarged-body {
    display: flex; gap: 18px; align-items: flex-start;
    flex-wrap: wrap;
  }
  .gallery-enlarged-img {
    flex: 1 1 320px; min-width: 260px; max-width: 460px;
    width: 100%; border-radius: 12px; display: block;
  }

  .gallery-details {
    flex: 1 1 280px; min-width: 240px;
    display: flex; flex-direction: column; gap: 14px;
  }
  .gallery-details-row { display: flex; flex-direction: column; gap: 5px; }
  .gallery-details-row-hdr { display: flex; align-items: center; justify-content: space-between; }
  .gallery-details-label {
    font-size: 10.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.6px;
    color: var(--fg-muted); font-family: var(--font-mono);
  }
  .gallery-copy-btn {
    background: none; border: none; padding: 3px; border-radius: 6px; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: background 150ms ease;
  }
  .gallery-copy-btn:hover { background: rgba(139,92,246,0.08); }
  .gallery-details-prose {
    font-size: 12.5px; line-height: 1.55; color: var(--fg-secondary);
    margin: 0; max-height: 140px; overflow-y: auto;
    padding: 8px 10px; border-radius: 8px;
    background: rgba(139,92,246,0.04); border: 1px solid rgba(139,92,246,0.08);
  }

  .gallery-details-chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .gallery-chip {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 10px; border-radius: 999px;
    background: rgba(139,92,246,0.06); border: 1px solid rgba(139,92,246,0.1);
    font-size: 11px;
  }
  .gallery-chip-k {
    color: var(--fg-muted); font-family: var(--font-mono); text-transform: uppercase;
    font-size: 9.5px; letter-spacing: 0.4px; font-weight: 700;
  }
  .gallery-chip-v { color: var(--fg-secondary); font-weight: 600; }

  .gallery-details-hint { font-size: 11px; color: var(--fg-muted); font-style: italic; }
</style>
