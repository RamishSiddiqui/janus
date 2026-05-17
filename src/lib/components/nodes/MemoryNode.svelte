<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data } = $props();

  const categoryMeta: Record<string, { icon: string; label: string }> = {
    trait:        { icon: '🧬', label: 'Trait' },
    event:       { icon: '⚡', label: 'Event' },
    relationship: { icon: '💜', label: 'Relationship' },
    preference:  { icon: '⭐', label: 'Preference' },
    goal:        { icon: '🎯', label: 'Goal' },
    discovery:   { icon: '🔮', label: 'Discovery' },
    fact:        { icon: '📄', label: 'Fact' },
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

<div class="mem-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-bg: {data.colorBg ?? 'rgba(139,92,246,0.08)'}; --accent-border: {data.colorBorder ?? 'rgba(139,92,246,0.2)'};">
  <!-- Category strip -->
  <div class="mem-strip">
    <div class="strip-left">
      <span class="cat-icon">{parsed.icon}</span>
      <span class="cat-label">{parsed.label}</span>
    </div>
    <div class="strip-right">
      {#if data.version > 1}
        <span class="pill version">v{data.version}</span>
      {/if}
      <span class="pill source" class:auto={data.source === 'auto'}>
        {data.source === 'auto' ? '🤖' : '📌'}
      </span>
    </div>
  </div>

  <!-- Content -->
  <p class="mem-content">{parsed.text}</p>

  <!-- Status bar -->
  {#if data.isCanon || data.parentId}
    <div class="mem-status">
      {#if data.isCanon}
        <span class="status-badge canon">
          <span class="status-dot canon-dot"></span>
          Canon
        </span>
      {/if}
      {#if data.parentId}
        <span class="status-badge inherited">
          <span class="status-dot inherited-dot"></span>
          Inherited
        </span>
      {/if}
    </div>
  {/if}
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .mem-node {
    background: linear-gradient(180deg, rgba(10,10,26,0.97), rgba(14,12,28,0.97));
    border: 1px solid var(--accent-border);
    border-radius: 12px;
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    position: relative;
    overflow: hidden;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  .mem-node:hover {
    border-color: var(--accent);
    box-shadow:
      0 0 16px color-mix(in srgb, var(--accent) 15%, transparent),
      0 4px 12px rgba(0,0,0,0.25);
  }

  /* Accent top line */
  .mem-node::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: linear-gradient(90deg,
      transparent 5%,
      color-mix(in srgb, var(--accent) 60%, transparent) 30%,
      var(--accent) 50%,
      color-mix(in srgb, var(--accent) 60%, transparent) 70%,
      transparent 95%
    );
    opacity: 0.5;
  }

  /* Category strip */
  .mem-strip {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 7px 10px 5px;
    border-bottom: 1px solid rgba(255,255,255,0.03);
  }

  .strip-left {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .cat-icon {
    font-size: 11px;
    line-height: 1;
  }

  .cat-label {
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.8px;
    opacity: 0.65;
  }

  .strip-right {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .pill {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
    line-height: 1.4;
  }

  .pill.version {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    opacity: 0.7;
  }

  .pill.source {
    font-size: 10px;
    line-height: 1;
    opacity: 0.6;
  }

  /* Content */
  .mem-content {
    font-size: 11px;
    line-height: 1.55;
    color: rgba(230, 225, 245, 0.85);
    margin: 0;
    padding: 6px 10px 8px;
    letter-spacing: 0.05px;
  }

  /* Status bar */
  .mem-status {
    display: flex;
    gap: 6px;
    padding: 0 10px 7px;
  }

  .status-badge {
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

  .status-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-badge.canon {
    background: rgba(218, 165, 32, 0.12);
    color: #fbbf24;
  }

  .canon-dot {
    background: #fbbf24;
    box-shadow: 0 0 4px rgba(218,165,32,0.5);
  }

  .status-badge.inherited {
    background: rgba(139, 92, 246, 0.08);
    color: #9d8abf;
  }

  .inherited-dot {
    background: #9d8abf;
    box-shadow: 0 0 4px rgba(139,92,246,0.3);
  }
</style>
