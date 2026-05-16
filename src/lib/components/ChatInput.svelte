<script lang="ts">
  import Icon from './Icon.svelte';
  import { browser } from '$app/environment';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    value = $bindable(''), modelName, tokenCount, onSend, disabled = false,
    selectedModel = $bindable(''), availableModels = [],
    onRefreshModels,
  }: {
    value: string; modelName: string; tokenCount: string;
    onSend: () => void; disabled?: boolean;
    selectedModel?: string; availableModels?: string[];
    onRefreshModels?: () => void;
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

<div class="ci" class:focused>
  <div class="ci-glow"></div>
  <div class="ci-row">
    <div class="ci-field">
      <textarea
        bind:this={inputElement} bind:value
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
                {availableModels.length === 0 ? 'Loading models...' : 'No matches'}
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
  .ci {
    padding: 14px 24px 12px; flex-shrink: 0;
    display: flex; flex-direction: column; gap: 8px;
    background: linear-gradient(0deg, rgba(9,9,26,0.98), rgba(12,12,30,0.9));
    border-top: 1px solid rgba(139,92,246,0.06);
    position: relative;
  }
  .ci-glow {
    position: absolute; top: 0; left: 0; right: 0; height: 1px;
    background: linear-gradient(90deg, transparent 10%, rgba(139,92,246,0.2) 50%, transparent 90%);
    opacity: 0; transition: opacity 300ms;
  }
  .ci.focused .ci-glow { opacity: 1; }

  .ci-row { display: flex; gap: 10px; align-items: flex-end; }

  .ci-field {
    flex: 1; border-radius: 14px; padding: 12px 16px;
    background: rgba(14,14,30,0.7);
    border: 1px solid rgba(139,92,246,0.08);
    transition: all 250ms var(--ease-out);
  }
  .ci.focused .ci-field {
    border-color: rgba(139,92,246,0.3);
    box-shadow: 0 0 0 4px rgba(139,92,246,0.05), 0 4px 24px rgba(139,92,246,0.06);
    background: rgba(18,18,36,0.9);
  }

  .ci-field textarea {
    width: 100%; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: var(--text-base); font-family: var(--font-body);
    line-height: 1.6; resize: none; max-height: 160px;
  }
  .ci-field textarea::placeholder { color: #4a4a6a; }

  .ci-actions { display: flex; gap: 8px; }

  .ci-btn {
    width: 42px; height: 42px; border-radius: 12px;
    border: 1px solid rgba(139,92,246,0.08); background: transparent;
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; cursor: pointer;
    transition: all 180ms var(--ease-out);
  }
  .ci-btn.attach:hover {
    background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.15);
  }

  .ci-btn.send {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    border: none; opacity: 0.35; position: relative; overflow: hidden;
    transition: opacity 200ms, transform 200ms var(--ease-spring), box-shadow 200ms;
  }
  .ci-btn.send.active { opacity: 1; box-shadow: 0 4px 20px rgba(139,92,246,0.3); }
  .ci-btn.send.active:hover { transform: translateY(-1px) scale(1.05); box-shadow: 0 6px 28px rgba(139,92,246,0.45); }
  .ci-btn.send:active { transform: scale(0.92); }
  .ci-btn.send:disabled { cursor: not-allowed; }

  .send-glow {
    position: absolute; inset: 0;
    background: linear-gradient(135deg, transparent 40%, rgba(255,255,255,0.12));
    pointer-events: none;
  }

  .ci-hints {
    display: flex; justify-content: space-between; align-items: center;
    font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono);
    padding: 0 4px;
  }

  /* Model Picker */
  .model-picker-wrap {
    position: relative; display: flex; align-items: center; gap: 6px;
  }

  .ci-model-btn {
    display: flex; align-items: center; gap: 4px;
    background: rgba(139,92,246,0.06); border: 1px solid rgba(139,92,246,0.1);
    border-radius: 6px; padding: 2px 8px; cursor: pointer;
    transition: all 150ms; color: #5a5a7a; font-family: var(--font-mono);
    font-size: 10px; white-space: nowrap;
  }
  .ci-model-btn:hover {
    background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.2);
    color: #8b8ba7;
  }

  .ci-model-text {
    max-width: 180px; overflow: hidden; text-overflow: ellipsis;
  }

  .ci-token-count { color: #4a4a6a; white-space: nowrap; }

  .model-dropdown {
    position: absolute; bottom: calc(100% + 8px); right: 0;
    width: 320px; max-height: 340px;
    background: rgba(14,14,30,0.97); backdrop-filter: blur(16px);
    border: 1px solid rgba(139,92,246,0.15); border-radius: 12px;
    box-shadow: 0 8px 40px rgba(0,0,0,0.5), 0 0 0 1px rgba(139,92,246,0.05);
    z-index: 100; display: flex; flex-direction: column;
    animation: dropUp 150ms var(--ease-out);
  }

  @keyframes dropUp {
    from { opacity: 0; transform: translateY(8px) scale(0.97); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .model-search-wrap {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid rgba(139,92,246,0.08);
  }

  .model-search {
    flex: 1; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: 12px; font-family: var(--font-body);
  }
  .model-search::placeholder { color: #4a4a6a; }

  .model-list {
    overflow-y: auto; max-height: 280px; padding: 4px;
  }
  .model-list::-webkit-scrollbar { width: 4px; }
  .model-list::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  .model-option {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 8px 10px; border: none; background: transparent;
    color: #b0b0cc; font-size: 11px; font-family: var(--font-mono);
    border-radius: 8px; cursor: pointer; text-align: left;
    transition: all 120ms;
  }
  .model-option:hover { background: rgba(139,92,246,0.08); color: #e0e0f0; }
  .model-option.selected { background: rgba(139,92,246,0.1); color: #c4a1ff; }

  .model-option-name {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    flex: 1; margin-right: 8px;
  }

  .model-empty {
    padding: 20px; text-align: center; color: #4a4a6a;
    font-size: 11px; font-family: var(--font-mono);
  }

  @media (max-width: 768px) {
    .ci { padding: 10px 14px; }
    .ci-field { padding: 10px 14px; }
    .model-dropdown { width: 260px; }
  }
</style>
