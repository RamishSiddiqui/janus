<script lang="ts">
  import { getBezierPath, BaseEdge } from '@xyflow/svelte';

  let {
    id,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    data,
    style,
    markerEnd,
    markerStart,
  } = $props();

  const isSync = data?.linkType === 'sync';
  const isTwoWay = data?.direction === 'two_way';
  const offset = 4; // parallel offset in px

  // Calculate the path
  let pathResult = $derived(getBezierPath({
    sourceX, sourceY, targetX, targetY,
    sourcePosition, targetPosition,
  }));

  let mainPath = $derived(pathResult[0]);

  // Compute a perpendicular offset for parallel lines
  let dx = $derived(targetX - sourceX);
  let dy = $derived(targetY - sourceY);
  let len = $derived(Math.sqrt(dx * dx + dy * dy) || 1);
  let nx = $derived(-dy / len * offset);
  let ny = $derived(dx / len * offset);

  // Offset paths
  let path1Result = $derived(getBezierPath({
    sourceX: sourceX + nx, sourceY: sourceY + ny,
    targetX: targetX + nx, targetY: targetY + ny,
    sourcePosition, targetPosition,
  }));
  let path2Result = $derived(getBezierPath({
    sourceX: sourceX - nx, sourceY: sourceY - ny,
    targetX: targetX - nx, targetY: targetY - ny,
    sourcePosition, targetPosition,
  }));

  let path1 = $derived(path1Result[0]);
  let path2 = $derived(path2Result[0]);

  // Colors
  const color = isSync ? 'rgba(0,242,255,0.45)' : 'rgba(139,92,246,0.4)';
  const glowColor = isSync ? 'rgba(0,242,255,0.15)' : 'rgba(139,92,246,0.1)';
</script>

<!-- Glow underlay -->
<path
  d={mainPath}
  fill="none"
  stroke={glowColor}
  stroke-width="12"
  class="glow-path"
/>

{#if isTwoWay}
  <!-- Two-way: parallel lines with opposite flow -->
  <path
    d={path1}
    fill="none"
    stroke={color}
    stroke-width="1.5"
    stroke-dasharray="6 4"
    class="flow-line forward"
  />
  <path
    d={path2}
    fill="none"
    stroke={color}
    stroke-width="1.5"
    stroke-dasharray="6 4"
    class="flow-line reverse"
  />

  <!-- Arrow dots flowing forward on path1 -->
  <circle r="3" fill={color} class="flow-dot">
    <animateMotion dur="2.5s" repeatCount="indefinite" path={path1} />
  </circle>
  <!-- Arrow dots flowing reverse on path2 -->
  <circle r="3" fill={color} class="flow-dot">
    <animateMotion dur="2.5s" repeatCount="indefinite" path={path2} keyPoints="1;0" keyTimes="0;1" />
  </circle>
{:else}
  <!-- One-way: single animated line -->
  <path
    d={mainPath}
    fill="none"
    stroke={color}
    stroke-width="1.5"
    stroke-dasharray="6 4"
    class="flow-line forward"
  />
  <circle r="3" fill={color} class="flow-dot">
    <animateMotion dur="2.5s" repeatCount="indefinite" path={mainPath} />
  </circle>
{/if}

<!-- Label badge -->
{#if data?.label}
  {@const midX = (sourceX + targetX) / 2}
  {@const midY = (sourceY + targetY) / 2}
  <foreignObject x={midX - 28} y={midY - 10} width="56" height="20" class="label-fo">
    <div class="edge-badge" style="--badge-color: {color};">
      {data.label}
    </div>
  </foreignObject>
{/if}

<style>
  .glow-path {
    filter: blur(6px);
    opacity: 0.5;
  }

  .flow-line.forward {
    animation: dashFlow 1.2s linear infinite;
  }

  .flow-line.reverse {
    animation: dashFlowReverse 1.2s linear infinite;
  }

  @keyframes dashFlow {
    to { stroke-dashoffset: -20; }
  }

  @keyframes dashFlowReverse {
    to { stroke-dashoffset: 20; }
  }

  .flow-dot {
    filter: drop-shadow(0 0 4px currentColor);
    opacity: 0.9;
  }

  .edge-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    font-weight: 700;
    font-family: Inter, sans-serif;
    color: var(--badge-color);
    background: rgba(7, 7, 26, 0.9);
    border: 1px solid var(--badge-color);
    border-radius: 6px;
    padding: 2px 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    white-space: nowrap;
    width: fit-content;
    margin: 0 auto;
  }
</style>
