<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data } = $props();

  const categoryMeta: Record<string, { icon: string; label: string }> = {
    trait:         { icon: '🧬', label: 'Trait' },
    event:        { icon: '⚡', label: 'Event' },
    relationship: { icon: '💜', label: 'Relationship' },
    preference:   { icon: '⭐', label: 'Preference' },
    goal:         { icon: '🎯', label: 'Goal' },
    discovery:    { icon: '🔮', label: 'Discovery' },
    fact:         { icon: '📄', label: 'Fact' },
  };

  /** Parse the first memory for display on the top card face */
  let parsed = $derived.by(() => {
    const first = data.memories?.[0];
    const content = first?.content ?? '';
    const match = content.match(/^\[(\w+)\]\s*/);
    const category = match ? match[1].toLowerCase() : 'fact';
    const text = match ? content.slice(match[0].length) : content;
    const trunc = text.length > 80 ? text.slice(0, 77) + '…' : text;
    const meta = categoryMeta[category] ?? categoryMeta.fact;
    return { category, text: trunc, ...meta };
  });

  /** Category dots — one per memory, colored by its category */
  let categoryDots = $derived.by(() => {
    return (data.memories ?? []).map((m: { content: string }) => {
      const match = m.content?.match(/^\[(\w+)\]\s*/);
      const cat = match ? match[1].toLowerCase() : 'fact';
      return { category: cat, icon: categoryMeta[cat]?.icon ?? '📄' };
    });
  });

  let count = $derived((data.memories ?? []).length);
</script>

<Handle type="target" position={Position.Top} id="top" />

<div class="memgroup-node" style="--accent: {data.color ?? '#c4a1ff'};">
  <div class="memgroup-inner">
    <!-- Left accent strip — identical to MemoryNode -->
    <div class="memgroup-accent-strip"></div>

    <div class="memgroup-body">
      <!-- Header row: category left -->
      <div class="memgroup-header">
        <span class="memgroup-cat">
          <span class="cat-icon">{parsed.icon}</span>
          {parsed.label}
        </span>
        <span class="memgroup-count-pill">{count}</span>
      </div>

      <!-- Content text (from first memory) -->
      <p class="memgroup-text">{parsed.text}</p>

      <!-- Count badge with category dots -->
      <div class="memgroup-footer">
        <span class="memgroup-badge">
          {count} {count === 1 ? 'memory' : 'memories'}
          <span class="cat-dots">
            {#each categoryDots as dot}
              <span class="cat-dot" title={dot.category}></span>
            {/each}
          </span>
        </span>
      </div>
    </div>
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .memgroup-node {
    position: relative;
    border-radius: 12px;
    overflow: visible; /* pseudo-element stacked cards need this */
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    font-family: 'Inter', -apple-system, sans-serif;
  }

  /* Inner wrapper clips the accent strip at the rounded corners */
  .memgroup-inner {
    display: flex;
    background: linear-gradient(135deg, #0e0e1e, #141028);
    border: 1px solid rgba(45, 36, 88, 0.6);
    border-radius: 12px;
    overflow: hidden; /* ← clips accent strip at corners */
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    position: relative;
    z-index: 1;
  }

  /* ── Stacked deck pseudo-elements ── */
  .memgroup-node::before,
  .memgroup-node::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    border-radius: 12px;
    border: 1px solid rgba(45, 36, 88, 0.4);
    pointer-events: none;
  }

  /* Shadow card 1 — closer, slightly visible */
  .memgroup-node::before {
    background: linear-gradient(135deg, #0c0c1a, #120e24);
    transform: translate(2px, 2px);
    opacity: 0.55;
    z-index: -1;
  }

  /* Shadow card 2 — further, fainter */
  .memgroup-node::after {
    background: linear-gradient(135deg, #0a0a18, #100c20);
    transform: translate(4px, 4px);
    opacity: 0.3;
    z-index: -2;
  }

  /* Left accent strip — clipped by .memgroup-inner's overflow:hidden */
  .memgroup-accent-strip {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(
      180deg,
      var(--accent),
      color-mix(in srgb, var(--accent) 25%, transparent)
    );
  }

  .memgroup-body {
    flex: 1;
    min-width: 0;
    padding: 8px 10px;
  }

  .memgroup-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .memgroup-cat {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.8px;
  }

  .cat-icon {
    font-size: 11px;
    line-height: 1;
  }

  .memgroup-count-pill {
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 1px 6px;
    border-radius: 6px;
    line-height: 1.4;
  }

  .memgroup-text {
    font-size: 12px;
    line-height: 1.55;
    color: #cbc5dd;
    margin: 0;
  }

  .memgroup-footer {
    display: flex;
    margin-top: 8px;
  }

  .memgroup-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    opacity: 0.7;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    padding: 3px 8px;
    border-radius: 10px;
  }

  .cat-dots {
    display: inline-flex;
    gap: 3px;
    align-items: center;
  }

  .cat-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.6;
    flex-shrink: 0;
  }

  /* ── Light theme ── */
  :global([data-theme="light"]) .memgroup-inner {
    background: linear-gradient(135deg, rgba(255,255,255,0.9), rgba(245,242,250,0.95));
    border-color: rgba(139,92,246,0.1);
    box-shadow: 0 2px 12px rgba(0,0,0,0.06);
  }

  :global([data-theme="light"]) .memgroup-node::before {
    background: linear-gradient(135deg, rgba(250,250,255,0.7), rgba(240,237,248,0.8));
    border-color: rgba(139,92,246,0.08);
    opacity: 0.6;
  }

  :global([data-theme="light"]) .memgroup-node::after {
    background: linear-gradient(135deg, rgba(248,248,255,0.5), rgba(238,235,246,0.6));
    border-color: rgba(139,92,246,0.06);
    opacity: 0.35;
  }

  :global([data-theme="light"]) .memgroup-text {
    color: #2a2a3e;
  }

  :global([data-theme="light"]) .memgroup-badge {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
</style>
