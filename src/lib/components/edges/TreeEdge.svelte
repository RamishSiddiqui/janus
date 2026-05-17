<script lang="ts">
  import { getSmoothStepPath } from '@xyflow/svelte';

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
  } = $props();

  // Color from data (passed by MemoryGraph)
  let color = $derived(data?.color ?? 'rgba(139,92,246,0.4)');

  let pathResult = $derived(getSmoothStepPath({
    sourceX, sourceY, targetX, targetY,
    sourcePosition, targetPosition,
    borderRadius: 16,
  }));

  let edgePath = $derived(pathResult[0]);

  // Unique gradient ID per edge
  let gradId = $derived(`tree-grad-${id}`);
</script>

<defs>
  <linearGradient id={gradId} x1="0" y1="0" x2="0" y2="1">
    <stop offset="0%" stop-color={color} stop-opacity="0.15" />
    <stop offset="40%" stop-color={color} stop-opacity="0.6" />
    <stop offset="100%" stop-color={color} stop-opacity="0.3" />
  </linearGradient>
</defs>

<!-- Soft glow underlay -->
<path
  d={edgePath}
  fill="none"
  stroke={color}
  stroke-width="8"
  class="glow-underlay"
/>

<!-- Main edge path -->
<path
  d={edgePath}
  fill="none"
  stroke="url(#{gradId})"
  stroke-width="1.5"
  class="main-edge"
/>

<!-- Bright dot traveling along the path -->
<circle r="2" class="travel-dot" style="--dot-color: {color};">
  <animateMotion dur="4s" repeatCount="indefinite" path={edgePath} />
</circle>

<style>
  .glow-underlay {
    opacity: 0.06;
    filter: blur(4px);
  }

  .main-edge {
    stroke-linecap: round;
  }

  .travel-dot {
    fill: var(--dot-color);
    filter: drop-shadow(0 0 3px var(--dot-color));
    opacity: 0.7;
  }
</style>
