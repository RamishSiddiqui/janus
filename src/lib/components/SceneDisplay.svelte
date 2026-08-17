<script lang="ts">
  import { browser } from '$app/environment';
  import { onMount, onDestroy } from 'svelte';
  import Icon from './Icon.svelte';
  import { activeConversationId } from '$lib/stores/chat';
  import { settings } from '$lib/stores/settings';
  import { get } from 'svelte/store';
  import type { SceneState } from '$lib/services/ipc';
  import {
    sceneGenerations, getSceneGenerationState, runSceneGeneration, runVideoSceneGeneration, describeProgress,
  } from '$lib/stores/sceneGeneration';
  import { humanizeProviderError } from '$lib/utils/providerError';

  let {
    characterId = null,
    characterName = '',
    characterDescription = '',
    avatarPath = null,
    additionalCharacters = [],
  }: {
    characterId?: string | null;
    characterName?: string;
    characterDescription?: string;
    /** Raw relative avatar path (not a blob: URL) — used as the img2img
     *  reference when that toggle is on. */
    avatarPath?: string | null;
    additionalCharacters?: { id: string; name: string; description: string }[];
  } = $props();

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let activeTab: 'image' | 'video' = $state('image');
  let sceneCaption = $state('No scene generated yet');
  let sceneImageUrl: string | null = $state(null);
  let currentSceneId: string | null = $state(null);
  let promptText = $state('');
  let isEditingPrompt = $state(false);

  // Image generation presets — the list is shared across all chats, but the
  // *selection* (null = "use whatever the default preset is") is per-conversation.
  let presets = $state<{ id: string; name: string; model: string | null }[]>([]);
  let selectedPresetId = $state<string | null>(null);

  // Models the user has enabled for image generation (Models page) — the
  // pool the "Model" override dropdown picks from.
  let enabledImageModels = $state<{ model_id: string; img2img_supported: boolean | null }[]>([]);
  // Per-generation model override: auto-filled from whichever preset is
  // selected (its own `model`, if set), but freely editable just before
  // generating — not persisted, so it re-derives from the preset every time
  // the conversation/preset changes rather than sticking around stale.
  let modelOverride = $state<string | null>(null);

  // Whether the currently-resolved model is known to support img2img — used
  // to gate the "use avatar as reference" toggle. Unknown models (no
  // capability data cached yet) default to allowed rather than blocking.
  let modelSupportsImg2Img = $derived.by(() => {
    if (!modelOverride) return true;
    const m = enabledImageModels.find(e => e.model_id === modelOverride);
    return m?.img2img_supported ?? true;
  });

  // Community-favorite checkpoints as of this writing (Civitai + AI Horde
  // usage) worth surfacing above the raw alphabetical list — Pony/AAM for
  // anime-leaning character art like Aria's, Juggernaut/RealVis for more
  // painterly/semi-realistic scenes. Matched fuzzily since AI Horde's exact
  // registered model name can vary slightly from the Civitai listing name.
  const RECOMMENDED_MODEL_PATTERNS = ['pony', 'aam', 'juggernaut', 'realvis', 'albedo'];
  function isRecommendedModel(modelId: string): boolean {
    const lower = modelId.toLowerCase();
    return RECOMMENDED_MODEL_PATTERNS.some(p => lower.includes(p));
  }
  let sortedImageModels = $derived(
    [...enabledImageModels].sort((a, b) => {
      const ra = isRecommendedModel(a.model_id), rb = isRecommendedModel(b.model_id);
      if (ra !== rb) return ra ? -1 : 1;
      return a.model_id.localeCompare(b.model_id);
    })
  );

  // img2img: anchor the generation to the primary character's avatar
  // instead of generating purely from text. Off by default (avatars are
  // usually portraits, which bias every output toward a portrait framing —
  // see the img2img composition-bias caveat), and only offered when there's
  // an avatar to use and the resolved model supports it.
  let useAvatarReference = $state(false);
  // 0.6 kept too much of the avatar's exact pose/crop (avatars are almost
  // always tight portrait shots), so every scene generation came out as a
  // close-up of the character instead of the actual described scene.
  // 0.75 lets the text prompt drive composition while the avatar still
  // contributes a likeness/style cue rather than dictating the framing.
  let denoisingStrength = $state(0.75);

  // Multi-character portrait conditioning — relevant when the resolved
  // default provider's adapter is 'comfy_ui' (the {{CHARACTER_IMAGE_n}}
  // placeholder tokens in the user's own workflow — see providers::comfyui
  // on the backend) or 'wan_gp' (WanGP attaches them as reference images —
  // see providers::wangp). Every other adapter keeps the existing single
  // "use avatar as reference" toggle above, unchanged.
  let providerAdapter: string | null = $state(null);
  let sceneCast: { characterId: string; name: string; avatarPath: string | null; role: string }[] = $state([]);
  let selectedCastIds = $state<Set<string>>(new Set());
  let castThumbUrls: Record<string, string> = $state({});

  // Video generation — same shape as the image path above, but sourced from
  // whichever provider is default for provider_type "video" (currently only
  // ever WanGP; there's no video-capable ComfyUI/AI Horde path today).
  let videoProviderAdapter: string | null = $state(null);
  let sceneVideoUrl: string | null = $state(null);
  let currentVideoSceneId: string | null = $state(null);
  let videoCaption = $state('No video generated yet');
  let videoDuration = $state(4);
  let videoFps = $state(24);

  /** Whichever adapter is relevant to the currently-active tab — drives the
   *  shared Cast Portraits picker below, since both ComfyUI (image) and
   *  WanGP (image or video) use it, just interpreting the images
   *  differently on the backend. */
  let currentTabAdapter = $derived(activeTab === 'image' ? providerAdapter : videoProviderAdapter);

  // "Unconfirmed" cast members (role 'transient') are excluded — same
  // semantics as the "Unconfirmed" badge elsewhere in this app (see
  // ContextNpcPanel): they haven't survived the two-pass NPC detector yet,
  // so their portrait (if any) is likely a placeholder, not a real one.
  let comfyEligibleCast = $derived(sceneCast.filter(m => m.avatarPath && m.role !== 'transient'));

  async function loadProviderAdapter() {
    try {
      const ipc = await import('$lib/services/ipc');
      const providers = await ipc.listProviders('image');
      const def = providers.find(p => p.is_default) ?? providers[0];
      providerAdapter = def?.adapter ?? null;
    } catch (err) {
      console.error('Failed to load image provider adapter:', err);
      providerAdapter = null;
    }
  }

  async function loadVideoProviderAdapter() {
    try {
      const ipc = await import('$lib/services/ipc');
      const providers = await ipc.listProviders('video');
      const def = providers.find(p => p.is_default) ?? providers[0];
      videoProviderAdapter = def?.adapter ?? null;
    } catch (err) {
      console.error('Failed to load video provider adapter:', err);
      videoProviderAdapter = null;
    }
  }

  async function loadSceneCast(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      sceneCast = await ipc.listSceneCastMembers(convId);
      await loadCastThumbnails(sceneCast);
    } catch (err) {
      console.error('Failed to load scene cast members:', err);
      sceneCast = [];
    }
  }

  /** Thumbnails are keyed by characterId (not cached across conversations
   *  like chat.ts's avatar cache) since this list is small and conversation-
   *  scoped — simplest to just reload/revoke wholesale on every switch. */
  async function loadCastThumbnails(members: typeof sceneCast) {
    const { loadFileAsBlobUrl, revokeIfSet } = await import('$lib/utils/blobUrl');
    for (const url of Object.values(castThumbUrls)) revokeIfSet(url);
    const next: Record<string, string> = {};
    await Promise.all(members.filter(m => m.avatarPath).map(async (m) => {
      try {
        next[m.characterId] = await loadFileAsBlobUrl(m.avatarPath!, 'image/png');
      } catch (err) {
        console.error(`Failed to load cast thumbnail for ${m.name}:`, err);
      }
    }));
    castThumbUrls = next;
  }

  function toggleCastMember(characterId: string, checked: boolean) {
    const next = new Set(selectedCastIds);
    if (checked) next.add(characterId); else next.delete(characterId);
    selectedCastIds = next;
  }

  // Scene extraction is told to use the literal "{{user}}" token to refer
  // to the player character in `characters_present` — resolving it here
  // (rather than leaving it literal) matters beyond display: this feeds
  // straight into the actual image-generation prompt text via
  // `buildAutoPrompt` below, and a raw "{{user}}" token sent to a
  // text-to-image model is meaningless.
  let personaName: string | null = $state(null);
  let personaDescription: string | null = $state(null);
  $effect(() => {
    const convId = $activeConversationId;
    if (convId && isTauri) {
      resolvePersona(convId);
    } else {
      personaName = null;
      personaDescription = null;
    }
  });

  async function resolvePersona(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const conv = await ipc.getConversation(convId);
      const personaId = (conv as unknown as { persona_id: string | null }).persona_id;
      if (personaId) {
        const persona = await ipc.getPersona(personaId);
        const { parseCharacterData } = await import('$lib/utils/character');
        personaName = persona.name;
        personaDescription = (parseCharacterData(persona.data).description as string) || null;
      } else {
        personaName = null;
        personaDescription = null;
      }
    } catch {
      personaName = null;
      personaDescription = null;
    }
  }

  /** Best-available description for a character mentioned in a scene —
   *  matches the primary character or the known extra cast by name, and
   *  falls back to just the bare name for anyone not in either (a one-off
   *  NPC the AI introduced that was never explicitly added to the cast). */
  function characterDescriptor(rawName: string): string {
    const trim = (s: string) => s.length > 160 ? s.slice(0, 160).trimEnd() + '…' : s;
    if (/^\{\{user\}\}$/i.test(rawName.trim())) {
      const name = personaName || 'the user';
      return personaDescription ? `${name} (${trim(personaDescription)})` : name;
    }
    const name = rawName;
    if (name === characterName && characterDescription) {
      return `${name} (${trim(characterDescription)})`;
    }
    const extra = additionalCharacters.find(c => c.name === name);
    if (extra?.description) return `${name} (${trim(extra.description)})`;
    return name;
  }

  // Generation status lives in a shared store keyed by conversation_id (not
  // local component state) — it needs to survive this component remounting
  // (e.g. switching chats and back, or the context panel key-remounting) and
  // stay visible to other components like the scene gallery.
  let genState = $derived(getSceneGenerationState($sceneGenerations, $activeConversationId));
  let isLoading = $derived(genState.isLoading);
  let progressLabel = $derived(describeProgress(genState.progress));

  // Reloads the latest scene once a generation for the active conversation
  // completes — fires even if this component instance didn't start it (e.g.
  // it finished while the user was on a different chat).
  let lastHandledCompletion: number | null = $state(null);
  $effect(() => {
    const convId = $activeConversationId;
    if (convId && genState.completedAt && genState.completedAt !== lastHandledCompletion) {
      lastHandledCompletion = genState.completedAt;
      loadLatestScene(convId);
    }
  });

  // Load existing scenes when conversation changes
  $effect(() => {
    const convId = $activeConversationId;
    useAvatarReference = false;
    selectedCastIds = new Set();
    if (convId && isTauri) {
      loadLatestScene(convId);
      loadPresetSelection(convId);
      loadSceneCast(convId);
    } else {
      sceneImageUrl = null;
      sceneCaption = 'No scene generated yet';
      currentSceneId = null;
      sceneVideoUrl = null;
      videoCaption = 'No video generated yet';
      currentVideoSceneId = null;
      selectedPresetId = null;
      sceneCast = [];
    }
  });

  onMount(() => {
    if (!isTauri) return;
    import('$lib/services/ipc').then(async (ipc) => {
      const [rows, enabled] = await Promise.all([ipc.listImagePresets(), ipc.listEnabledModels()]);
      presets = rows.map(p => ({ id: p.id, name: p.name, model: p.model }));
      enabledImageModels = enabled.filter(m => m.model_type === 'image').map(m => ({
        model_id: m.model_id, img2img_supported: m.img2img_supported ?? null,
      }));
    }).catch(err => console.error('Failed to load image presets/models:', err));
    loadProviderAdapter();
    loadVideoProviderAdapter();
  });

  async function loadPresetSelection(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const conv = await ipc.getConversation(convId);
      selectedPresetId = conv.image_preset_id ?? null;
    } catch (err) {
      console.error('Failed to load conversation preset selection:', err);
    }
  }

  async function handlePresetChange(value: string) {
    const convId = $activeConversationId;
    if (!convId) return;
    const presetId = value || null;
    selectedPresetId = presetId;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.setConversationImagePreset(convId, presetId);
    } catch (err) {
      console.error('Failed to set conversation preset:', err);
    }
  }

  // Auto-fills the model override from whichever preset is selected — reacts
  // to `presets` too so it self-corrects if the preset list is still loading
  // when `selectedPresetId` first arrives, without re-running (and clobbering
  // a manual override) on anything else.
  let lastAutoFilledPresetId: string | null | undefined = undefined;
  $effect(() => {
    if (selectedPresetId !== lastAutoFilledPresetId || presets.length > 0) {
      lastAutoFilledPresetId = selectedPresetId;
      modelOverride = presets.find(p => p.id === selectedPresetId)?.model ?? null;
    }
  });

  // Auto-generate a scene image whenever the backend detects a meaningful
  // narrative scene change (location/mood/etc.) — gated by the "Auto-Generate
  // Images" setting. Deliberately NOT triggered per-message: that would be
  // both spammy and, for a paid/kudos-metered image provider, wasteful.
  onMount(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<SceneState>('scene_state_changed', (event) => {
        if (!get(settings).autoGenerateImages) return;
        const convId = get(activeConversationId);
        if (!convId || isLoading) return;
        if (providerAdapter === 'comfy_ui' || providerAdapter === 'wan_gp') autoSelectCastFromScene(event.payload);
        handleAutoGenerate(convId, buildAutoPrompt(event.payload));
      }).then(fn => { unlisten = fn; });
    });
    return () => unlisten?.();
  });

  // Tracks the currently-displayed blob URL so it can be revoked before the
  // next one is created (otherwise each generation/switch leaks one) — and
  // on unmount, since the context panel remounts this component on every
  // conversation switch (see the {#key} wrapper in +page.svelte).
  let currentImageBlobUrl: string | null = null;
  let currentVideoBlobUrl: string | null = null;
  onDestroy(() => {
    if (currentImageBlobUrl) URL.revokeObjectURL(currentImageBlobUrl);
    if (currentVideoBlobUrl) URL.revokeObjectURL(currentVideoBlobUrl);
    for (const url of Object.values(castThumbUrls)) URL.revokeObjectURL(url);
  });

  /** Reads a scene's PNG bytes and turns them into a blob: URL for <img src>.
   *  NOT convertFileSrc()/asset:// — the app's CSP only allows `img-src 'self'
   *  blob: data:`, so an asset:// URL is silently blocked by the browser and
   *  renders as a broken image. This mirrors how character avatars are
   *  loaded elsewhere in the app (see chat.ts's resolveCachedAvatarUrl). */
  async function loadSceneImageBlob(fileRelative: string): Promise<string | null> {
    try {
      const { loadFileAsBlobUrl, revokeIfSet } = await import('$lib/utils/blobUrl');
      const url = await loadFileAsBlobUrl(fileRelative, 'image/png');
      revokeIfSet(currentImageBlobUrl);
      currentImageBlobUrl = url;
      return currentImageBlobUrl;
    } catch (err) {
      console.error('Failed to load scene image:', err);
      return null;
    }
  }

  /** Same as `loadSceneImageBlob`, for video — needs `media-src 'self' blob:
   *  data:` in the CSP (see tauri.conf.json) for `<video src="blob:...">`
   *  to actually play instead of being silently blocked. */
  async function loadSceneVideoBlob(fileRelative: string): Promise<string | null> {
    try {
      const { loadFileAsBlobUrl, revokeIfSet } = await import('$lib/utils/blobUrl');
      const url = await loadFileAsBlobUrl(fileRelative, 'video/mp4');
      revokeIfSet(currentVideoBlobUrl);
      currentVideoBlobUrl = url;
      return currentVideoBlobUrl;
    } catch (err) {
      console.error('Failed to load scene video:', err);
      return null;
    }
  }

  /** Builds a prompt from the current scene state for auto-generation. */
  function buildAutoPrompt(state: SceneState): string {
    const parts = [state.location_description || state.location_name];
    if (state.time_period && state.time_period !== 'unspecified') parts.push(state.time_period);
    if (state.weather) parts.push(state.weather);
    if (state.characters_present?.length) {
      parts.push(`featuring ${state.characters_present.map(characterDescriptor).join(', ')}`);
    }
    if (state.scene_mood && state.scene_mood !== 'neutral') parts.push(`${state.scene_mood} atmosphere`);
    if (state.ambient_details) parts.push(state.ambient_details);
    return parts.filter(Boolean).join(', ');
  }

  /** Default-selects whichever eligible cast portraits match the scene's
   *  `characters_present` names, so automatic ComfyUI generation reasonably
   *  picks the right images without the user manually reselecting every
   *  time. Manual generation is untouched — it just uses whatever was last
   *  selected (or nothing). `{{user}}` resolves to the persona name, same
   *  as `characterDescriptor` above. */
  function autoSelectCastFromScene(state: SceneState) {
    const present = state.characters_present ?? [];
    if (present.length === 0) return;
    const names = present.map(n => /^\{\{user\}\}$/i.test(n.trim()) ? (personaName || '') : n);
    const matched = comfyEligibleCast.filter(m => names.some(n => n && n.toLowerCase() === m.name.toLowerCase()));
    if (matched.length > 0) selectedCastIds = new Set(matched.map(m => m.characterId));
  }

  /** Shared options for both manual and auto generation — includes the
   *  img2img reference only when the toggle is on, an avatar is available,
   *  and the resolved model actually supports it. */
  function buildGenOptions() {
    const opts: {
      width: number; height: number; modelOverride?: string; referenceImagePath?: string;
      denoisingStrength?: number; allowNsfw?: boolean;
      characterImages?: { characterId: string; characterName: string; relativePath: string }[];
    } = {
      width: 512, height: 512,
      allowNsfw: get(settings).allowMatureContent,
    };
    if (modelOverride) opts.modelOverride = modelOverride;
    if ((providerAdapter === 'comfy_ui' || providerAdapter === 'wan_gp') && selectedCastIds.size > 0) {
      opts.characterImages = comfyEligibleCast
        .filter(m => selectedCastIds.has(m.characterId))
        .map(m => ({ characterId: m.characterId, characterName: m.name, relativePath: m.avatarPath! }));
    } else if (useAvatarReference && avatarPath && modelSupportsImg2Img) {
      opts.referenceImagePath = avatarPath;
      opts.denoisingStrength = denoisingStrength;
    }
    return opts;
  }

  /** Video counterpart to `buildGenOptions` — WanGP is currently the only
   *  video-capable adapter, and it's the only one whose Cast Portraits
   *  picker can appear while the Video tab is active (see `currentTabAdapter`). */
  function buildVideoGenOptions() {
    const opts: {
      width: number; height: number; durationSeconds: number; fps: number;
      allowNsfw?: boolean;
      characterImages?: { characterId: string; characterName: string; relativePath: string }[];
    } = {
      width: 1280, height: 720, durationSeconds: videoDuration, fps: videoFps,
      allowNsfw: get(settings).allowMatureContent,
    };
    if (videoProviderAdapter === 'wan_gp' && selectedCastIds.size > 0) {
      opts.characterImages = comfyEligibleCast
        .filter(m => selectedCastIds.has(m.characterId))
        .map(m => ({ characterId: m.characterId, characterName: m.name, relativePath: m.avatarPath! }));
    }
    return opts;
  }

  /** Same generation flow as handleGenerate(), but silent and non-destructive
   *  toward whatever the user may be typing in the manual prompt field. */
  async function handleAutoGenerate(convId: string, prompt: string) {
    if (!prompt.trim() || get(sceneGenerations)[convId]?.isLoading) return;
    try {
      await runSceneGeneration(convId, prompt, buildGenOptions());
    } catch (err) {
      if (!isCancellationError(err)) console.error('Auto scene generation failed:', err);
    }
  }

  async function loadLatestScene(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const scenes = await ipc.listScenes(convId); // sorted DESC by created_at, mixed media types
      const latestImage = scenes.find(s => s.media_type !== 'video');
      if (latestImage) {
        currentSceneId = latestImage.id;
        sceneCaption = latestImage.caption || latestImage.prompt;
        promptText = latestImage.prompt;
        sceneImageUrl = await loadSceneImageBlob(latestImage.file_path);
      } else {
        sceneImageUrl = null;
        sceneCaption = 'No scene generated yet';
        currentSceneId = null;
      }

      const latestVideo = scenes.find(s => s.media_type === 'video');
      if (latestVideo) {
        currentVideoSceneId = latestVideo.id;
        videoCaption = latestVideo.caption || latestVideo.prompt;
        sceneVideoUrl = await loadSceneVideoBlob(latestVideo.file_path);
      } else {
        sceneVideoUrl = null;
        videoCaption = 'No video generated yet';
        currentVideoSceneId = null;
      }
    } catch (err) {
      console.error('Failed to load scenes:', err);
    }
  }

  /** Falls back to the current scene state (same description auto-generate
   *  would use) when the user hasn't typed a manual prompt, instead of a
   *  generic placeholder unrelated to the actual scene/characters. */
  async function resolveDefaultPrompt(convId: string): Promise<string> {
    const fallback = 'A detailed scene from the current conversation';
    if (!isTauri) return fallback;
    try {
      const ipc = await import('$lib/services/ipc');
      const state = await ipc.getSceneState(convId);
      if (state) {
        const built = buildAutoPrompt(state);
        if (built) return built;
      }

      // No structured scene state yet — happens for conversations started
      // before scene extraction existed, or when the background extraction
      // from the greeting/last reply just hasn't finished. Use the latest
      // in-character reply itself as the prompt rather than a placeholder
      // that carries zero information about the actual scene/characters,
      // and kick off extraction now so the next generation gets the richer
      // structured prompt.
      const history = await ipc.getConversationMessages(convId);
      const lastAssistant = [...history].reverse().find(m => m.role === 'assistant' && m.content.trim());
      if (lastAssistant) {
        ipc.extractInitialScene(convId, lastAssistant.content).catch(() => {});
        return lastAssistant.content.trim();
      }
    } catch (err) {
      console.error('Failed to load scene state for default prompt:', err);
    }
    return fallback;
  }

  async function handleGenerate() {
    const convId = $activeConversationId;
    if (!convId || get(sceneGenerations)[convId]?.isLoading) return;

    isEditingPrompt = false;
    const prompt = promptText.trim() || await resolveDefaultPrompt(convId);

    if (!isTauri) {
      // Dev mode preview (outside Tauri) — no real backend to call.
      sceneCaption = `${prompt} — generated (mock)`;
      return;
    }

    try {
      await runSceneGeneration(convId, prompt, buildGenOptions());
    } catch (err) {
      if (isCancellationError(err)) {
        sceneCaption = 'Generation stopped';
      } else {
        console.error('Failed to generate scene:', err);
        sceneCaption = `Generation failed: ${humanizeProviderError((err as { message?: string } | null)?.message ?? '')}`;
      }
    }
  }

  /** Distinguishes a user-initiated Stop from an actual failure — without
   *  this, cancelling looked identical to an error ("check your provider
   *  settings"), which gave no confirmation Stop had worked at all. */
  function isCancellationError(err: unknown): boolean {
    const msg = (err as { message?: string } | null)?.message ?? '';
    return msg.toLowerCase().includes('cancelled');
  }

  /** Video counterpart to `handleGenerate`. */
  async function handleGenerateVideo() {
    const convId = $activeConversationId;
    if (!convId || get(sceneGenerations)[convId]?.isLoading || !videoProviderAdapter) return;

    isEditingPrompt = false;
    const prompt = promptText.trim() || await resolveDefaultPrompt(convId);

    if (!isTauri) {
      videoCaption = `${prompt} — generated (mock)`;
      return;
    }

    try {
      await runVideoSceneGeneration(convId, prompt, buildVideoGenOptions());
    } catch (err) {
      if (isCancellationError(err)) {
        videoCaption = 'Generation stopped';
      } else {
        console.error('Failed to generate video scene:', err);
        videoCaption = `Generation failed: ${humanizeProviderError((err as { message?: string } | null)?.message ?? '')}`;
      }
    }
  }

  async function handleRegenerate() {
    if (activeTab === 'video') await handleGenerateVideo();
    else await handleGenerate();
  }

  let isCancelling = $state(false);

  async function handleStopGeneration() {
    const convId = $activeConversationId;
    if (!convId || isCancelling) return;
    isCancelling = true;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.cancelSceneGeneration(convId);
    } catch (err) {
      console.error('Failed to cancel scene generation:', err);
    }
    isCancelling = false;
  }

  async function handleSave() {
    const isVideo = activeTab === 'video';
    const sceneId = isVideo ? currentVideoSceneId : currentSceneId;
    if (isVideo ? !sceneVideoUrl : !sceneImageUrl) return;
    // Open the image/video in a save dialog
    if (isTauri && sceneId) {
      try {
        const { save } = await import('@tauri-apps/plugin-dialog');
        const ipc = await import('$lib/services/ipc');
        const scenes = await ipc.listScenes($activeConversationId);
        const scene = scenes.find(s => s.id === sceneId);
        if (scene) {
          const absPath = await ipc.getScenePath(scene.file_path);
          // Copy to user-selected path
          const dest = isVideo
            ? await save({ defaultPath: `scene-${sceneId}.mp4`, filters: [{ name: 'MP4 Video', extensions: ['mp4'] }] })
            : await save({ defaultPath: `scene-${sceneId}.png`, filters: [{ name: 'PNG Image', extensions: ['png'] }] });
          if (dest) {
            const { copyFile } = await import('@tauri-apps/plugin-fs');
            await copyFile(absPath, dest);
          }
        }
      } catch (err) {
        console.error('Failed to save scene:', err);
      }
    }
  }

  function handleEditPrompt() {
    isEditingPrompt = !isEditingPrompt;
  }
</script>

<div class="scene-display">
  <!-- Header with toggle -->
  <div class="scene-header">
    <span class="scene-title">SCENE</span>
    <div class="scene-toggle" role="tablist" aria-label="Scene media type">
      <button 
        class="toggle-btn" 
        class:active={activeTab === 'image'}
        onclick={() => activeTab = 'image'}
        role="tab"
        aria-selected={activeTab === 'image'}
        aria-controls="scene-panel"
      >
        <Icon name="image" size={12} color={activeTab === 'image' ? '#FFFFFF' : 'var(--fg-muted)'} />
        <span>Image</span>
      </button>
      {#if videoProviderAdapter}
        <button
          class="toggle-btn"
          class:active={activeTab === 'video'}
          onclick={() => activeTab = 'video'}
          role="tab"
          aria-selected={activeTab === 'video'}
          aria-controls="scene-panel"
        >
          <Icon name="video" size={12} color={activeTab === 'video' ? '#FFFFFF' : 'var(--fg-muted)'} />
          <span>Video</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Scene Frame -->
  <div class="scene-frame" class:loading={isLoading} id="scene-panel" role="tabpanel">
    {#if isLoading}
      <div class="scene-loading animate-shimmer">
        <Icon name={activeTab === 'video' ? 'video' : 'image'} size={24} color="var(--fg-muted)" />
        <span class="loading-text">{progressLabel}</span>
        {#if genState.progress && 'is_possible' in genState.progress && genState.progress.is_possible === false}
          <span class="loading-subtext">No matching worker online right now — still waiting</span>
        {/if}
        <button class="stop-gen-btn" onclick={handleStopGeneration} disabled={isCancelling}>
          <Icon name="x" size={11} color="#F43F5E" />
          {isCancelling ? 'Stopping…' : 'Stop'}
        </button>
      </div>
    {:else if activeTab === 'image'}
      <div class="scene-image">
        {#if sceneImageUrl}
          <img src={sceneImageUrl} alt={sceneCaption} class="generated-image" />
        {:else}
          <div class="scene-placeholder" onclick={handleGenerate} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleGenerate()}>
            <div class="scene-overlay">
              <Icon name="sparkles" size={20} color="rgba(255,255,255,0.6)" />
              <span class="gen-hint">Click to generate</span>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="scene-video">
        {#if sceneVideoUrl}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={sceneVideoUrl} controls class="generated-video"></video>
        {:else if videoProviderAdapter}
          <div class="scene-placeholder" onclick={handleGenerateVideo} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleGenerateVideo()}>
            <div class="scene-overlay">
              <Icon name="sparkles" size={20} color="rgba(255,255,255,0.6)" />
              <span class="gen-hint">Click to generate</span>
            </div>
          </div>
        {:else}
          <div class="video-placeholder">
            <Icon name="video" size={32} color="var(--fg-muted)" />
            <span class="video-text">No video provider configured</span>
            <a href="/providers" class="generate-video-btn">Add one in Settings →</a>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Prompt Editor -->
  {#if isEditingPrompt}
    <div class="prompt-editor">
      <textarea
        class="prompt-input"
        bind:value={promptText}
        placeholder="Describe the scene to generate..."
        rows="2"
      ></textarea>
      <button class="prompt-go-btn" onclick={activeTab === 'video' ? handleGenerateVideo : handleGenerate} disabled={isLoading}>
        <Icon name="sparkles" size={12} color="#FFFFFF" />
        Generate
      </button>
    </div>
  {/if}

  <!-- Caption -->
  <div class="scene-caption">
    <span class="caption-text">{activeTab === 'video' ? videoCaption : sceneCaption}</span>
  </div>

  {#if activeTab === 'image'}
    <!-- Preset Picker -->
    <div class="preset-picker">
      <span class="preset-picker-label">Style Preset</span>
      {#if presets.length > 0}
        <select
          class="preset-picker-select"
          value={selectedPresetId ?? ''}
          onchange={(e) => handlePresetChange(e.currentTarget.value)}
          aria-label="Image generation preset for this chat"
        >
          <option value="">Use Default</option>
          {#each presets as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
        </select>
      {:else}
        <a href="/settings" class="preset-picker-link">Add one in Settings →</a>
      {/if}
    </div>

    <!-- Model Override -->
    <div class="preset-picker">
      <span class="preset-picker-label">Model</span>
      {#if enabledImageModels.length > 0}
        <select
          class="preset-picker-select"
          value={modelOverride ?? ''}
          onchange={(e) => modelOverride = e.currentTarget.value || null}
          aria-label="Model for this generation"
        >
          <option value="">Use Default</option>
          {#each sortedImageModels as m}
            <option value={m.model_id}>{isRecommendedModel(m.model_id) ? `★ ${m.model_id}` : m.model_id}</option>
          {/each}
        </select>
      {:else}
        <a href="/models" class="preset-picker-link">Enable models →</a>
      {/if}
    </div>
  {:else}
    <!-- Duration / FPS — video-only, mirrors the image tab's minimal
         generation-options footprint (no resolution controls there either). -->
    <div class="preset-picker">
      <span class="preset-picker-label">Duration</span>
      <input
        class="ref-strength-input"
        type="range" min="1" max="10" step="0.5"
        bind:value={videoDuration}
        aria-label="Clip length in seconds"
      />
      <span class="ref-strength-value">{videoDuration.toFixed(1)}s</span>
    </div>
    <div class="preset-picker">
      <span class="preset-picker-label">FPS</span>
      <select
        class="preset-picker-select"
        value={videoFps}
        onchange={(e) => videoFps = Number(e.currentTarget.value)}
        aria-label="Frames per second"
      >
        <option value={16}>16</option>
        <option value={24}>24</option>
        <option value={30}>30</option>
      </select>
    </div>
  {/if}

  <!-- Multi-character portrait conditioning (ComfyUI image, or WanGP image/video) -->
  {#if currentTabAdapter === 'comfy_ui' || currentTabAdapter === 'wan_gp'}
    <div class="preset-picker">
      <span class="preset-picker-label">Cast Portraits</span>
      {#if comfyEligibleCast.length === 0}
        <span class="preset-picker-hint">No confirmed cast portraits yet</span>
      {/if}
    </div>
    {#if comfyEligibleCast.length > 0}
      <div class="cast-picker-grid">
        {#each comfyEligibleCast as member (member.characterId)}
          <label class="cast-chip" class:selected={selectedCastIds.has(member.characterId)}>
            <input type="checkbox" checked={selectedCastIds.has(member.characterId)}
              onchange={(e) => toggleCastMember(member.characterId, e.currentTarget.checked)} />
            {#if castThumbUrls[member.characterId]}
              <img src={castThumbUrls[member.characterId]} alt={member.name} class="cast-chip-thumb" />
            {/if}
            <span class="cast-chip-name">{member.name}</span>
          </label>
        {/each}
      </div>
      <span class="preset-picker-hint">
        {currentTabAdapter === 'comfy_ui'
          ? "Sent to your workflow's {{CHARACTER_IMAGE_n}} tokens, in the order selected here"
          : 'Sent to WanGP as reference images for this scene'}
      </span>
    {/if}
  {:else if activeTab === 'image' && avatarPath}
    <!-- img2img Reference -->
    <div class="preset-picker">
      <span class="preset-picker-label">Avatar Reference</span>
      {#if modelSupportsImg2Img}
        <label class="ref-toggle">
          <input type="checkbox" bind:checked={useAvatarReference} />
          <span>Anchor to {characterName || 'character'}'s avatar</span>
        </label>
      {:else}
        <span class="preset-picker-hint">Not supported by this model</span>
      {/if}
    </div>
    {#if useAvatarReference && modelSupportsImg2Img}
      <div class="preset-picker">
        <span class="preset-picker-label">Reference Strength</span>
        <input
          class="ref-strength-input"
          type="range" min="0.2" max="0.9" step="0.05"
          bind:value={denoisingStrength}
          aria-label="How closely to match the avatar vs. the new scene"
        />
        <span class="ref-strength-value">{denoisingStrength.toFixed(2)}</span>
      </div>
      <span class="preset-picker-hint">Lower = closer to the avatar's exact pose/crop, higher = more freedom to match the described scene</span>
    {/if}
  {/if}

  <!-- Actions -->
  <div class="scene-actions">
    <button class="scene-action-btn" onclick={handleRegenerate} aria-label="Regenerate scene" disabled={isLoading}>
      <Icon name="refresh-cw" size={10} color="var(--fg-muted)" />
      <span>Regenerate</span>
    </button>
    <button class="scene-action-btn" onclick={handleSave} aria-label="Save scene" disabled={activeTab === 'video' ? !sceneVideoUrl : !sceneImageUrl}>
      <Icon name="download" size={10} color="var(--fg-muted)" />
      <span>Save</span>
    </button>
    <button class="scene-action-btn" onclick={handleEditPrompt} aria-label="Edit scene prompt" class:active={isEditingPrompt}>
      <Icon name="pencil" size={10} color={isEditingPrompt ? 'var(--accent-primary)' : 'var(--fg-muted)'} />
      <span>Edit Prompt</span>
    </button>
  </div>
</div>

<style>
  .scene-display {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .scene-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .scene-title {
    font-size: clamp(10px, 2.6cqi, 13px);
    font-weight: 600;
    color: var(--fg-muted);
    font-family: var(--font-mono);
    letter-spacing: 1px;
  }

  /* Toggle — Light Carousel chips: independent floating pills, not an
     enclosed segmented-control capsule (dimming/enclosing the inactive one
     reads as disabled). Active chip pops forward with a solid fill + glow. */
  .scene-toggle {
    display: flex;
    gap: 6px;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: clamp(5px, 1.2cqi, 8px) clamp(10px, 2.5cqi, 16px);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border-subtle);
    color: var(--fg-muted);
    font-size: clamp(10px, 2.6cqi, 13px);
    font-family: var(--font-body);
    cursor: pointer;
    transition: all 220ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .toggle-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.18);
    color: var(--fg-secondary);
  }

  .toggle-btn.active {
    transform: scale(1.05);
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: #FFFFFF;
    font-weight: 600;
    box-shadow: 0 6px 16px -6px var(--accent-primary);
  }

  /* Scene Frame — square aspect-ratio (generated images are 512x512) instead
     of a fixed height, so the frame scales with the panel's width instead of
     forcing object-fit: cover to crop more of the image as it widens. */
  .scene-frame {
    width: 100%;
    aspect-ratio: 1;
    border-radius: var(--rounded-md);
    background: var(--surface-card);
    border: 1px solid var(--border-subtle);
    overflow: hidden;
    position: relative;
  }

  .scene-image, .scene-video {
    width: 100%;
    height: 100%;
  }

  .generated-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .generated-video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    background: #000;
  }

  .scene-placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(
      135deg,
      #1a0a2e 0%,
      #2d1b69 30%,
      #8B5CF620 60%,
      #1a1a2e 100%
    );
    position: relative;
    cursor: pointer;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .scene-placeholder:hover {
    opacity: 0.85;
  }

  .scene-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .gen-hint {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: rgba(255, 255, 255, 0.5);
    font-family: var(--font-mono);
  }

  /* Loading */
  .scene-loading {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .loading-text {
    font-size: clamp(11px, 2.8cqi, 15px);
    color: var(--fg-muted);
    font-family: var(--font-mono);
    text-align: center;
    padding: 0 12px;
  }

  .loading-subtext {
    font-size: clamp(9px, 2.2cqi, 11px);
    color: rgba(245, 158, 11, 0.8);
    text-align: center;
    padding: 0 16px;
  }

  .stop-gen-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 4px;
    padding: 4px 10px;
    border-radius: var(--rounded-sm);
    border: 1px solid rgba(244, 63, 94, 0.3);
    background: rgba(244, 63, 94, 0.08);
    color: #F43F5E;
    font-size: clamp(9px, 2.2cqi, 11px);
    font-weight: 600;
    font-family: var(--font-body);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .stop-gen-btn:hover:not(:disabled) {
    background: rgba(244, 63, 94, 0.16);
  }
  .stop-gen-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* Video placeholder */
  .video-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: var(--surface-card);
  }

  .video-text {
    font-size: clamp(11px, 2.8cqi, 15px);
    color: var(--fg-muted);
  }

  .generate-video-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: clamp(6px, 1.4cqi, 10px) clamp(12px, 3cqi, 18px);
    border-radius: var(--rounded-md);
    background: var(--accent-tertiary);
    border: none;
    color: #000;
    font-size: clamp(11px, 2.8cqi, 15px);
    font-weight: 600;
    font-family: var(--font-body);
    text-decoration: none;
    cursor: pointer;
    margin-top: 4px;
    transition: all var(--duration-fast) var(--ease-out);
  }

  .generate-video-btn:hover {
    opacity: 0.9;
    transform: scale(1.02);
  }

  /* Prompt Editor */
  .prompt-editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .prompt-input {
    width: 100%;
    padding: clamp(8px, 1.8cqi, 12px) clamp(10px, 2.2cqi, 14px);
    border-radius: var(--rounded-sm);
    border: 1px solid var(--border-subtle);
    background: var(--surface-input);
    color: var(--fg-primary);
    font-size: clamp(11px, 2.8cqi, 15px);
    font-family: var(--font-body);
    resize: vertical;
    min-height: 40px;
    outline: none;
    transition: border-color var(--duration-fast) var(--ease-out);
  }

  .prompt-input:focus {
    border-color: var(--accent-primary);
  }

  .prompt-input::placeholder {
    color: var(--fg-muted);
  }

  .prompt-go-btn {
    align-self: flex-end;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: clamp(5px, 1.2cqi, 8px) clamp(10px, 2.4cqi, 15px);
    border-radius: var(--rounded-sm);
    background: var(--accent-primary);
    border: none;
    color: #FFFFFF;
    font-size: clamp(10px, 2.6cqi, 13px);
    font-weight: 600;
    font-family: var(--font-body);
    transition: all var(--duration-fast) var(--ease-out);
  }

  .prompt-go-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .prompt-go-btn:disabled {
    opacity: 0.5;
  }

  /* Caption */
  .scene-caption {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .caption-text {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Preset Picker */
  .preset-picker {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .preset-picker-label {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-muted);
    font-family: var(--font-mono);
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }

  .preset-picker-select {
    flex: 1;
    min-width: 0;
    max-width: 220px;
    height: clamp(26px, 6cqi, 32px);
    padding: 0 clamp(6px, 1.5cqi, 10px);
    border-radius: 7px;
    background: rgba(10,10,22,0.7);
    border: 1px solid var(--border-subtle);
    color: var(--fg-secondary);
    font-size: clamp(10px, 2.6cqi, 13px);
    font-family: var(--font-body);
    outline: none;
    cursor: pointer;
  }

  .preset-picker-select:focus {
    border-color: var(--accent-primary);
  }

  .preset-picker-link {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--accent-primary);
    font-weight: 600;
    text-decoration: none;
  }
  .preset-picker-link:hover {
    text-decoration: underline;
  }

  .preset-picker-hint {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-muted);
    font-style: italic;
  }

  .ref-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-secondary);
    cursor: pointer;
  }
  .ref-toggle input {
    appearance: none;
    -webkit-appearance: none;
    width: 15px;
    height: 15px;
    flex-shrink: 0;
    margin: 0;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    background: rgba(10, 10, 22, 0.7);
    cursor: pointer;
    position: relative;
    transition: background var(--duration-fast) var(--ease-out), border-color var(--duration-fast) var(--ease-out);
  }
  .ref-toggle input:hover {
    border-color: var(--accent-primary);
  }
  .ref-toggle input:checked {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
  }
  .ref-toggle input:checked::after {
    content: '';
    position: absolute;
    left: 4px;
    top: 1px;
    width: 4px;
    height: 8px;
    border: solid #fff;
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  .ref-toggle input:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 1px;
  }

  /* Cast Portrait Picker (ComfyUI) */
  .cast-picker-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .cast-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 4px 4px;
    border-radius: var(--rounded-md);
    border: 1px solid var(--border-subtle);
    background: var(--surface-card);
    cursor: pointer;
    transition: all var(--duration-fast) var(--ease-out);
  }
  .cast-chip:hover {
    border-color: var(--accent-primary);
  }
  .cast-chip.selected {
    border-color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 12%, var(--surface-card));
  }
  .cast-chip input[type="checkbox"] {
    accent-color: var(--accent-primary);
    cursor: pointer;
    margin: 0;
  }
  .cast-chip-thumb {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .cast-chip-name {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-secondary);
    white-space: nowrap;
  }

  .ref-strength-input {
    flex: 1;
    accent-color: var(--accent-primary);
    cursor: pointer;
  }
  .ref-strength-value {
    font-size: clamp(10px, 2.6cqi, 13px);
    color: var(--fg-muted);
    font-family: var(--font-mono);
    min-width: 2.5em;
    text-align: right;
  }

  /* Actions */
  .scene-actions {
    display: flex;
    gap: 6px;
  }

  .scene-action-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: clamp(4px, 1cqi, 7px) clamp(8px, 2cqi, 13px);
    border-radius: var(--rounded-sm);
    border: 1px solid var(--border-subtle);
    background: transparent;
    color: var(--fg-muted);
    font-size: clamp(10px, 2.6cqi, 13px);
    font-family: var(--font-body);
    transition: all var(--duration-fast) var(--ease-out);
  }

  .scene-action-btn:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--fg-secondary);
  }

  .scene-action-btn:disabled {
    opacity: 0.4;
  }

  .scene-action-btn.active {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }
</style>
