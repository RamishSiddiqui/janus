<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';

  let { data } = $props();

  const categoryIcons: Record<string, string> = {
    trait: '🧬',
    event: '⚡',
    relationship: '💜',
    preference: '⭐',
    goal: '🎯',
    discovery: '🔮',
    fact: '📄',
  };

  // Parse category from content like "[trait] ..."
  let parsed = $derived.by(() => {
    const match = data.content?.match(/^\[(\w+)\]\s*/);
    const category = match ? match[1].toLowerCase() : 'fact';
    const text = match ? data.content.slice(match[0].length) : (data.content ?? data.label);
    const trunc = text.length > 60 ? text.slice(0, 57) + '…' : text;
    return { category, text: trunc, icon: categoryIcons[category] ?? '📄' };
  });
</script>

<Handle type="target" position={Position.Top} />

<div class="mem-node" style="--accent: {data.color ?? '#c4a1ff'}; --accent-bg: {data.colorBg ?? 'rgba(139,92,246,0.08)'}; --accent-border: {data.colorBorder ?? 'rgba(139,92,246,0.2)'};">
  <!-- Header row -->
  <div class="mem-header">
    <span class="mem-cat">
      <span class="cat-icon">{parsed.icon}</span>
      {parsed.category}
    </span>
    <div class="mem-tags">
      {#if data.version > 1}
        <span class="tag ver">v{data.version}</span>
      {/if}
      <span class="tag source" class:auto={data.source === 'auto'}>
        {data.source === 'auto' ? '🤖' : '📌'}
      </span>
    </div>
  </div>

  <!-- Content -->
  <p class="mem-text">{parsed.text}</p>

  <!-- Footer -->
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

<Handle type="source" position={Position.Bottom} />

<style>
  .mem-node {
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    border-radius: 10px;
    padding: 8px 12px;
    width: 100%;
    box-sizing: border-box;
    cursor: grab;
    position: relative;
    isolation: isolate;
  }

  /* Opaque dark base so edges behind the node are hidden */
  .mem-node::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: #0a0a1a;
    z-index: -1;
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
    gap: 3px;
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    opacity: 0.7;
  }

  .cat-icon {
    font-size: 11px;
  }

  .mem-tags {
    display: flex;
    gap: 3px;
  }

  .tag {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
  }

  .tag.ver {
    background: rgba(139, 92, 246, 0.12);
    color: #c4a1ff;
  }

  .tag.source {
    font-size: 10px;
    line-height: 1;
  }

  .mem-text {
    font-size: 11px;
    line-height: 1.45;
    color: var(--accent);
    margin: 0;
    opacity: 0.9;
  }

  .mem-footer {
    display: flex;
    gap: 4px;
    margin-top: 6px;
  }

  .badge {
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    letter-spacing: 0.3px;
  }

  .badge.canon {
    background: rgba(218, 165, 32, 0.15);
    color: #daa520;
  }

  .badge.inherited {
    background: rgba(139, 92, 246, 0.1);
    color: #8b8ba7;
  }
</style>
