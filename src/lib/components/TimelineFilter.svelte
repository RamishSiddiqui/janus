<script lang="ts">
  import Icon from './Icon.svelte';

  interface ConvOption {
    id: string;
    title: string;
  }

  let {
    conversations,
    selected,
    onToggle,
    onToggleAll,
  }: {
    conversations: ConvOption[];
    selected: Set<string>;
    onToggle: (id: string) => void;
    onToggleAll: () => void;
  } = $props();

  // Same cycling palette MemoryGraph and MemoryTimeline already use for
  // conversation coloring, indexed the same way (by conversations[] order)
  // so a dot here matches its lane/node color in whichever view is open.
  const PALETTE = ['#c4a1ff', '#00f2ff', '#fb7185', '#fbbf24', '#34d399', '#d580ff'];
  function colorFor(i: number): string {
    return PALETTE[i % PALETTE.length];
  }

  let open = $state(false);
  let rootEl: HTMLDivElement | undefined = $state();

  let allSelected = $derived(conversations.length > 0 && selected.size === conversations.length);

  let label = $derived.by(() => {
    if (conversations.length === 0) return 'Timelines';
    if (allSelected) return `All timelines (${conversations.length})`;
    if (selected.size === 0) return 'No timelines';
    return `${selected.size} of ${conversations.length} timelines`;
  });

  function handleClickOutside(e: MouseEvent): void {
    if (rootEl && !rootEl.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onmousedown={handleClickOutside} />

{#if conversations.length > 1}
  <div class="tf-root" bind:this={rootEl}>
    <button
      type="button"
      class="tf-btn"
      onclick={() => (open = !open)}
      aria-expanded={open}
      aria-label="Filter which timelines are shown"
    >
      <Icon name="filter" size={11} color={allSelected ? '#5a5a7a' : '#c4a1ff'} />
      <span>{label}</span>
      <span class="tf-chevron" class:open>
        <Icon name="chevron-down" size={10} />
      </span>
    </button>
    {#if open}
      <div class="tf-dropdown">
        <button type="button" class="tf-selectall" onclick={onToggleAll}>
          <span class="tf-check" class:checked={allSelected}>
            {#if allSelected}<Icon name="check" size={9} />{/if}
          </span>
          <span class="tf-label">Select All</span>
        </button>
        <div class="tf-divider"></div>
        {#each conversations as c, i (c.id)}
          <button type="button" class="tf-option" onclick={() => onToggle(c.id)}>
            <span class="tf-check" class:checked={selected.has(c.id)}>
              {#if selected.has(c.id)}<Icon name="check" size={9} />{/if}
            </span>
            <span class="tf-dot" style="background: {colorFor(i)};"></span>
            <span class="tf-label">{c.title}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .tf-root {
    position: relative;
    margin-left: auto;
  }

  .tf-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 999px;
    background: rgba(139, 92, 246, 0.05);
    border: 1px solid rgba(139, 92, 246, 0.16);
    color: #b8a8e8;
    font-family: 'Raleway', -apple-system, sans-serif;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 150ms ease;
    white-space: nowrap;
  }

  .tf-btn:hover {
    border-color: rgba(139, 92, 246, 0.32);
    background: rgba(139, 92, 246, 0.09);
    color: #e8e0ff;
  }

  .tf-chevron {
    display: flex;
    transition: transform 200ms ease;
  }

  .tf-chevron.open {
    transform: rotate(180deg);
  }

  .tf-dropdown {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 30;
    min-width: 200px;
    max-height: 260px;
    overflow-y: auto;
    padding: 6px;
    border-radius: 10px;
    background: rgba(12, 12, 26, 0.98);
    border: 1px solid rgba(139, 92, 246, 0.16);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(10px);
    animation: tfDropdownIn 140ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes tfDropdownIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .tf-selectall,
  .tf-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    background: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    font-family: 'Raleway', -apple-system, sans-serif;
    transition: background 120ms ease;
  }

  .tf-selectall:hover,
  .tf-option:hover {
    background: rgba(139, 92, 246, 0.08);
  }

  .tf-divider {
    height: 1px;
    margin: 4px 2px;
    background: rgba(139, 92, 246, 0.1);
  }

  .tf-check {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    border-radius: 4px;
    border: 1px solid rgba(139, 92, 246, 0.3);
    color: #0a0a18;
    transition: all 120ms ease;
  }

  .tf-check.checked {
    background: #9075f2;
    border-color: #9075f2;
    color: #fff;
  }

  .tf-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tf-label {
    font-size: 11.5px;
    color: #c8c8e0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tf-selectall .tf-label {
    font-weight: 700;
    color: #e8e0ff;
  }
</style>
