<script lang="ts">
  import type { CharacterState } from '$lib/services/ipc';

  let { state }: { state: CharacterState } = $props();

  // Curated palette mapping dominant emotions to accent colours
  const EMOTION_COLOURS: Record<string, string> = {
    curious:    '#8B5CF6',
    guarded:    '#6366F1',
    elated:     '#10B981',
    anxious:    '#F59E0B',
    tender:     '#EC4899',
    wary:       '#64748B',
    angry:      '#EF4444',
    sad:        '#3B82F6',
    neutral:    '#6B7280',
    playful:    '#A855F7',
    melancholy: '#0EA5E9',
    hopeful:    '#06B6D4',
    amused:     '#F97316',
    conflicted: '#7C3AED',
    serene:     '#14B8A6',
  };

  let colour = $derived(
    EMOTION_COLOURS[state.dominant_emotion.toLowerCase()] ?? '#8B5CF6'
  );

  // Normalise bar heights: map [0,100] → [3px, 12px]
  let moodH    = $derived(3 + state.mood    * 0.09);
  let trustH   = $derived(3 + state.trust   * 0.09);
  let arousalH = $derived(3 + state.arousal * 0.09);
</script>

<!--
  EmotionHUD — a compact pill showing the character's live emotional state.
  Appears as part of the message toolbar on assistant messages.
  Tooltip reveals the full state_summary for extra context on hover.
-->
<div
  class="ehud"
  title={state.state_summary || state.dominant_emotion}
  aria-label="Character emotion: {state.dominant_emotion}"
  role="status"
>
  <span class="dot" style="background:{colour}; box-shadow: 0 0 6px {colour}80;" aria-hidden="true"></span>
  <span class="label" style="color:{colour}">{state.dominant_emotion}</span>
  <span class="bars" aria-hidden="true">
    <span
      class="bar"
      style="height:{moodH}px; background:{colour}55;"
      title="Mood {state.mood}/100"
    ></span>
    <span
      class="bar"
      style="height:{trustH}px; background:{colour}55;"
      title="Trust {state.trust}/100"
    ></span>
    <span
      class="bar"
      style="height:{arousalH}px; background:{colour}55;"
      title="Intensity {state.arousal}/100"
    ></span>
  </span>
</div>

<style>
  .ehud {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px 2px 5px;
    background: rgba(14, 14, 30, 0.65);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 99px;
    backdrop-filter: blur(8px);
    cursor: default;
    transition: opacity 200ms ease;
    user-select: none;
  }
  .ehud:hover { opacity: 0.8; }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    animation: ehud-pulse 2.8s ease-in-out infinite;
  }
  @keyframes ehud-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }

  .label {
    font-size: 10px;
    font-weight: 600;
    text-transform: capitalize;
    letter-spacing: 0.04em;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    line-height: 1;
  }

  .bars {
    display: flex;
    gap: 2px;
    align-items: flex-end;
    height: 12px;
  }
  .bar {
    width: 3px;
    border-radius: 2px;
    min-height: 3px;
    max-height: 12px;
    transition: height 700ms ease, background 500ms ease;
  }
</style>
