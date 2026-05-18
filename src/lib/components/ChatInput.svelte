<script lang="ts">
  import Icon from './Icon.svelte';
  import { browser } from '$app/environment';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    value = $bindable(''), modelName, tokenCount, onSend, disabled = false,
    selectedModel = $bindable(''), availableModels = [],
    onRefreshModels, isBranching = false,
  }: {
    value: string; modelName: string; tokenCount: string;
    onSend: () => void; disabled?: boolean;
    selectedModel?: string; availableModels?: string[];
    onRefreshModels?: () => void;
    isBranching?: boolean;
  } = $props();

  let inputElement: HTMLTextAreaElement | undefined = $state();
  let focused = $state(false);
  let showModelPicker = $state(false);
  let modelFilter = $state('');

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
    if (showModelPicker && !target.closest('.model-picker-wrap')) {
      showModelPicker = false;
      modelFilter = '';
    }
  }
</script>


<svelte:window onclick={handleWindowClick} />

<div class="composer" class:focused class:branching={isBranching} class:has-content={hasContent}>
  <!-- Ambient backdrop glow -->
  <div class="composer-ambient" aria-hidden="true"></div>

  <!-- Branch indicator strip -->
  {#if isBranching}
    <div class="branch-banner" role="status" aria-label="Branching mode active">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>
        <path d="M18 9a9 9 0 0 1-9 9"/>
      </svg>
      <span>Branching â€” your reply creates a new timeline</span>
    </div>
  {/if}

  <!-- Main composer panel -->
  <div class="composer-panel">
    <!-- Textarea -->
    <div class="composer-field">
      <textarea
        bind:this={inputElement}
        bind:value
        class="composer-textarea"
        placeholder="Write your responseâ€¦"
        aria-label="Message input"
        rows="1"
        onkeydown={handleKeydown}
        oninput={autoResize}
        onfocus={() => focused = true}
        onblur={() => focused = false}
      ></textarea>
    </div>

    <!-- Right action cluster -->
    <div class="composer-actions">
      <!-- Attach -->
      <button class="action-icon" title="Attach File" aria-label="Attach file">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
        </svg>
      </button>

      <!-- Send orb -->
      <button
        class="send-orb"
        class:ready={hasContent && !disabled}
        onclick={onSend}
        title="Send message"
        aria-label="Send message"
        disabled={!hasContent || disabled}
      >
        <span class="send-orb-ring" aria-hidden="true"></span>
        <span class="send-orb-ring send-orb-ring--2" aria-hidden="true"></span>
        <span class="send-orb-core" aria-hidden="true">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="22" y1="2" x2="11" y2="13"/>
            <polygon points="22 2 15 22 11 13 2 9 22 2"/>
          </svg>
        </span>
      </button>
    </div>
  </div>

  <!-- Footer bar -->
  <div class="composer-footer">
    <span class="footer-hint">
      <kbd>Shift+Enter</kbd> new line Â· <kbd>Enter</kbd> send Â· Markdown supported
    </span>
    <div class="model-picker-wrap">
      <button class="model-pill" onclick={togglePicker} title="Click to select model" aria-label="Select model">
        <span class="model-pill-dot" aria-hidden="true"></span>
        <span class="model-pill-text">{selectedModel || modelName}</span>
        <svg class="model-pill-caret" width="9" height="9" viewBox="0 0 10 6" fill="none">
          <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>
      <span class="token-badge">{tokenCount} <span class="token-label">tokens</span></span>

      {#if showModelPicker}
        <div class="model-dropdown" role="listbox" aria-label="Available models">
          <div class="model-search-row">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <input type="text" class="model-search" placeholder="Search modelsâ€¦" bind:value={modelFilter} aria-label="Filter models" />
          </div>
          <div class="model-list">
            {#if filteredModels.length === 0}
              <div class="model-empty">
                {availableModels.length === 0 ? 'No enabled models â€” go to AI Studio â†’ Models' : 'No matches for this query'}
              </div>
            {:else}
              {#each filteredModels as model}
                <button
                  class="model-item"
                  class:model-item--active={model === selectedModel}
                  onclick={() => selectModel(model)}
                  role="option"
                  aria-selected={model === selectedModel}
                >
                  <span class="model-item-name">{model}</span>
                  {#if model === selectedModel}
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#c4a1ff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
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
  </div>
</div>
<style>
  /* ---------------------------------------------------
     NEURAL COMPOSER — Awwwards-tier chat input
     Design System: Dark OLED + Glassmorphism hybrid
     Colors: #7C3AED primary, #0F0F23 bg, #A78BFA secondary
  --------------------------------------------------- */

  /* -- Outer wrapper -- */
  .composer {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 0 20px 16px;
    position: relative;
    background: transparent;
  }

  /* -- Ambient background glow (rises from bottom) -- */
  .composer-ambient {
    position: absolute;
    bottom: 0; left: 0; right: 0;
    height: 160px;
    background: radial-gradient(ellipse 80% 60% at 50% 100%,
      rgba(124, 58, 237, 0.07) 0%,
      rgba(124, 58, 237, 0.03) 50%,
      transparent 100%
    );
    pointer-events: none;
    transition: opacity 400ms ease;
  }
  .composer.focused .composer-ambient {
    background: radial-gradient(ellipse 80% 60% at 50% 100%,
      rgba(124, 58, 237, 0.13) 0%,
      rgba(124, 58, 237, 0.05) 50%,
      transparent 100%
    );
  }
  .composer.branching .composer-ambient {
    background: radial-gradient(ellipse 80% 60% at 50% 100%,
      rgba(0, 210, 220, 0.1) 0%,
      rgba(0, 210, 220, 0.04) 50%,
      transparent 100%
    );
  }

  /* -- Branch banner -- */
  .branch-banner {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 14px;
    margin-bottom: 10px;
    background: rgba(0, 210, 220, 0.06);
    border: 1px solid rgba(0, 210, 220, 0.18);
    border-radius: 10px;
    color: rgba(0, 210, 220, 0.75);
    font-size: 11px; font-family: var(--font-mono);
    letter-spacing: 0.04em;
    animation: bannerIn 220ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes bannerIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* -- Main panel (floating glass card) -- */
  .composer-panel {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    padding: 6px 6px 6px 20px;
    background: rgba(13, 13, 32, 0.85);
    border: 1px solid rgba(255, 255, 255, 0.055);
    border-radius: 20px;
    backdrop-filter: blur(24px) saturate(160%);
    box-shadow:
      /* top inner sheen */
      inset 0 1px 0 rgba(255, 255, 255, 0.06),
      /* bottom inner shadow */
      inset 0 -1px 0 rgba(0, 0, 0, 0.35),
      /* ambient float */
      0 8px 40px rgba(0, 0, 0, 0.5),
      /* subtle depth */
      0 2px 10px rgba(0, 0, 0, 0.3);
    transition:
      border-color 350ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 350ms cubic-bezier(0.16, 1, 0.3, 1);
    position: relative;
    overflow: hidden;
  }

  /* Top edge shimmer line */
  .composer-panel::before {
    content: '';
    position: absolute; top: 0; left: 10%; right: 10%; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255,255,255,0.07) 50%, transparent);
    pointer-events: none;
  }

  /* Focus state — violet glow border */
  .composer.focused .composer-panel {
    border-color: rgba(124, 58, 237, 0.28);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      inset 0 -1px 0 rgba(0, 0, 0, 0.35),
      0 0 0 4px rgba(124, 58, 237, 0.07),
      0 8px 48px rgba(124, 58, 237, 0.14),
      0 2px 10px rgba(0, 0, 0, 0.3);
  }

  /* Branch state — cyan glow border */
  .composer.branching .composer-panel {
    border-color: rgba(0, 210, 220, 0.25);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      inset 0 -1px 0 rgba(0, 0, 0, 0.35),
      0 0 0 4px rgba(0, 210, 220, 0.06),
      0 8px 48px rgba(0, 210, 220, 0.1),
      0 2px 10px rgba(0, 0, 0, 0.3);
  }

  /* -- Textarea area -- */
  .composer-field { flex: 1; padding: 13px 0; }

  .composer-textarea {
    width: 100%;
    background: none; border: none; outline: none;
    color: rgba(232, 226, 255, 0.94);
    font-size: 15px;
    font-family: var(--font-body);
    line-height: 1.65;
    resize: none;
    max-height: 180px;
    letter-spacing: 0.012em;
    caret-color: #a78bfa;
  }
  .composer-textarea::placeholder {
    color: rgba(94, 88, 140, 0.55);
    font-style: italic;
    font-weight: 400;
  }

  /* -- Right action cluster -- */
  .composer-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 8px 8px 4px;
    flex-shrink: 0;
  }

  /* Attach icon button */
  .action-icon {
    display: flex; align-items: center; justify-content: center;
    width: 38px; height: 38px; border-radius: 11px;
    border: none; background: transparent;
    color: rgba(100, 95, 150, 0.5);
    cursor: pointer;
    transition: color 180ms ease, background 180ms ease, transform 120ms ease;
  }
  .action-icon:hover {
    color: rgba(167, 139, 250, 0.85);
    background: rgba(124, 58, 237, 0.1);
    transform: scale(1.08);
  }
  .action-icon:active { transform: scale(0.92); }

  /* ------------------------------------------------
     SEND ORB — the centrepiece of the design
  ------------------------------------------------ */
  .send-orb {
    position: relative;
    width: 44px; height: 44px;
    border-radius: 14px;
    border: none;
    background: none;
    padding: 0;
    cursor: not-allowed;
    flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
  }

  /* Outer animated rings (hidden when not ready) */
  .send-orb-ring {
    position: absolute; inset: -4px;
    border-radius: 18px;
    border: 1.5px solid rgba(124, 58, 237, 0.0);
    transition: border-color 400ms ease, inset 400ms ease;
    pointer-events: none;
  }
  .send-orb-ring--2 {
    inset: -9px;
    border-radius: 23px;
    animation-delay: 200ms;
  }

  /* Core button body */
  .send-orb-core {
    position: relative; z-index: 2;
    width: 44px; height: 44px;
    border-radius: 14px;
    display: flex; align-items: center; justify-content: center;
    background: linear-gradient(145deg, rgba(60, 30, 120, 0.6), rgba(80, 40, 160, 0.4));
    border: 1px solid rgba(124, 58, 237, 0.15);
    color: rgba(167, 139, 250, 0.3);
    transition:
      background 350ms cubic-bezier(0.16, 1, 0.3, 1),
      border-color 350ms ease,
      color 350ms ease,
      box-shadow 350ms ease,
      transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  /* Top specular highlight on core */
  .send-orb-core::before {
    content: '';
    position: absolute; inset: 0;
    border-radius: inherit;
    background: radial-gradient(ellipse 80% 50% at 50% 0%, rgba(255,255,255,0.12), transparent 60%);
    pointer-events: none;
    opacity: 0;
    transition: opacity 350ms ease;
  }

  /* READY state — full activation */
  .send-orb.ready {
    cursor: pointer;
  }
  .send-orb.ready .send-orb-ring {
    border-color: rgba(124, 58, 237, 0.2);
    animation: orbRingPulse 2.8s ease-in-out infinite;
  }
  .send-orb.ready .send-orb-ring--2 {
    border-color: rgba(124, 58, 237, 0.1);
    animation: orbRingPulse 2.8s ease-in-out infinite 0.7s;
  }
  .send-orb.ready .send-orb-core {
    background: linear-gradient(145deg, #5b21b6 0%, #7c3aed 50%, #8b5cf6 100%);
    border-color: rgba(139, 92, 246, 0.5);
    color: rgba(255, 255, 255, 0.95);
    box-shadow:
      0 0 0 1px rgba(139, 92, 246, 0.2),
      0 4px 20px rgba(109, 40, 217, 0.45),
      0 10px 40px rgba(109, 40, 217, 0.22),
      inset 0 1px 0 rgba(255, 255, 255, 0.18);
  }
  .send-orb.ready .send-orb-core::before { opacity: 1; }

  .send-orb.ready:hover .send-orb-core {
    background: linear-gradient(145deg, #6d28d9 0%, #8b5cf6 50%, #a78bfa 100%);
    box-shadow:
      0 0 0 1px rgba(139, 92, 246, 0.35),
      0 6px 28px rgba(109, 40, 217, 0.6),
      0 14px 48px rgba(109, 40, 217, 0.3),
      inset 0 1px 0 rgba(255, 255, 255, 0.22);
    transform: scale(1.07) translateY(-1px);
  }
  .send-orb.ready:active .send-orb-core {
    transform: scale(0.93) translateY(0);
    transition-duration: 80ms;
  }

  /* Branching mode — cyan orb */
  .composer.branching .send-orb.ready .send-orb-ring {
    border-color: rgba(0, 210, 220, 0.2);
    animation: orbRingPulseCyan 2.8s ease-in-out infinite;
  }
  .composer.branching .send-orb.ready .send-orb-ring--2 {
    border-color: rgba(0, 210, 220, 0.1);
  }
  .composer.branching .send-orb.ready .send-orb-core {
    background: linear-gradient(145deg, #0e5c66, #0d8f9e);
    border-color: rgba(0, 210, 220, 0.45);
    box-shadow:
      0 0 0 1px rgba(0, 210, 220, 0.2),
      0 4px 24px rgba(0, 180, 200, 0.4),
      0 10px 40px rgba(0, 180, 200, 0.2),
      inset 0 1px 0 rgba(255, 255, 255, 0.15);
  }

  @keyframes orbRingPulse {
    0%, 100% { opacity: 0.4; transform: scale(1); }
    50%       { opacity: 1;   transform: scale(1.04); }
  }
  @keyframes orbRingPulseCyan {
    0%, 100% { opacity: 0.35; transform: scale(1); }
    50%       { opacity: 0.9;  transform: scale(1.05); }
  }

  /* -- Footer bar -- */
  .composer-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 4px 0;
  }

  .footer-hint {
    font-size: 10px;
    color: rgba(70, 65, 115, 0.65);
    font-family: var(--font-mono);
    letter-spacing: 0.035em;
    display: flex; gap: 1px; align-items: center;
  }
  .footer-hint kbd {
    display: inline-flex; align-items: center;
    padding: 1px 5px;
    background: rgba(124, 58, 237, 0.08);
    border: 1px solid rgba(124, 58, 237, 0.12);
    border-radius: 5px;
    font-size: 9.5px;
    font-family: var(--font-mono);
    color: rgba(167, 139, 250, 0.5);
    margin: 0 2px;
  }

  /* -- Model picker wrap -- */
  .model-picker-wrap {
    position: relative; display: flex; align-items: center; gap: 7px;
  }

  /* Model pill */
  .model-pill {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 9px 3px 6px;
    background: rgba(124, 58, 237, 0.05);
    border: 1px solid rgba(124, 58, 237, 0.12);
    border-radius: 99px;
    cursor: pointer;
    transition: background 180ms ease, border-color 180ms ease, box-shadow 180ms ease;
    color: rgba(90, 85, 140, 0.8);
    font-family: var(--font-mono);
    font-size: 10px; letter-spacing: 0.03em;
    white-space: nowrap;
  }
  .model-pill:hover {
    background: rgba(124, 58, 237, 0.1);
    border-color: rgba(167, 139, 250, 0.25);
    color: rgba(180, 160, 255, 0.9);
    box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.06);
  }
  .model-pill-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: radial-gradient(circle, #7c3aed, #5b21b6);
    box-shadow: 0 0 4px rgba(124, 58, 237, 0.6);
    flex-shrink: 0;
    animation: dotPulse 3s ease-in-out infinite;
  }
  @keyframes dotPulse {
    0%, 100% { box-shadow: 0 0 4px rgba(124, 58, 237, 0.5); }
    50%       { box-shadow: 0 0 8px rgba(124, 58, 237, 0.9), 0 0 12px rgba(124, 58, 237, 0.3); }
  }
  .model-pill-text {
    max-width: 180px; overflow: hidden; text-overflow: ellipsis;
  }
  .model-pill-caret {
    color: rgba(90, 85, 140, 0.5); flex-shrink: 0;
    transition: transform 200ms ease;
  }
  /* no way to conditionally rotate in Svelte without JS, skip for now */

  .token-badge {
    font-family: var(--font-mono);
    font-size: 10px; letter-spacing: 0.04em;
    color: rgba(55, 50, 100, 0.65);
    white-space: nowrap;
  }
  .token-label { color: rgba(55, 50, 100, 0.45); }

  /* -- Model dropdown -- */
  .model-dropdown {
    position: absolute; bottom: calc(100% + 12px); right: 0;
    width: 360px; max-height: 380px;
    background: rgba(8, 8, 22, 0.97);
    backdrop-filter: blur(28px) saturate(160%);
    border: 1px solid rgba(124, 58, 237, 0.16);
    border-radius: 18px;
    box-shadow:
      0 0 0 1px rgba(124, 58, 237, 0.05),
      0 -8px 32px rgba(0, 0, 0, 0.5),
      0 32px 80px rgba(0, 0, 0, 0.7),
      inset 0 1px 0 rgba(255, 255, 255, 0.04);
    z-index: 100;
    display: flex; flex-direction: column;
    overflow: hidden;
    animation: dropUp 200ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes dropUp {
    from { opacity: 0; transform: translateY(12px) scale(0.95); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .model-search-row {
    display: flex; align-items: center; gap: 10px;
    padding: 13px 16px;
    border-bottom: 1px solid rgba(124, 58, 237, 0.08);
    color: rgba(90, 85, 140, 0.6);
  }
  .model-search {
    flex: 1; background: none; border: none; outline: none;
    color: rgba(220, 215, 252, 0.92);
    font-size: 12.5px; font-family: var(--font-body);
    letter-spacing: 0.01em;
  }
  .model-search::placeholder { color: rgba(80, 75, 130, 0.5); }

  .model-list {
    overflow-y: auto; max-height: 310px; padding: 6px;
  }
  .model-list::-webkit-scrollbar { width: 3px; }
  .model-list::-webkit-scrollbar-track { background: transparent; }
  .model-list::-webkit-scrollbar-thumb {
    background: rgba(124, 58, 237, 0.25);
    border-radius: 3px;
  }

  .model-item {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border: none; background: transparent;
    color: rgba(160, 150, 210, 0.72);
    font-size: 11.5px; font-family: var(--font-mono);
    border-radius: 11px; cursor: pointer; text-align: left;
    transition: background 100ms ease, color 100ms ease;
    letter-spacing: 0.025em;
  }
  .model-item:hover {
    background: rgba(124, 58, 237, 0.09);
    color: rgba(220, 210, 255, 0.95);
  }
  .model-item--active {
    background: rgba(124, 58, 237, 0.14);
    color: #c4a1ff;
  }
  .model-item-name {
    overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; flex: 1; margin-right: 10px;
  }

  .model-empty {
    padding: 28px 20px; text-align: center;
    color: rgba(80, 75, 130, 0.6);
    font-size: 11px; font-family: var(--font-mono);
    line-height: 1.7;
  }

  /* -- Responsive -- */
  @media (max-width: 768px) {
    .composer { padding: 0 12px 14px; }
    .composer-panel { padding: 4px 4px 4px 14px; border-radius: 16px; }
    .model-dropdown { width: 290px; }
  }
</style>