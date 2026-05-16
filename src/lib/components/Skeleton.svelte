<script lang="ts">
  let {
    variant = 'text',
    width = '100%',
    height,
    count = 1,
  }: {
    variant?: 'text' | 'circle' | 'card' | 'rect';
    width?: string;
    height?: string;
    count?: number;
  } = $props();

  const heights: Record<string, string> = {
    text: '12px',
    circle: '40px',
    card: '180px',
    rect: '40px',
  };

  const radii: Record<string, string> = {
    text: '4px',
    circle: '999px',
    card: '14px',
    rect: '10px',
  };

  const resolvedHeight = height ?? heights[variant];
  const resolvedRadius = radii[variant];
  const resolvedWidth = variant === 'circle' ? resolvedHeight : width;
</script>

{#each Array(count) as _, i}
  <div
    class="skeleton"
    style="width:{resolvedWidth};height:{resolvedHeight};border-radius:{resolvedRadius};"
    role="status"
    aria-label="Loading"
  ></div>
{/each}

<style>
  .skeleton {
    background: linear-gradient(
      90deg,
      rgba(14,14,30,0.5) 25%,
      rgba(139,92,246,0.06) 50%,
      rgba(14,14,30,0.5) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.8s ease-in-out infinite;
  }

  @keyframes shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
</style>
