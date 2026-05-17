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

<div class="mem-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-dim: {data.colorBorder ?? 'rgba(139,92,246,0.2)'};">
  <!-- Left accent strip -->
  <div class="mem-accent-strip"></div>

  <div class="mem-body">
    <!-- Header -->
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

    <!-- Content -->
    <p class="mem-text">{parsed.text}</p>

    <!-- Footer badges -->
    {#if data.isCanon || data.parentId}
      <div class="mem-footer">
        {#if data.isCanon}
          <span class="badge canon">Canon</span>
        {/if}
        {#if data.parentId}
          <span class="badge inherited">Inherited</span>
        {/if}
      </div>
    {/if}
  </div>
</div>

<Handle type="source" position={Position.Bottom} id="bottom" />

<style>
  .mem-node {
    display: flex;
    background: linear-gradient(135deg, #0e0e1e, #141028);
    border: 1px solid var(--accent-dim);
    border-radius: 12px;
    overflow: hidden;
    box-shadow:
      0 0 20px rgba(0, 0, 0, 0.3),
      0 0 8px color-mix(in srgb, var(--accent) 6%, transparent);
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
  }

  /* Colored left edge strip */
  .mem-accent-strip {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(180deg, var(--accent), color-mix(in srgb, var(--accent) 30%, transparent));
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
    margin-bottom: 5px;
  }

  .mem-cat {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .cat-icon {
    font-size: 12px;
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
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
  }

  .pill.src {
    font-size: 11px;
    line-height: 1;
  }

  .mem-text {
    font-size: 12px;
    line-height: 1.5;
    color: #cbc5dd;
    margin: 0 0 2px;
  }

  .mem-footer {
    display: flex;
    gap: 5px;
    margin-top: 6px;
  }

  .badge {
    font-size: 9px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 5px;
    letter-spacing: 0.2px;
  }

  .badge.canon {
    background: rgba(218, 165, 32, 0.15);
    color: #fbbf24;
  }

  .badge.inherited {
    background: rgba(139, 92, 246, 0.1);
    color: #8b8ba7;
  }
</style>
