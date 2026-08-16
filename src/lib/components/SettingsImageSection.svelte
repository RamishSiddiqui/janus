<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { HORDE_SAMPLERS } from '$lib/constants/aiHorde';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let allowMatureContent = $state($settings.allowMatureContent);
  let autoGenerateNpcPortraits = $state($settings.autoGenerateNpcPortraits);
  let autoApproveNpcPortraits = $state($settings.autoApproveNpcPortraits);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { allowMatureContent, autoGenerateNpcPortraits, autoApproveNpcPortraits };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

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

  onMount(() => {
    loadImagePresets();
    loadEnabledImageModels();
  });
</script>

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

<style>
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
</style>
