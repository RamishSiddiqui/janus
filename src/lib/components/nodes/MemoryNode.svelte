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

  let parsed = $derived.by(() => {
    const match = data.content?.match(/^\[(\w+)\]\s*/);
    const category = match ? match[1].toLowerCase() : 'fact';
    const text = match ? data.content.slice(match[0].length) : (data.content ?? data.label);
    const trunc = text.length > 80 ? text.slice(0, 77) + '…' : text;
    const meta = categoryMeta[category] ?? categoryMeta.fact;
    return { category, text: trunc, ...meta };
  });
</script>

<Handle type="target" position={Position.Top} id="top" />

<div class="mem-node" style="--accent: {data.color ?? '#c4a1ff'};">
  <!-- Left accent strip — the ONLY colored element -->
  <div class="mem-accent-strip"></div>

  <div class="mem-body">
    <!-- Header row: category left, pills right -->
    <div class="mem-header">
      <span class="mem-cat">
        <span class="cat-icon">{parsed.icon}</span>
        {parsed.label}
      </span>
      <div class="mem-pills">
        {#if data.version > 1}
          <span class="pill ver">v{data.version}</span>
        {/if}
        <span class="pill src">{data.source === 'auto' ? '🤖' : '📌'}</span>
      </div>
    </div>

    <!-- Content text -->
    <p class="mem-text">{parsed.text}</p>

    <!-- Footer badges -->
    {#if data.isCanon || data.parentId}
      <div class="mem-footer">
        {#if data.isCanon}
          <span class="badge canon"><span class="dot"></span>Canon</span>
        {/if}
        {#if data.parentId}
          <span class="badge inherited"><span class="dot"></span>Inherited</span>
        {/if}
      </div>
    {/if}
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .mem-node {
    display: flex;
    /* Always dark — never tinted by accent color */
    background: linear-gradient(135deg, #0e0e1e, #141028);
    /* Subtle structural border only — not accent-colored */
    border: 1px solid rgba(45, 36, 88, 0.6);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    font-family: 'Raleway', -apple-system, sans-serif;
  }

  /* Left accent strip — the only place accent color appears */
  .mem-accent-strip {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(
      180deg,
      var(--accent),
      color-mix(in srgb, var(--accent) 25%, transparent)
    );
  }

  .mem-body {
    flex: 1;
    min-width: 0;
    padding: 8px 10px;
  }

  .mem-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .mem-cat {
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

  .mem-pills {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .pill {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
  }

  .pill.ver {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }

  .pill.src {
    font-size: 10px;
    line-height: 1;
  }

  .mem-text {
    font-size: 12px;
    line-height: 1.55;
    color: #cbc5dd;
    margin: 0;
  }

  .mem-footer {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 8px;
    font-weight: 700;
    padding: 2px 7px 2px 5px;
    border-radius: 6px;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }

  .badge .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .badge.canon {
    background: rgba(218, 165, 32, 0.12);
    color: #fbbf24;
  }

  .badge.canon .dot {
    background: #fbbf24;
    box-shadow: 0 0 4px rgba(218, 165, 32, 0.5);
  }

  .badge.inherited {
    background: rgba(139, 92, 246, 0.08);
    color: #8b8ba7;
  }

  .badge.inherited .dot {
    background: #8b8ba7;
  }

  /* ── Light theme ── */
  :global([data-theme="light"]) .mem-node {
    background: linear-gradient(135deg, rgba(255,255,255,0.9), rgba(245,242,250,0.95));
    border-color: rgba(139,92,246,0.1);
    box-shadow: 0 2px 12px rgba(0,0,0,0.06);
  }

  :global([data-theme="light"]) .mem-text {
    color: #2a2a3e;
  }

  :global([data-theme="light"]) .badge.inherited {
    background: rgba(139,92,246,0.06);
  }
</style>
