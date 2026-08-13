<script lang="ts">
  let {
    size = 22,
    goldColor = '#CDA15F',
    violetColor = '#9075F2',
  }: {
    size?: number;
    goldColor?: string;
    violetColor?: string;
  } = $props();
</script>

<svg viewBox="-61 -83 122 166" width={size} height={size * (166 / 122)} aria-hidden="true">
  <g fill="none" stroke-width="18" stroke-linecap="round">
    <path stroke={goldColor} d="M 0,2 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52" />
    <path stroke={violetColor} d="M 0,-2 L 0,46 Q 0,74 -30,74 Q -52,74 -52,52" />
  </g>
</svg>
