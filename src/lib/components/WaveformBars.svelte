<script lang="ts">
  // Small live level-meter for TTS preview playback — bars scale with
  // real-time amplitude read off an AnalyserNode. Idle (analyser null, or
  // active false) shows flat resting bars rather than disappearing, so the
  // layout doesn't jump when playback starts/stops.
  //
  // Peak amplitude, not RMS — RMS is a smoothed loudness average and reads
  // as barely-moving for speech; peak-per-frame is punchier and actually
  // looks "alive" at this small a size. Center-weighted per-bar multiplier
  // (classic equalizer shape: tallest in the middle) rather than splitting
  // FFT frequency bins across bars — speech energy is almost entirely
  // low-frequency, so a frequency-bin split left every bar but the
  // leftmost sitting dead flat.
  let {
    analyser = null,
    active = false,
    barCount = 5,
    color = 'var(--accent, #8b5cf6)',
  }: {
    analyser: AnalyserNode | null;
    active?: boolean;
    barCount?: number;
    color?: string;
  } = $props();

  const REST_LEVEL = 0.08;
  const barWeights = Array.from({ length: barCount }, (_, i) => {
    const mid = (barCount - 1) / 2;
    const dist = Math.abs(i - mid) / (mid || 1);
    return 1 - dist * 0.4; // center bar at 1.0, edges at ~0.6
  });

  let levels = $state<number[]>(Array(barCount).fill(REST_LEVEL));
  let rafId: number | null = null;
  let envelope = 0;

  function tick() {
    if (!analyser || !active) {
      rafId = null;
      return;
    }
    const data = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(data);
    let peak = 0;
    for (let i = 0; i < data.length; i++) {
      const centered = Math.abs(data[i] - 128) / 128; // 0..1
      if (centered > peak) peak = centered;
    }
    // Fast attack (jump toward a new peak immediately), slower decay (fall
    // off over a few frames) — reads as responsive rather than flickery.
    envelope = peak > envelope ? peak : envelope * 0.7;
    const boosted = Math.min(1, envelope * 2.2);

    levels = barWeights.map((w) => Math.max(REST_LEVEL, boosted * w));
    rafId = requestAnimationFrame(tick);
  }

  $effect(() => {
    if (active && analyser) {
      if (rafId === null) tick();
    } else {
      if (rafId !== null) cancelAnimationFrame(rafId);
      rafId = null;
      envelope = 0;
      levels = Array(barCount).fill(REST_LEVEL);
    }
    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
    };
  });
</script>

<div class="waveform-bars" class:active aria-hidden="true">
  {#each levels as level, i (i)}
    <span class="waveform-bar" style="transform: scaleY({level}); background: {color};"></span>
  {/each}
</div>

<style>
  .waveform-bars {
    display: inline-flex;
    align-items: flex-end;
    justify-content: center;
    gap: 3px;
    height: 18px;
    width: 26px;
    flex-shrink: 0;
    opacity: 0.6;
    transition: opacity 150ms ease-out;
  }
  .waveform-bars.active {
    opacity: 1;
  }
  .waveform-bar {
    width: 3px;
    height: 100%;
    border-radius: 1.5px;
    transform-origin: bottom;
    transition: transform 70ms ease-out;
  }
</style>
