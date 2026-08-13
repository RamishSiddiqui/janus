<script lang="ts">
  import Icon from './Icon.svelte';
  import { browser } from '$app/environment';
  import { error as toastError } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Soft cap — large images bloat the request and most vision models
  // downscale internally anyway, so there's no quality benefit to allowing
  // arbitrarily large uploads.
  const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
  const MAX_ATTACHMENTS = 4;

  export interface PendingAttachment {
    relativePath: string;
    mimeType: string;
    previewUrl: string;
  }

  let {
    value = $bindable(''), modelName, tokenCount, onSend, disabled = false,
    selectedModel = $bindable(''), availableModels = [],
    onRefreshModels, isBranching = false, onStop,
    pendingAttachments = $bindable([]),
    selectedModelSupportsVision = false,
  }: {
    value: string; modelName: string; tokenCount: string;
    onSend: () => void; disabled?: boolean;
    selectedModel?: string; availableModels?: string[];
    onRefreshModels?: () => void;
    isBranching?: boolean;
    /** Shown in place of the Send button while `disabled` (streaming) is true. */
    onStop?: () => void;
    /** Images picked but not yet sent — the parent reads this at send time
     *  and clears it afterward. */
    pendingAttachments?: PendingAttachment[];
    /** Gates the attach button — only vision-capable models can actually
     *  use an attached image as input. */
    selectedModelSupportsVision?: boolean;
  } = $props();

  let inputElement: HTMLTextAreaElement | undefined = $state();
  let focused = $state(false);
  let showModelPicker = $state(false);
  let modelFilter = $state('');
  let isUploadingAttachment = $state(false);

  async function handleAttachClick() {
    if (!isTauri || isUploadingAttachment) return;
    if (pendingAttachments.length >= MAX_ATTACHMENTS) {
      toastError(`You can attach up to ${MAX_ATTACHMENTS} images per message`);
      return;
    }
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({
      multiple: true,
      filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
    });
    if (!selected) return;
    const paths = (Array.isArray(selected) ? selected : [selected]).slice(
      0, MAX_ATTACHMENTS - pendingAttachments.length
    );

    isUploadingAttachment = true;
    try {
      const { stat } = await import('@tauri-apps/plugin-fs');
      const ipc = await import('$lib/services/ipc');
      const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
      for (const path of paths) {
        const info = await stat(path);
        if (info.size > MAX_ATTACHMENT_BYTES) {
          toastError(`${path.split(/[\\/]/).pop()} is too large (max 10MB)`);
          continue;
        }
        const attachment = await ipc.uploadMessageAttachment(path);
        const previewUrl = await loadFileAsBlobUrl(attachment.relativePath, attachment.mimeType);
        pendingAttachments = [...pendingAttachments, { ...attachment, previewUrl }];
      }
    } catch (err) {
      console.error('Failed to attach image:', err);
      toastError('Failed to attach image');
    }
    isUploadingAttachment = false;
  }

  function removePendingAttachment(index: number) {
    const removed = pendingAttachments[index];
    if (removed) URL.revokeObjectURL(removed.previewUrl);
    pendingAttachments = pendingAttachments.filter((_, i) => i !== index);
  }

  const EXT_BY_MIME: Record<string, string> = {
    'image/png': 'png', 'image/jpeg': 'jpg', 'image/webp': 'webp', 'image/gif': 'gif',
  };

  /** Handles pasting an image (e.g. a screenshot) directly into the
   *  textarea with Ctrl+V — the file-picker path above is for images that
   *  already exist on disk; this one has only raw clipboard bytes. Text
   *  pastes fall through untouched. */
  async function handlePaste(e: ClipboardEvent) {
    if (!isTauri || isUploadingAttachment) return;
    const items = e.clipboardData?.items;
    if (!items) return;
    const imageItem = Array.from(items).find(i => i.type.startsWith('image/'));
    if (!imageItem) return; // let normal text paste proceed

    e.preventDefault();
    if (pendingAttachments.length >= MAX_ATTACHMENTS) {
      toastError(`You can attach up to ${MAX_ATTACHMENTS} images per message`);
      return;
    }
    const ext = EXT_BY_MIME[imageItem.type];
    if (!ext) {
      toastError('Unsupported clipboard image type');
      return;
    }
    const blob = imageItem.getAsFile();
    if (!blob) return;
    if (blob.size > MAX_ATTACHMENT_BYTES) {
      toastError('Pasted image is too large (max 10MB)');
      return;
    }

    isUploadingAttachment = true;
    try {
      const bytes = new Uint8Array(await blob.arrayBuffer());
      const ipc = await import('$lib/services/ipc');
      const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
      const attachment = await ipc.uploadMessageAttachmentBytes(bytes, ext);
      const previewUrl = await loadFileAsBlobUrl(attachment.relativePath, attachment.mimeType);
      pendingAttachments = [...pendingAttachments, { ...attachment, previewUrl }];
    } catch (err) {
      console.error('Failed to attach pasted image:', err);
      toastError('Failed to attach pasted image');
    }
    isUploadingAttachment = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onSend(); }
  }

  function autoResize(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    target.style.height = 'auto';
    target.style.height = Math.min(target.scrollHeight, 160) + 'px';
  }

  let hasContent = $derived(value.trim().length > 0);

  let filteredModels = $derived(
    modelFilter
      ? availableModels.filter(m => m.toLowerCase().includes(modelFilter.toLowerCase()))
      : availableModels
  );

  function selectModel(model: string) {
    selectedModel = model;
    showModelPicker = false;
    modelFilter = '';
  }

  function togglePicker() {
    showModelPicker = !showModelPicker;
    if (showModelPicker && onRefreshModels) {
      onRefreshModels();
    }
    modelFilter = '';
  }

  // Close picker when clicking outside
  function handleWindowClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (showModelPicker && !target.closest('.ci-model-wrap')) {
      showModelPicker = false;
      modelFilter = '';
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<!-- B3 - TWO-ROW CARD with PILL BUTTON -->
<div class="ci-wrap" class:is-focused={focused} class:is-branching={isBranching} class:has-content={hasContent}>

  <!-- Branch indicator banner -->
  {#if isBranching}
    <div class="ci-branch-banner" role="status">
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>
        <path d="M18 9a9 9 0 0 1-9 9"/>
      </svg>
      Branching - your reply creates a new timeline
    </div>
  {/if}

  <!-- CARD -->
  <div class="ci-card">

    <!-- Pending attachment previews -->
    {#if pendingAttachments.length > 0}
      <div class="ci-attachments-row">
        {#each pendingAttachments as att, i}
          <div
            class="ci-attachment-chip"
            class:ci-attachment-unsupported={!selectedModelSupportsVision}
            title={selectedModelSupportsVision ? undefined : "Model doesn't support image input"}
          >
            <img src={att.previewUrl} alt="Attached" class="ci-attachment-thumb" />
            {#if !selectedModelSupportsVision}
              <span class="ci-attachment-unsupported-badge" aria-label="Model doesn't support image input">
                <Icon name="alert-circle" size={13} color="#FFFFFF" />
              </span>
            {/if}
            <button class="ci-attachment-remove" onclick={() => removePendingAttachment(i)} aria-label="Remove attachment" title="Remove">
              <Icon name="x" size={10} color="#FFFFFF" />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Textarea row -->
    <div class="ci-textarea-row">
      <textarea
        bind:this={inputElement}
        bind:value
        class="ci-textarea"
        placeholder="Write your response..."
        aria-label="Message input"
        rows="1"
        onkeydown={handleKeydown}
        oninput={autoResize}
        onfocus={() => focused = true}
        onblur={() => focused = false}
        onpaste={handlePaste}
      ></textarea>
    </div>

    <!-- Divider -->
    <div class="ci-divider" aria-hidden="true"></div>

    <!-- Toolbar row -->
    <div class="ci-toolbar">

      <!-- Left tools: paperclip / bold / sparkle -->
      <div class="ci-left-tools">
        <button
          class="ci-tool-btn"
          class:ci-tool-btn-disabled={isUploadingAttachment}
          onclick={handleAttachClick}
          disabled={isUploadingAttachment}
          title="Attach an image"
          aria-label="Attach file"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
          </svg>
        </button>
        <button class="ci-tool-btn" title="Bold" aria-label="Bold">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/>
          </svg>
        </button>
        <button class="ci-tool-btn" title="AI Assist" aria-label="AI Assist">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3L9.5 9.5 3 12l6.5 2.5L12 21l2.5-6.5L21 12l-6.5-2.5z"/>
          </svg>
        </button>
      </div>

      <!-- Right cluster: model pill + send pill -->
      <div class="ci-right-tools">

        <!-- Model picker pill -->
        <div class="ci-model-wrap">
          <button class="ci-model-pill" onclick={togglePicker} aria-label="Select model" title="Select AI model">
            <span class="ci-model-dot" aria-hidden="true"></span>
            <span class="ci-model-name">{selectedModel || modelName}</span>
            <svg class="ci-model-caret" width="9" height="9" viewBox="0 0 10 6" fill="none">
              <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>

          {#if showModelPicker}
            <div class="ci-dropdown" role="listbox" aria-label="Available models">
              <div class="ci-dropdown-search">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
                </svg>
                <input type="text" class="ci-search-input" placeholder="Search models..." bind:value={modelFilter} aria-label="Filter models" />
              </div>
              <div class="ci-dropdown-list">
                {#if filteredModels.length === 0}
                  <div class="ci-dropdown-empty">
                    {availableModels.length === 0 ? 'No enabled models - go to AI Studio > Models' : 'No matches'}
                  </div>
                {:else}
                  {#each filteredModels as model}
                    <button
                      class="ci-dropdown-item"
                      class:is-active={model === selectedModel}
                      onclick={() => selectModel(model)}
                      role="option"
                      aria-selected={model === selectedModel}
                    >
                      <span class="ci-dropdown-item-name">{model}</span>
                      {#if model === selectedModel}
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#BF40FF" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                          <polyline points="20 6 9 17 4 12"/>
                        </svg>
                      {/if}
                    </button>
                  {/each}
                {/if}
              </div>
            </div>
          {/if}
        </div>

        <!-- Send pill - B3 centrepiece -->
        {#if disabled && onStop}
          <button
            class="ci-send-pill ci-stop-pill is-ready"
            onclick={onStop}
            aria-label="Stop generating"
            title="Stop generating"
          >
            <svg class="ci-send-zap" width="11" height="11" viewBox="0 0 24 24" fill="currentColor">
              <rect x="5" y="5" width="14" height="14" rx="2"/>
            </svg>
            <span>Stop</span>
          </button>
        {:else}
          <button
            class="ci-send-pill"
            class:is-ready={hasContent && !disabled}
            onclick={onSend}
            disabled={!hasContent || disabled}
            aria-label="Send message"
            title="Send message"
          >
            <svg class="ci-send-zap" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>
            </svg>
            <span>Send</span>
          </button>
        {/if}

      </div>
    </div>
  </div>

  <!-- Footer: hints + token count -->
  <div class="ci-footer">
    <span class="ci-hints"><kbd>Shift+Enter</kbd> new line | Markdown supported</span>
    <span class="ci-tokens">{tokenCount} <span class="ci-tokens-label">tokens</span></span>
  </div>

</div>

<style>
  /* ======================================================
     B3 PILL BUTTON - Pixel-perfect from Pencil spec
     Design tokens from Violet Void + Raleway + Geist Mono
  ====================================================== */

  /* -- Outer wrapper -- */
  .ci-wrap {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 0 20px 14px;
    position: relative;
  }

  /* Branch banner */
  .ci-branch-banner {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    margin-bottom: 10px;
    background: rgba(0, 210, 220, 0.06);
    border: 1px solid rgba(0, 210, 220, 0.18);
    border-radius: 10px;
    color: rgba(0, 210, 220, 0.75);
    font-size: 11px;
    font-family: 'Geist Mono', monospace;
    letter-spacing: 0.03em;
    animation: bannerSlideIn 220ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes bannerSlideIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* -- Card -- */
  .ci-card {
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    background: rgba(16, 14, 36, 0.92);
    border: 1px solid rgba(88, 44, 255, 0.18);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
    /* NO overflow:hidden - dropdown must float above */
    transition:
      border-color 300ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow   300ms cubic-bezier(0.16, 1, 0.3, 1);
    position: relative;
  }
  .ci-card::before {
    content: '';
    position: absolute; top: 0; left: 12%; right: 12%; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255,255,255,0.055) 50%, transparent);
    pointer-events: none;
  }

  .is-focused .ci-card {
    border-color: rgba(124, 58, 237, 0.32);
    box-shadow:
      0 0 0 3px rgba(124, 58, 237, 0.08),
      0 12px 48px rgba(124, 58, 237, 0.18);
  }
  .is-branching .ci-card {
    border-color: rgba(0, 210, 220, 0.28);
    box-shadow:
      0 0 0 3px rgba(0, 210, 220, 0.06),
      0 12px 48px rgba(0, 210, 220, 0.12);
  }

  /* -- Textarea row -- */
  .ci-textarea-row {
    display: flex;
    align-items: center;
    min-height: 54px;
    padding: 0 18px;
  }

  .ci-textarea {
    width: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: rgba(232, 224, 255, 0.92);
    font-size: 14px;
    font-family: 'Raleway', sans-serif;
    font-weight: 400;
    line-height: 1.6;
    resize: none;
    max-height: 160px;
    caret-color: #BF40FF;
    letter-spacing: 0.01em;
  }
  .ci-textarea::placeholder {
    color: #3D3560;
    font-style: italic;
  }

  /* -- Divider -- */
  .ci-divider {
    width: 100%;
    height: 1px;
    background: rgba(255, 255, 255, 0.05);
    flex-shrink: 0;
  }

  /* -- Toolbar -- */
  .ci-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 46px;
    padding: 0 12px;
    flex-shrink: 0;
  }

  /* Left tool buttons */
  .ci-left-tools {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .ci-tool-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: rgba(100, 90, 160, 0.45);
    cursor: pointer;
    transition: color 150ms ease, background 150ms ease;
  }
  .ci-tool-btn:hover {
    color: rgba(167, 139, 250, 0.75);
    background: rgba(124, 58, 237, 0.08);
  }
  .ci-tool-btn:active { transform: scale(0.9); }
  .ci-tool-btn-disabled { opacity: 0.35; cursor: default; }
  .ci-tool-btn-disabled:hover { color: rgba(100, 90, 160, 0.45); background: transparent; }

  /* -- Pending attachment previews -- */
  .ci-attachments-row {
    display: flex;
    gap: 8px;
    padding: 12px 18px 0;
    flex-wrap: wrap;
  }
  .ci-attachment-chip {
    position: relative;
    width: 48px;
    height: 48px;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid rgba(139, 92, 246, 0.2);
    flex-shrink: 0;
  }
  .ci-attachment-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ci-attachment-remove {
    position: absolute;
    top: 2px; right: 2px;
    width: 16px; height: 16px;
    display: flex; align-items: center; justify-content: center;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    cursor: pointer;
    padding: 0;
    transition: background 150ms ease;
  }
  .ci-attachment-remove:hover { background: rgba(244, 63, 94, 0.85); }

  .ci-attachment-unsupported {
    border-color: rgba(244, 63, 94, 0.35);
  }
  .ci-attachment-unsupported .ci-attachment-thumb {
    filter: grayscale(0.9) brightness(0.6);
  }
  .ci-attachment-unsupported-badge {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }

  /* Right cluster */
  .ci-right-tools {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* -- Model pill -- */
  .ci-model-wrap { position: relative; }

  .ci-model-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px 4px 8px;
    border-radius: 99px;
    border: 1px solid rgba(191, 64, 255, 0.14);
    background: rgba(191, 64, 255, 0.06);
    cursor: pointer;
    color: rgba(180, 130, 255, 0.7);
    font-family: 'Geist Mono', monospace;
    font-size: 9px;
    letter-spacing: 0.03em;
    white-space: nowrap;
    transition: background 160ms ease, border-color 160ms ease;
  }
  .ci-model-pill:hover {
    background: rgba(191, 64, 255, 0.1);
    border-color: rgba(191, 64, 255, 0.28);
    color: rgba(200, 160, 255, 0.9);
  }

  /* Pulsing dot */
  .ci-model-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: #BF40FF;
    box-shadow: 0 0 5px rgba(191, 64, 255, 0.6);
    flex-shrink: 0;
    animation: dotGlow 2.5s ease-in-out infinite;
  }
  @keyframes dotGlow {
    0%, 100% { box-shadow: 0 0 4px rgba(191,64,255,0.5); }
    50%       { box-shadow: 0 0 9px rgba(191,64,255,1.0), 0 0 14px rgba(191,64,255,0.4); }
  }

  .ci-model-name {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ci-model-caret { color: rgba(140, 100, 200, 0.5); flex-shrink: 0; }

  /* -- B3 SEND PILL -- */
  .ci-send-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 18px 0 14px;
    height: 34px;
    border-radius: 99px;
    border: none;
    cursor: not-allowed;
    font-family: 'Raleway', sans-serif;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    white-space: nowrap;
    position: relative;
    background: rgba(80, 40, 140, 0.3);
    color: rgba(167, 139, 250, 0.3);
    transition:
      background     350ms cubic-bezier(0.16, 1, 0.3, 1),
      color          350ms ease,
      box-shadow     350ms ease,
      transform      200ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .ci-send-zap {
    flex-shrink: 0;
    transition: transform 300ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  /* READY state */
  .ci-send-pill.is-ready {
    cursor: pointer;
    background: linear-gradient(135deg, #7C3AED 0%, #BF40FF 100%);
    color: #FFFFFF;
    box-shadow:
      0 4px 16px rgba(124, 58, 237, 0.4),
      0 8px 32px rgba(191, 64, 255, 0.13);
  }

  .ci-send-pill.is-ready:hover {
    background: linear-gradient(135deg, #8B5CF6 0%, #D946EF 100%);
    box-shadow:
      0 4px 20px rgba(124, 58, 237, 0.6),
      0 10px 40px rgba(191, 64, 255, 0.22);
    transform: scale(1.04) translateY(-1px);
  }
  .ci-send-pill.is-ready:hover .ci-send-zap {
    transform: rotate(-15deg) scale(1.15);
  }
  .ci-send-pill.is-ready:active {
    transform: scale(0.95) translateY(0);
    transition-duration: 80ms;
  }

  /* Stop pill — shown in place of Send while a response is streaming */
  .ci-stop-pill.is-ready {
    cursor: pointer;
    background: linear-gradient(135deg, #7C2D3E 0%, #F43F5E 100%);
    box-shadow:
      0 4px 16px rgba(244, 63, 94, 0.35),
      0 8px 32px rgba(244, 63, 94, 0.12);
  }
  .ci-stop-pill.is-ready:hover {
    background: linear-gradient(135deg, #9F2E44 0%, #FB7185 100%);
    box-shadow:
      0 4px 20px rgba(244, 63, 94, 0.55),
      0 10px 40px rgba(244, 63, 94, 0.2);
    transform: scale(1.04) translateY(-1px);
  }
  .ci-stop-pill.is-ready:active {
    transform: scale(0.95) translateY(0);
    transition-duration: 80ms;
  }

  /* Branching mode - cyan pill */
  .is-branching .ci-send-pill.is-ready {
    background: linear-gradient(135deg, #0e5c66, #00c8d7);
    box-shadow:
      0 4px 16px rgba(0, 200, 215, 0.4),
      0 8px 32px rgba(0, 200, 215, 0.15);
  }
  .is-branching .ci-send-pill.is-ready:hover {
    background: linear-gradient(135deg, #117a86, #00f2ff);
  }

  /* -- Model dropdown -- */
  .ci-dropdown {
    position: absolute;
    bottom: calc(100% + 10px);
    right: 0;
    width: 340px;
    max-height: 360px;
    background: rgba(8, 6, 20, 0.97);
    backdrop-filter: blur(28px) saturate(160%);
    border: 1px solid rgba(191, 64, 255, 0.15);
    border-radius: 14px;
    box-shadow:
      0 0 0 1px rgba(191, 64, 255, 0.04),
      0 -4px 24px rgba(0, 0, 0, 0.4),
      0 24px 64px rgba(0, 0, 0, 0.7),
      inset 0 1px 0 rgba(255, 255, 255, 0.04);
    z-index: 100;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: dropUp 200ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes dropUp {
    from { opacity: 0; transform: translateY(10px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .ci-dropdown-search {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 14px;
    border-bottom: 1px solid rgba(191, 64, 255, 0.07);
    color: rgba(90, 80, 140, 0.55);
  }
  .ci-search-input {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    color: rgba(220, 210, 255, 0.9);
    font-size: 12px;
    font-family: 'Raleway', sans-serif;
  }
  .ci-search-input::placeholder { color: rgba(80, 70, 130, 0.5); }

  .ci-dropdown-list {
    overflow-y: auto;
    max-height: 300px;
    padding: 5px;
  }
  .ci-dropdown-list::-webkit-scrollbar { width: 3px; }
  .ci-dropdown-list::-webkit-scrollbar-track { background: transparent; }
  .ci-dropdown-list::-webkit-scrollbar-thumb {
    background: rgba(191, 64, 255, 0.2);
    border-radius: 3px;
  }

  .ci-dropdown-item {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 12px;
    border: none;
    background: transparent;
    color: rgba(160, 145, 210, 0.7);
    font-size: 11px;
    font-family: 'Geist Mono', monospace;
    border-radius: 9px;
    cursor: pointer;
    text-align: left;
    transition: background 100ms ease, color 100ms ease;
    letter-spacing: 0.02em;
  }
  .ci-dropdown-item:hover {
    background: rgba(191, 64, 255, 0.08);
    color: rgba(220, 205, 255, 0.95);
  }
  .ci-dropdown-item.is-active {
    background: rgba(191, 64, 255, 0.12);
    color: #BF40FF;
  }
  .ci-dropdown-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    margin-right: 8px;
  }
  .ci-dropdown-empty {
    padding: 24px 16px;
    text-align: center;
    color: rgba(80, 70, 130, 0.55);
    font-size: 11px;
    font-family: 'Geist Mono', monospace;
    line-height: 1.7;
  }

  /* -- Footer -- */
  .ci-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 7px 4px 0;
  }

  .ci-hints {
    font-family: 'Geist Mono', monospace;
    font-size: 9px;
    color: rgba(70, 60, 120, 0.5);
    letter-spacing: 0.04em;
    display: flex;
    align-items: center;
    gap: 1px;
  }
  .ci-hints kbd {
    display: inline-flex;
    align-items: center;
    padding: 1px 4px;
    background: rgba(124, 58, 237, 0.07);
    border: 1px solid rgba(124, 58, 237, 0.12);
    border-radius: 4px;
    font-size: 9px;
    font-family: 'Geist Mono', monospace;
    color: rgba(180, 130, 255, 0.45);
    margin: 0 2px;
  }

  .ci-tokens {
    font-family: 'Geist Mono', monospace;
    font-size: 9px;
    color: rgba(55, 50, 100, 0.55);
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .ci-tokens-label { color: rgba(55, 50, 100, 0.4); }

  /* -- Mobile -- */
  @media (max-width: 768px) {
    .ci-wrap { padding: 0 12px 12px; }
    .ci-dropdown { width: 280px; }
    .ci-model-name { max-width: 100px; }
  }
</style>
