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

  let color = $derived(data?.color ?? 'rgba(139,92,246,0.4)');

  let pathResult = $derived(getSmoothStepPath({
    sourceX, sourceY, targetX, targetY,
    sourcePosition, targetPosition,
    borderRadius: 16,
  }));

  let edgePath = $derived(pathResult[0]);
</script>

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
  stroke={color}
  stroke-width="1.5"
  stroke-opacity="0.55"
  stroke-linecap="round"
/>

<!-- Bright dot traveling along the path -->
<circle r="2" class="travel-dot" style="--dot-color: {color};">
  <animateMotion dur="4s" repeatCount="indefinite" path={edgePath} />
</circle>

<style>
  .glow-underlay {
    opacity: 0.07;
    filter: blur(4px);
  }

  .travel-dot {
    fill: var(--dot-color);
    filter: drop-shadow(0 0 3px var(--dot-color));
    opacity: 0.7;
  }
</style>
