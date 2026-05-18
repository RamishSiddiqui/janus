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

<div class="ci" class:focused class:branching={isBranching}>
  <div class="ci-glow"></div>
  <div class="ci-row">
    <div class="ci-field">
      <textarea
        bind:this={inputElement} bind:value
        class="chat-input-field"
        placeholder="Write your response..."
        aria-label="Message input" rows="1"
        onkeydown={handleKeydown} oninput={autoResize}
        onfocus={() => focused = true} onblur={() => focused = false}
      ></textarea>
    </div>
    <div class="ci-actions">
      <button class="ci-btn attach" title="Attach File" aria-label="Attach file">
        <Icon name="paperclip" size={16} color="#6b6b8a" />
      </button>
      <button class="ci-btn send" class:active={hasContent && !disabled}
        onclick={onSend} title="Send" aria-label="Send message"
        disabled={!hasContent || disabled}>
        <Icon name="send" size={16} color="#fff" />
        <div class="send-glow"></div>
      </button>
    </div>
  </div>
  <div class="ci-hints">
    <span>Shift+Enter for new line · Markdown supported</span>
    <div class="model-picker-wrap">
      <button class="ci-model-btn" onclick={togglePicker} title="Click to select model" aria-label="Select model">
        <Icon name="cpu" size={10} color="#5a5a7a" />
        <span class="ci-model-text">{selectedModel || modelName}</span>
        <Icon name="chevron-down" size={10} color="#5a5a7a" />
      </button>
      <span class="ci-token-count">· {tokenCount} tokens</span>

      {#if showModelPicker}
        <div class="model-dropdown" role="listbox" aria-label="Available models">
          <div class="model-search-wrap">
            <Icon name="search" size={12} color="#6b6b8a" />
            <input
              type="text"
              class="model-search"
              placeholder="Filter models..."
              bind:value={modelFilter}
              aria-label="Filter models"
            />
          </div>
          <div class="model-list">
            {#if filteredModels.length === 0}
              <div class="model-empty">
              {availableModels.length === 0 ? 'No enabled models — go to AI Studio → Models' : 'No matches'}
              </div>
            {:else}
              {#each filteredModels as model}
                <button
                  class="model-option"
                  class:selected={model === selectedModel}
                  onclick={() => selectModel(model)}
                  role="option"
                  aria-selected={model === selectedModel}
                >
                  <span class="model-option-name">{model}</span>
                  {#if model === selectedModel}
                    <Icon name="check" size={12} color="#c4a1ff" />
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
  /* ───────────────────────────────────────────────
     COMMAND BAR WRAPPER
  ─────────────────────────────────────────────── */
  .ci {
    padding: 16px 20px 14px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: linear-gradient(0deg, rgba(7,7,20,0.99) 0%, rgba(10,10,26,0.95) 100%);
    position: relative;
  }
  /* Ambient top glow line */
  .ci-glow {
    position: absolute; top: 0; left: 0; right: 0; height: 1px;
    background: linear-gradient(90deg,
      transparent 0%,
      rgba(139,92,246,0.0) 20%,
      rgba(139,92,246,0.35) 50%,
      rgba(139,92,246,0.0) 80%,
      transparent 100%
    );
    opacity: 0;
    transition: opacity 400ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .ci.focused .ci-glow { opacity: 1; }
  .ci.branching .ci-glow {
    background: linear-gradient(90deg,
      transparent 0%,
      rgba(0,242,255,0.0) 20%,
      rgba(0,242,255,0.4) 50%,
      rgba(0,242,255,0.0) 80%,
      transparent 100%
    );
    opacity: 1;
  }

  /* ───────────────────────────────────────────────
     MAIN INPUT PANEL
  ─────────────────────────────────────────────── */
  .ci-row {
    display: flex;
    gap: 10px;
    align-items: flex-end;
    padding: 4px 4px 4px 18px;
    background: rgba(12,12,30,0.8);
    border: 1px solid rgba(255,255,255,0.05);
    border-radius: 18px;
    backdrop-filter: blur(20px);
    box-shadow:
      0 1px 0 rgba(255,255,255,0.04) inset,
      0 -1px 0 rgba(0,0,0,0.3) inset,
      0 8px 32px rgba(0,0,0,0.4),
      0 2px 8px rgba(0,0,0,0.3);
    transition:
      border-color 300ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 300ms cubic-bezier(0.16, 1, 0.3, 1);
    position: relative;
    overflow: hidden;
  }
  /* Focus ring — animated border glow */
  .ci.focused .ci-row {
    border-color: rgba(139,92,246,0.22);
    box-shadow:
      0 1px 0 rgba(255,255,255,0.04) inset,
      0 -1px 0 rgba(0,0,0,0.3) inset,
      0 0 0 3px rgba(139,92,246,0.06),
      0 8px 40px rgba(139,92,246,0.1),
      0 2px 8px rgba(0,0,0,0.3);
  }
  .ci.branching .ci-row {
    border-color: rgba(0,242,255,0.2);
    box-shadow:
      0 1px 0 rgba(255,255,255,0.04) inset,
      0 -1px 0 rgba(0,0,0,0.3) inset,
      0 0 0 3px rgba(0,242,255,0.05),
      0 8px 40px rgba(0,242,255,0.08),
      0 2px 8px rgba(0,0,0,0.3);
  }
  /* Inner top sheen */
  .ci-row::before {
    content: '';
    position: absolute; top: 0; left: 0; right: 0; height: 1px;
    background: linear-gradient(90deg, transparent 10%, rgba(255,255,255,0.06) 50%, transparent 90%);
    pointer-events: none;
  }

  /* ───────────────────────────────────────────────
     TEXTAREA
  ─────────────────────────────────────────────── */
  .ci-field { flex: 1; padding: 12px 0; }

  .chat-input-field {
    width: 100%; background: none; border: none; outline: none;
    color: rgba(230, 225, 255, 0.94);
    font-size: 14.5px;
    font-family: var(--font-body);
    line-height: 1.65;
    resize: none;
    max-height: 160px;
    letter-spacing: 0.01em;
  }
  .chat-input-field::placeholder {
    color: rgba(90, 85, 130, 0.6);
    font-style: italic;
  }

  /* ───────────────────────────────────────────────
     ACTIONS (attach + send)
  ─────────────────────────────────────────────── */
  .ci-actions {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 6px 6px 0;
    flex-shrink: 0;
  }

  /* Attach button */
  .ci-btn {
    display: flex; align-items: center; justify-content: center;
    width: 36px; height: 36px; border-radius: 10px;
    border: none; background: transparent; cursor: pointer;
    transition: background 150ms ease, transform 100ms ease;
    flex-shrink: 0;
  }
  .ci-btn.attach { color: rgba(107,107,165,0.6); }
  .ci-btn.attach:hover {
    background: rgba(139,92,246,0.08);
    color: rgba(139,92,246,0.8);
    transform: scale(1.08);
  }

  /* Send orb */
  .ci-btn.send {
    width: 40px; height: 40px; border-radius: 12px;
    background: linear-gradient(145deg, #6d28d9, #7c3aed);
    border: 1px solid rgba(139,92,246,0.4);
    opacity: 0.3;
    position: relative; overflow: hidden;
    transition: opacity 250ms ease, transform 200ms cubic-bezier(0.34,1.56,0.64,1),
                box-shadow 250ms ease, border-color 250ms ease;
    cursor: not-allowed;
  }
  .ci-btn.send.active {
    opacity: 1;
    cursor: pointer;
    border-color: rgba(139,92,246,0.6);
    box-shadow:
      0 0 0 1px rgba(139,92,246,0.15),
      0 4px 20px rgba(109,40,217,0.4),
      0 8px 32px rgba(109,40,217,0.2),
      inset 0 1px 0 rgba(255,255,255,0.16);
  }
  .ci-btn.send.active:hover {
    transform: translateY(-2px) scale(1.06);
    box-shadow:
      0 0 0 1px rgba(139,92,246,0.25),
      0 8px 28px rgba(109,40,217,0.55),
      0 14px 40px rgba(109,40,217,0.28),
      inset 0 1px 0 rgba(255,255,255,0.2);
  }
  .ci-btn.send.active:active { transform: scale(0.94); }
  .ci-btn.send:disabled { cursor: not-allowed; }

  /* Send specular highlight */
  .send-glow {
    position: absolute; inset: 0;
    background: radial-gradient(ellipse 70% 50% at 50% 0%, rgba(255,255,255,0.18), transparent 60%);
    pointer-events: none;
  }

  /* Branching mode — send becomes cyan */
  .ci.branching .ci-btn.send.active {
    background: linear-gradient(145deg, #0e5c66, #0a7f8f);
    border-color: rgba(0,242,255,0.4);
    box-shadow:
      0 0 0 1px rgba(0,242,255,0.15),
      0 4px 20px rgba(0,200,220,0.35),
      inset 0 1px 0 rgba(255,255,255,0.14);
  }

  /* ───────────────────────────────────────────────
     FOOTER BAR — hints + model picker
  ─────────────────────────────────────────────── */
  .ci-hints {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 6px;
    font-size: 10.5px;
    color: rgba(75,70,120,0.7);
    font-family: var(--font-mono);
    letter-spacing: 0.02em;
  }

  /* ───────────────────────────────────────────────
     MODEL PICKER
  ─────────────────────────────────────────────── */
  .model-picker-wrap {
    position: relative; display: flex; align-items: center; gap: 6px;
  }

  .ci-model-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 10px 3px 7px;
    background: rgba(139,92,246,0.05);
    border: 1px solid rgba(139,92,246,0.1);
    border-radius: 99px;
    cursor: pointer;
    transition: all 200ms ease;
    color: rgba(90,85,130,0.8);
    font-family: var(--font-mono);
    font-size: 10px;
    white-space: nowrap;
    letter-spacing: 0.03em;
  }
  .ci-model-btn:hover {
    background: rgba(139,92,246,0.1);
    border-color: rgba(139,92,246,0.22);
    color: rgba(180,160,255,0.9);
    box-shadow: 0 0 0 1px rgba(139,92,246,0.05);
  }
  .ci-model-text {
    max-width: 200px; overflow: hidden; text-overflow: ellipsis;
  }
  .ci-token-count {
    color: rgba(60,55,100,0.6);
    white-space: nowrap;
    font-size: 10px;
    letter-spacing: 0.02em;
  }

  /* ───────────────────────────────────────────────
     MODEL DROPDOWN
  ─────────────────────────────────────────────── */
  .model-dropdown {
    position: absolute; bottom: calc(100% + 10px); right: 0;
    width: 340px; max-height: 360px;
    background: rgba(10,10,26,0.97);
    backdrop-filter: blur(24px);
    border: 1px solid rgba(139,92,246,0.14);
    border-radius: 16px;
    box-shadow:
      0 -4px 24px rgba(0,0,0,0.5),
      0 0 0 1px rgba(139,92,246,0.04),
      0 24px 64px rgba(0,0,0,0.6);
    z-index: 100;
    display: flex; flex-direction: column;
    overflow: hidden;
    animation: dropUp 180ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes dropUp {
    from { opacity: 0; transform: translateY(10px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .model-search-wrap {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid rgba(139,92,246,0.07);
  }
  .model-search {
    flex: 1; background: none; border: none; outline: none;
    color: rgba(220,215,248,0.9);
    font-size: 12px; font-family: var(--font-body);
    letter-spacing: 0.01em;
  }
  .model-search::placeholder { color: rgba(90,85,130,0.5); }

  .model-list {
    overflow-y: auto; max-height: 290px; padding: 6px;
  }
  .model-list::-webkit-scrollbar { width: 3px; }
  .model-list::-webkit-scrollbar-thumb {
    background: rgba(139,92,246,0.2);
    border-radius: 3px;
  }

  .model-option {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 9px 12px; border: none; background: transparent;
    color: rgba(170,160,210,0.75);
    font-size: 11px; font-family: var(--font-mono);
    border-radius: 10px; cursor: pointer; text-align: left;
    transition: background 120ms ease, color 120ms ease;
    letter-spacing: 0.02em;
  }
  .model-option:hover {
    background: rgba(139,92,246,0.08);
    color: rgba(220,210,255,0.95);
  }
  .model-option.selected {
    background: rgba(139,92,246,0.12);
    color: #c4a1ff;
  }
  .model-option-name {
    overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; flex: 1; margin-right: 8px;
  }

  .model-empty {
    padding: 24px; text-align: center;
    color: rgba(90,85,130,0.6);
    font-size: 11px; font-family: var(--font-mono);
    line-height: 1.6;
  }

  /* ───────────────────────────────────────────────
     RESPONSIVE
  ─────────────────────────────────────────────── */
  @media (max-width: 768px) {
    .ci { padding: 12px 14px 10px; }
    .ci-row { padding: 4px 4px 4px 14px; border-radius: 14px; }
    .model-dropdown { width: 280px; }
  }
</style>