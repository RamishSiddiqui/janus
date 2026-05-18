<script lang="ts">
  import type { CharacterState } from '$lib/services/ipc';

  let { state: emotionState }: { state: CharacterState } = $props();

  // â”€â”€ Emotion colour palette â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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
    welcoming:  '#5B8AF6',
  };

  let colour = $derived(
    EMOTION_COLOURS[emotionState.dominant_emotion.toLowerCase()] ?? '#8B5CF6'
  );

  // Bar heights for the pill (mini) â€“ [3px â€¦ 12px]
  let moodH    = $derived(3 + emotionState.mood    * 0.09);
  let trustH   = $derived(3 + emotionState.trust   * 0.09);
  let arousalH = $derived(3 + emotionState.arousal * 0.09);


  // Hover card state
  let isHovered = $state(false);
  let isCardVisible = $state(false);
  let isScanDone = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function onEnter() {
    isHovered = true;
    isCardVisible = true;
    isScanDone = false;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { isScanDone = true; }, 40);
  }

  function onLeave() {
    isHovered = false;
    isScanDone = false;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { isCardVisible = false; }, 220);
  }

  // Labels & descriptors for each metric
  const MOOD_LABEL = (v: number) =>
    v >= 80 ? 'Elevated' : v >= 55 ? 'Positive' : v >= 40 ? 'Neutral' : v >= 20 ? 'Low' : 'Depressed';
  const TRUST_LABEL = (v: number) =>
    v >= 80 ? 'Deep trust' : v >= 55 ? 'Open' : v >= 40 ? 'Cautious' : v >= 20 ? 'Wary' : 'Closed';
  const AROUSAL_LABEL = (v: number) =>
    v >= 80 ? 'Intense' : v >= 55 ? 'Engaged' : v >= 40 ? 'Steady' : v >= 20 ? 'Subdued' : 'Dormant';
</script>

<!-- Wrapper keeps the card anchored relative to the pill -->
<div
  class="ehud-root"
  onmouseenter={onEnter}
  onmouseleave={onLeave}
  role="status"
  aria-label="Character emotion: {emotionState.dominant_emotion}"
>

  <!-- pill -->
  <div
    class="ehud"
    class:active={isHovered}
    style="--c:{colour}; --c44:{colour}44; --c18:{colour}18; --c0a:{colour}0a;"
  >
    <span class="ehud-accent" aria-hidden="true">
      <span class="ehud-stripe"></span>
      <span class="ehud-ring"></span>
    </span>
    <span class="ehud-label">{emotionState.dominant_emotion}</span>
    <span class="ehud-eq" aria-hidden="true">
      <span class="ehud-eq-bar" style="--bh:{moodH * 1.1}px; --delay:0ms"></span>
      <span class="ehud-eq-bar" style="--bh:{arousalH * 0.9}px; --delay:120ms"></span>
      <span class="ehud-eq-bar" style="--bh:{trustH}px; --delay:60ms"></span>
      <span class="ehud-eq-bar" style="--bh:{arousalH * 1.1}px; --delay:180ms"></span>
      <span class="ehud-eq-bar" style="--bh:{moodH * 0.85}px; --delay:90ms"></span>
    </span>
  </div>

  <!-- â”€â”€ HOVER CARD â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ -->
  {#if isCardVisible}
    <div
      class="ecard"
      class:visible={isHovered}
      class:scan={isScanDone}
      style="--c:{colour}; --c55:{colour}55; --c22:{colour}22; --c11:{colour}11;"
      aria-hidden="true"
    >
      <!-- scan-line effect -->
      <div class="scan-line"></div>

      <!-- header -->
      <div class="ec-header">
        <span class="ec-dot" style="background:{colour};box-shadow:0 0 8px {colour};"></span>
        <span class="ec-emotion" style="color:{colour}">{emotionState.dominant_emotion}</span>
        <span class="ec-label">emotional state</span>
        <span class="ec-corner tl"></span>
        <span class="ec-corner tr"></span>
      </div>

      <!-- metric rows -->
      <div class="ec-metrics">

        <!-- Mood -->
        <div class="ec-metric">
          <div class="ecm-meta">
            <span class="ecm-name">Mood</span>
            <span class="ecm-desc">{MOOD_LABEL(emotionState.mood)}</span>
            <span class="ecm-val" style="color:{colour}">{emotionState.mood}<span class="ecm-unit">/100</span></span>
          </div>
          <div class="ecm-track">
            <div
              class="ecm-fill"
              style="width:{emotionState.mood}%; background: linear-gradient(90deg, {colour}44, {colour});"
            ></div>
            <!-- tick marks -->
            <span class="ecm-tick" style="left:25%"></span>
            <span class="ecm-tick" style="left:50%"></span>
            <span class="ecm-tick" style="left:75%"></span>
          </div>
        </div>

        <!-- Trust -->
        <div class="ec-metric">
          <div class="ecm-meta">
            <span class="ecm-name">Trust</span>
            <span class="ecm-desc">{TRUST_LABEL(emotionState.trust)}</span>
            <span class="ecm-val" style="color:{colour}">{emotionState.trust}<span class="ecm-unit">/100</span></span>
          </div>
          <div class="ecm-track">
            <div
              class="ecm-fill"
              style="width:{emotionState.trust}%; background: linear-gradient(90deg, {colour}44, {colour});"
            ></div>
            <span class="ecm-tick" style="left:25%"></span>
            <span class="ecm-tick" style="left:50%"></span>
            <span class="ecm-tick" style="left:75%"></span>
          </div>
        </div>

        <!-- Arousal/Intensity -->
        <div class="ec-metric">
          <div class="ecm-meta">
            <span class="ecm-name">Intensity</span>
            <span class="ecm-desc">{AROUSAL_LABEL(emotionState.arousal)}</span>
            <span class="ecm-val" style="color:{colour}">{emotionState.arousal}<span class="ecm-unit">/100</span></span>
          </div>
          <div class="ecm-track">
            <div
              class="ecm-fill"
              style="width:{emotionState.arousal}%; background: linear-gradient(90deg, {colour}44, {colour});"
            ></div>
            <span class="ecm-tick" style="left:25%"></span>
            <span class="ecm-tick" style="left:50%"></span>
            <span class="ecm-tick" style="left:75%"></span>
          </div>
        </div>

      </div>

      <!-- summary prose -->
      {#if emotionState.state_summary}
        <div class="ec-summary">
          <span class="ec-summary-icon">â—ˆ</span>
          <p class="ec-summary-text">{emotionState.state_summary}</p>
        </div>
      {/if}

      <!-- bottom corners -->
      <span class="ec-corner bl"></span>
      <span class="ec-corner br"></span>
    </div>
  {/if}

</div>

<style>
  /* â”€â”€ Root anchor â”€â”€ */
  .ehud-root {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  /* Pill / chip */
  .ehud {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0;
    height: 22px;
    padding: 0 9px 0 0;
    background: rgba(10, 10, 24, 0.82);
    border: 1px solid var(--c18, rgba(255,255,255,0.06));
    border-left: none;
    border-radius: 0 99px 99px 0;
    backdrop-filter: blur(12px) saturate(140%);
    cursor: default;
    user-select: none;
    overflow: hidden;
    transition: border-color 240ms ease, background 240ms ease, box-shadow 240ms ease;
    box-shadow: 0 2px 12px var(--c0a, transparent), inset 0 1px 0 rgba(255,255,255,0.04);
  }
  .ehud.active {
    border-color: var(--c44, rgba(255,255,255,0.15));
    background: rgba(12,12,28,0.95);
    box-shadow: 0 0 18px var(--c18, transparent), inset 0 1px 0 rgba(255,255,255,0.06);
  }
  .ehud::before {
    content: '';
    position: absolute; inset: 0;
    background: radial-gradient(ellipse 60% 100% at 20% 50%, var(--c0a, transparent) 0%, transparent 70%);
    pointer-events: none;
  }

  .ehud-accent {
    position: relative;
    display: flex; align-items: center; justify-content: center;
    width: 18px; height: 100%; flex-shrink: 0;
  }
  .ehud-stripe {
    position: absolute;
    left: 0; top: 0; bottom: 0; width: 2px;
    background: var(--c, #8B5CF6);
    box-shadow: 0 0 6px var(--c, #8B5CF6), 0 0 12px var(--c44, transparent);
    border-radius: 0 1px 1px 0;
  }
  .ehud-ring {
    width: 5px; height: 5px;
    border-radius: 50%;
    background: var(--c, #8B5CF6);
    box-shadow: 0 0 5px var(--c, #8B5CF6);
    position: relative;
    animation: ring-pulse 2.6s ease-in-out infinite;
  }
  .ehud-ring::after {
    content: '';
    position: absolute; inset: -3px;
    border-radius: 50%;
    border: 1px solid var(--c, #8B5CF6);
    opacity: 0;
    animation: ring-expand 2.6s ease-in-out infinite;
  }
  @keyframes ring-pulse {
    0%,100% { opacity: 1;    transform: scale(1); }
    50%      { opacity: 0.4; transform: scale(0.8); }
  }
  @keyframes ring-expand {
    0%   { transform: scale(1);   opacity: 0.6; }
    70%  { transform: scale(2.8); opacity: 0; }
    100% { transform: scale(2.8); opacity: 0; }
  }

  .ehud-label {
    font-size: 9.5px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.1em;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    color: var(--c, #8B5CF6); line-height: 1;
    margin: 0 6px 0 2px;
    opacity: 0.9;
    filter: drop-shadow(0 0 4px var(--c44, transparent));
  }

  .ehud-eq {
    display: flex; gap: 2px;
    align-items: flex-end; height: 12px; flex-shrink: 0;
  }
  .ehud-eq-bar {
    width: 2px; border-radius: 1px;
    background: var(--c, #8B5CF6); opacity: 0.55;
    height: var(--bh, 6px); min-height: 2px; max-height: 12px;
    transition: height 600ms ease;
    animation: eq-breathe 2.4s ease-in-out infinite;
    animation-delay: var(--delay, 0ms);
  }
  @keyframes eq-breathe {
    0%,100% { opacity: 0.55; transform: scaleY(1); }
    50%      { opacity: 0.9;  transform: scaleY(1.2); }
  }

  /* â”€â”€ Hover card â”€â”€ */
  .ecard {
    position: absolute;
    bottom: calc(100% + 10px);
    left: 0;
    width: 248px;
    background: rgba(8, 8, 22, 0.96);
    border: 1px solid var(--c22);
    border-radius: 10px;
    backdrop-filter: blur(24px) saturate(150%);
    box-shadow:
      0 0 0 1px rgba(255,255,255,0.04),
      0 20px 60px rgba(0,0,0,0.7),
      0 0 40px var(--c11);
    overflow: hidden;
    z-index: 200;

    /* enter/exit */
    opacity: 0;
    transform: translateY(6px) scale(0.97);
    transition:
      opacity 200ms cubic-bezier(.22,.68,0,1.2),
      transform 200ms cubic-bezier(.22,.68,0,1.2);
    pointer-events: none;
  }
  .ecard.visible {
    opacity: 1;
    transform: translateY(0) scale(1);
    pointer-events: auto;
  }

  /* Scan-line sweep on open */
  .scan-line {
    position: absolute;
    top: -4px; left: 0; right: 0;
    height: 2px;
    background: linear-gradient(90deg, transparent 10%, var(--c) 50%, transparent 90%);
    opacity: 0;
    transform: translateY(0);
    pointer-events: none;
    z-index: 10;
  }
  .ecard.scan .scan-line {
    animation: scan-sweep 500ms ease-out forwards;
  }
  @keyframes scan-sweep {
    0%   { opacity: 0.9; transform: translateY(0); }
    100% { opacity: 0;   transform: translateY(160px); }
  }

  /* Corner brackets */
  .ec-corner {
    position: absolute;
    width: 8px; height: 8px;
    border-color: var(--c);
    border-style: solid;
    opacity: 0.5;
  }
  .ec-corner.tl { top: 4px; left: 4px;  border-width: 1px 0 0 1px; }
  .ec-corner.tr { top: 4px; right: 4px; border-width: 1px 1px 0 0; }
  .ec-corner.bl { bottom: 4px; left: 4px;  border-width: 0 0 1px 1px; }
  .ec-corner.br { bottom: 4px; right: 4px; border-width: 0 1px 1px 0; }

  /* Header */
  .ec-header {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 11px 14px 9px;
    border-bottom: 1px solid var(--c22);
    position: relative;
  }
  .ec-dot {
    width: 7px; height: 7px;
    border-radius: 50%; flex-shrink: 0;
    animation: ehud-pulse 2.8s ease-in-out infinite;
  }
  .ec-emotion {
    font-size: 11px; font-weight: 700;
    text-transform: capitalize; letter-spacing: 0.06em;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
  }
  .ec-label {
    font-size: 9px; color: #3a3a58; letter-spacing: 0.1em;
    text-transform: uppercase; font-family: var(--font-mono, monospace);
    margin-left: auto;
  }

  /* Metrics */
  .ec-metrics {
    padding: 10px 14px 6px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .ec-metric { display: flex; flex-direction: column; gap: 4px; }

  .ecm-meta {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .ecm-name {
    font-size: 9px; font-weight: 700; text-transform: uppercase;
    letter-spacing: 0.12em; color: #5a5a7a;
    font-family: var(--font-mono, monospace);
    width: 52px; flex-shrink: 0;
  }
  .ecm-desc {
    font-size: 10px; color: #7a7a9a;
    font-family: var(--font-mono, monospace);
    flex: 1;
  }
  .ecm-val {
    font-size: 11px; font-weight: 700;
    font-family: var(--font-mono, monospace);
    letter-spacing: 0.02em;
  }
  .ecm-unit { font-size: 8px; opacity: 0.5; margin-left: 1px; }

  /* Progress track */
  .ecm-track {
    position: relative;
    height: 3px;
    background: rgba(255,255,255,0.04);
    border-radius: 99px;
    overflow: visible;
  }
  .ecm-fill {
    height: 100%;
    border-radius: 99px;
    transition: width 600ms cubic-bezier(.22,.68,0,1.1);
    position: relative;
  }
  /* Glow tip on fill */
  .ecm-fill::after {
    content: '';
    position: absolute;
    right: -1px; top: -2px;
    width: 4px; height: 7px;
    border-radius: 2px;
    background: var(--c);
    box-shadow: 0 0 8px var(--c);
  }
  /* Tick marks */
  .ecm-tick {
    position: absolute;
    top: -2px;
    width: 1px; height: 7px;
    background: rgba(255,255,255,0.06);
    transform: translateX(-50%);
    pointer-events: none;
  }

  /* Summary prose */
  .ec-summary {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 8px 14px 12px;
    border-top: 1px solid var(--c22);
    margin-top: 2px;
  }
  .ec-summary-icon {
    color: var(--c);
    opacity: 0.5;
    font-size: 10px;
    flex-shrink: 0;
    margin-top: 1px;
  }
  .ec-summary-text {
    font-size: 10px;
    line-height: 1.55;
    color: #5a5a7a;
    font-family: var(--font-body, system-ui, sans-serif);
    font-style: italic;
    margin: 0;
  }
</style>
