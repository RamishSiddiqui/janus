<script lang="ts">
  const uid = $props.id();
  const gradId = `janus-split-${uid}`;

  let {
    size = 22,
    goldColor = '#CDA15F',
    violetColor = '#9075F2',
    label = 'Loading',
  }: {
    size?: number;
    goldColor?: string;
    violetColor?: string;
    /** Accessible name — should describe the actual state (Loading/Thinking/Saving). */
    label?: string;
  } = $props();
</script>

<svg
  class="janus-loader"
  viewBox="-61 -83 122 166"
  width={size}
  height={size * (166 / 122)}
  role="img"
  aria-label={label}
>
  <defs>
    <!-- Colour is fixed in place along the mark, not carried by the moving
         segment, so the ink never changes colour as it travels through the
         centre — this is what the earlier draw-in/draw-out version got
         wrong (a hard colour snap at the midpoint). -->
    <linearGradient id={gradId} gradientUnits="userSpaceOnUse" x1="0" y1="-80" x2="0" y2="80">
      <stop offset="0" stop-color={goldColor} />
      <stop offset="0.485" stop-color={goldColor} />
      <stop offset="0.515" stop-color={violetColor} />
      <stop offset="1" stop-color={violetColor} />
    </linearGradient>
  </defs>
  <g fill="none" stroke-width="18" stroke-linecap="round">
    <!-- Both hooks are ONE path: bottom-left hook, up the stem, top-right
         hook. The track is the same gradient at low alpha, so the mark
         stays legible and needs no assumption about the background. -->
    <path stroke="url(#{gradId})" opacity="0.22"
      d="M -52,52 Q -52,74 -30,74 Q 0,74 0,46 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52" />
    <!-- ONE runner — two runners would need their coverage tiled exactly,
         and any gap renders as a detached round cap floating off the mark. -->
    <path class="run" stroke="url(#{gradId})" pathLength="100" stroke-dasharray="62 200"
      d="M -52,52 Q -52,74 -30,74 Q 0,74 0,46 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52" />
  </g>
</svg>

<style>
  /* A dashed segment leaving an open path shrinks to a round dot the width
     of the stroke, and a dot can't shrink further — so it blinks out. Fix:
     fade the runner to zero across the first/last ~11% of its run instead
     of letting the dash hide the ends. */
  @keyframes janus-run  { from { stroke-dashoffset: 62; } to { stroke-dashoffset: -100; } }
  @keyframes janus-fade { 0%, 100% { opacity: 0; } 11%, 89% { opacity: 1; } }

  .janus-loader .run {
    animation: janus-run 2.6s linear infinite, janus-fade 2.6s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .janus-loader .run { animation: none; stroke-dasharray: none; opacity: 1; }
  }
</style>
