<script lang="ts">
  let {
    text,
    goldColor = '#CDA15F',
    violetColor = '#9075F2',
  }: {
    text: string;
    goldColor?: string;
    violetColor?: string;
  } = $props();

  // Same ~40/60 split ratio as the JANUS wordmark ("JA" / "NUS" — 2 of 5
  // letters, then the rest). Splits on a word boundary near that point when
  // there's more than one word, so it doesn't cut through the middle of a
  // word for short multi-word headings — falls back to a straight character
  // split for single words.
  let splitIndex = $derived.by(() => {
    const target = Math.round(text.length * 0.4);
    const spaceIdx = text.indexOf(' ');
    if (spaceIdx !== -1 && Math.abs(spaceIdx - target) <= 2) return spaceIdx;
    return Math.max(1, target);
  });
</script>

<span class="split-heading">
  <span class="split-heading-a" style:color={violetColor}>{text.slice(0, splitIndex)}</span
  ><span class="split-heading-b" style:color={goldColor}>{text.slice(splitIndex)}</span>
</span>

<style>
  .split-heading { display: inline; }
</style>
