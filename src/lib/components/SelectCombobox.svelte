<script lang="ts">
  export interface ComboOption {
    value: string;
    label: string;
    sublabel?: string;
  }

  let {
    value = $bindable(''),
    options = [],
    placeholder = 'Select…',
    disabled = false,
    ariaLabel = 'Select an option',
    emptyText = 'No options',
    onChange,
  }: {
    value?: string;
    options?: ComboOption[];
    placeholder?: string;
    disabled?: boolean;
    ariaLabel?: string;
    emptyText?: string;
    onChange?: (value: string) => void;
  } = $props();

  let isOpen = $state(false);
  let filter = $state('');
  let rootEl: HTMLDivElement | undefined = $state();

  let selected = $derived(options.find(o => o.value === value));
  let filtered = $derived(
    filter.trim()
      ? options.filter(o =>
          o.label.toLowerCase().includes(filter.toLowerCase()) ||
          (o.sublabel ?? '').toLowerCase().includes(filter.toLowerCase()))
      : options
  );

  function toggle() {
    if (disabled) return;
    isOpen = !isOpen;
    if (!isOpen) filter = '';
  }

  function choose(opt: ComboOption) {
    value = opt.value;
    isOpen = false;
    filter = '';
    onChange?.(opt.value);
  }

  function handleWindowClick(e: MouseEvent) {
    if (!isOpen) return;
    const target = e.target as HTMLElement;
    if (rootEl && !rootEl.contains(target)) {
      isOpen = false;
      filter = '';
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { isOpen = false; filter = ''; }
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleKeydown} />

<div class="combo-wrap" bind:this={rootEl}>
  <button
    type="button"
    class="combo-trigger"
    class:is-open={isOpen}
    class:is-disabled={disabled}
    onclick={toggle}
    aria-haspopup="listbox"
    aria-expanded={isOpen}
    aria-label={ariaLabel}
    {disabled}
  >
    <span class="combo-trigger-text" class:is-placeholder={!selected} title={selected?.label ?? ''}>
      {selected ? selected.label : placeholder}
    </span>
    <svg class="combo-caret" class:flipped={isOpen} width="10" height="10" viewBox="0 0 10 6" fill="none">
      <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </button>

  {#if isOpen}
    <div class="combo-panel" role="listbox" aria-label={ariaLabel}>
      {#if options.length > 6}
        <div class="combo-search">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input type="text" class="combo-search-input" placeholder="Search…" bind:value={filter} aria-label="Filter options" />
        </div>
      {/if}
      <div class="combo-list">
        {#if filtered.length === 0}
          <div class="combo-empty">{emptyText}</div>
        {:else}
          {#each filtered as opt (opt.value)}
            <button
              type="button"
              class="combo-item"
              class:is-active={opt.value === value}
              onclick={() => choose(opt)}
              role="option"
              aria-selected={opt.value === value}
            >
              <span class="combo-item-text">
                <span class="combo-item-label" title={opt.label}>{opt.label}</span>
                {#if opt.sublabel}<span class="combo-item-sublabel" title={opt.sublabel}>{opt.sublabel}</span>{/if}
              </span>
              {#if opt.value === value}
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

<style>
  .combo-wrap {
    position: relative;
    width: 100%;
  }

  .combo-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border-radius: 8px;
    background: rgba(10, 10, 22, 0.7);
    border: 1px solid rgba(139, 92, 246, 0.14);
    color: #e0e0f0;
    font-size: 12px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    cursor: pointer;
    transition: border-color 150ms, background 150ms;
  }
  .combo-trigger:hover:not(.is-disabled) {
    border-color: rgba(139, 92, 246, 0.3);
    background: rgba(14, 12, 30, 0.8);
  }
  .combo-trigger.is-open {
    border-color: rgba(191, 64, 255, 0.4);
    box-shadow: 0 0 0 3px rgba(191, 64, 255, 0.1);
  }
  .combo-trigger.is-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .combo-trigger-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .combo-trigger-text.is-placeholder { color: #5a5a7a; }

  .combo-caret {
    flex-shrink: 0;
    color: rgba(140, 100, 200, 0.6);
    transition: transform 180ms ease;
  }
  .combo-caret.flipped { transform: rotate(180deg); }

  .combo-panel {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    min-width: 260px;
    max-height: 320px;
    background: rgba(8, 6, 20, 0.97);
    backdrop-filter: blur(28px) saturate(160%);
    -webkit-backdrop-filter: blur(28px) saturate(160%);
    border: 1px solid rgba(191, 64, 255, 0.16);
    border-radius: 12px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(0,0,0,0.2);
    z-index: 60;
    overflow: hidden;
    animation: comboPanelIn 160ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes comboPanelIn {
    from { opacity: 0; transform: translateY(-4px) scale(0.98); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .combo-search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid rgba(191, 64, 255, 0.08);
    color: rgba(90, 80, 140, 0.6);
  }
  .combo-search-input {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    color: rgba(220, 210, 255, 0.9);
    font-size: 12px;
    font-family: var(--font-body, 'Raleway', sans-serif);
  }
  .combo-search-input::placeholder { color: rgba(80, 70, 130, 0.5); }

  .combo-list {
    overflow-y: auto;
    max-height: 270px;
    padding: 5px;
  }
  .combo-list::-webkit-scrollbar { width: 3px; }
  .combo-list::-webkit-scrollbar-track { background: transparent; }
  .combo-list::-webkit-scrollbar-thumb {
    background: rgba(191, 64, 255, 0.2);
    border-radius: 3px;
  }

  .combo-item {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: rgba(190, 180, 220, 0.85);
    font-size: 12px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    cursor: pointer;
    transition: background 120ms, color 120ms;
    text-align: left;
  }
  .combo-item:hover {
    background: rgba(191, 64, 255, 0.08);
    color: rgba(220, 205, 255, 0.95);
  }
  .combo-item.is-active {
    background: rgba(191, 64, 255, 0.12);
    color: #BF40FF;
  }

  .combo-item-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    flex: 1;
    min-width: 0;
  }
  .combo-item-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .combo-item-sublabel {
    font-size: 10px;
    color: rgba(120, 110, 160, 0.65);
    font-family: var(--font-body, 'Raleway', sans-serif);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .combo-empty {
    padding: 20px 14px;
    text-align: center;
    color: rgba(80, 70, 130, 0.55);
    font-size: 11px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    line-height: 1.7;
  }
</style>
