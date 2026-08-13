<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { activeConversationId } from '$lib/stores/chat';
  import { substituteUserMacro } from '$lib/utils/personaMacros';
  import type { SceneState } from '$lib/services/ipc';

  let { sceneState = null, expanded = false }: { sceneState: SceneState | null; expanded?: boolean } = $props();

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let isExpanded = $state(false);

  // Initialize from prop
  $effect(() => { isExpanded = expanded; });

  // Scene extraction is told to use the literal "{{user}}" token to refer
  // to the player character in `characters_present` — resolve it to the
  // active persona's name (or the generic "User" fallback) for display,
  // same convention as the backend prompt's macro substitution.
  let personaName: string | null = $state(null);
  $effect(() => {
    const convId = $activeConversationId;
    if (convId && isTauri) {
      resolvePersonaName(convId);
    } else {
      personaName = null;
    }
  });

  async function resolvePersonaName(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const conv = await ipc.getConversation(convId);
      const personaId = (conv as unknown as { persona_id: string | null }).persona_id;
      personaName = personaId ? (await ipc.getPersona(personaId)).name : null;
    } catch {
      personaName = null;
    }
  }

  function displayCharName(name: string): string {
    return substituteUserMacro(name, personaName);
  }

  function toggle() {
    isExpanded = !isExpanded;
  }

  function getWeatherIcon(weather: string): string {
    const map: Record<string, string> = {
      clear: 'sun', cloudy: 'cloud', raining: 'cloud-rain',
      storming: 'cloud-lightning', snowing: 'cloud-snow',
      foggy: 'eye', windy: 'wind',
    };
    return map[weather.toLowerCase()] || 'cloud';
  }

  function getTimeIcon(time: string): string {
    const t = time.toLowerCase();
    if (['morning', 'dawn', 'midday', 'afternoon'].includes(t)) return 'sun';
    if (['evening', 'night', 'late_night'].includes(t)) return 'moon';
    return 'clock';
  }

  function getMoodColor(mood: string): string {
    const map: Record<string, string> = {
      tense: '#ef4444', dangerous: '#ef4444',
      calm: '#60a5fa', neutral: '#60a5fa',
      romantic: '#f472b6', mysterious: '#a78bfa',
      joyful: '#fbbf24', melancholic: '#9ca3af',
    };
    return map[mood.toLowerCase()] || '#60a5fa';
  }

  function formatTimePeriod(time: string): string {
    return time.replace(/_/g, ' ').replace(/^\w/, c => c.toUpperCase());
  }

  function formatWeather(weather: string): string {
    return weather.replace(/_/g, ' ').replace(/^\w/, c => c.toUpperCase());
  }
</script>

{#if sceneState}
  <div class="ssb" class:expanded={isExpanded}>
    <!-- Collapsed summary row (always visible) -->
    <button class="ssb-summary" onclick={toggle} aria-expanded={isExpanded} aria-label="Toggle scene details">
      <div class="ssb-chips">
        <span class="ssb-chip">
          <Icon name="map-pin" size={12} color="var(--accent-primary)" />
          <span class="ssb-chip-text">{sceneState.location_name || 'Unknown'}</span>
        </span>

        <span class="ssb-divider" aria-hidden="true">·</span>

        <span class="ssb-chip">
          <Icon name={getTimeIcon(sceneState.time_period)} size={12} color="var(--fg-muted)" />
          <span class="ssb-chip-text ssb-chip-muted">{formatTimePeriod(sceneState.time_period)}</span>
        </span>

        <span class="ssb-divider" aria-hidden="true">·</span>

        <span class="ssb-chip">
          <Icon name={getWeatherIcon(sceneState.weather)} size={12} color="var(--fg-muted)" />
          <span class="ssb-chip-text ssb-chip-muted">{formatWeather(sceneState.weather)}</span>
        </span>

        <span class="ssb-divider" aria-hidden="true">·</span>

        <span class="ssb-chip">
          <Icon name="users" size={12} color="var(--fg-muted)" />
          <span class="ssb-chip-text ssb-chip-muted">{sceneState.characters_present.length} character{sceneState.characters_present.length !== 1 ? 's' : ''}</span>
        </span>

        {#if sceneState.scene_mood}
          <span class="ssb-divider" aria-hidden="true">·</span>
          <span class="ssb-mood-dot" style="background:{getMoodColor(sceneState.scene_mood)}; box-shadow: 0 0 6px {getMoodColor(sceneState.scene_mood)}40;" title="Mood: {sceneState.scene_mood}"></span>
        {/if}
      </div>

      <span class="ssb-toggle" class:flipped={isExpanded}>
        <Icon name="chevron-down" size={14} color="var(--fg-muted)" />
      </span>
    </button>

    <!-- Expanded details -->
    {#if isExpanded}
      <div class="ssb-details">
        {#if sceneState.location_description}
          <div class="ssb-detail-row">
            <span class="ssb-detail-label">Location</span>
            <span class="ssb-detail-value">{sceneState.location_description}</span>
          </div>
        {/if}

        {#if sceneState.characters_present.length > 0}
          <div class="ssb-detail-row">
            <span class="ssb-detail-label">Present</span>
            <div class="ssb-char-list">
              {#each sceneState.characters_present as char}
                <span class="ssb-char-tag">{displayCharName(char)}</span>
              {/each}
            </div>
          </div>
        {/if}

        {#if sceneState.ambient_details}
          <div class="ssb-detail-row">
            <span class="ssb-detail-label">Ambience</span>
            <span class="ssb-detail-value ssb-detail-italic">{sceneState.ambient_details}</span>
          </div>
        {/if}

        {#if sceneState.scene_mood}
          <div class="ssb-detail-row">
            <span class="ssb-detail-label">Mood</span>
            <span class="ssb-detail-value">
              <span class="ssb-mood-dot-inline" style="background:{getMoodColor(sceneState.scene_mood)}; box-shadow: 0 0 6px {getMoodColor(sceneState.scene_mood)}40;"></span>
              {sceneState.scene_mood.replace(/^\w/, c => c.toUpperCase())}
            </span>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* ── Scene State Bar ── */
  .ssb {
    background: linear-gradient(180deg, rgba(10, 10, 26, 0.88), rgba(8, 8, 22, 0.82));
    border-bottom: 1px solid var(--border-subtle, rgba(139, 92, 246, 0.08));
    backdrop-filter: blur(16px) saturate(140%);
    -webkit-backdrop-filter: blur(16px) saturate(140%);
    flex-shrink: 0;
    overflow: hidden;

    /* Entrance animation */
    animation: ssbSlideIn 280ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes ssbSlideIn {
    from {
      opacity: 0;
      transform: translateY(-8px);
      max-height: 0;
    }
    to {
      opacity: 1;
      transform: translateY(0);
      max-height: 300px;
    }
  }

  /* ── Summary row (collapsed) ── */
  .ssb-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 7px 24px;
    background: none;
    border: none;
    cursor: pointer;
    transition: background var(--duration-fast, 120ms) var(--ease-out, ease-out);
  }

  .ssb-summary:hover {
    background: rgba(139, 92, 246, 0.04);
  }

  .ssb-chips {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: nowrap;
    overflow: hidden;
    min-width: 0;
  }

  .ssb-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }

  .ssb-chip-text {
    font-size: var(--text-xs, 11px);
    font-family: var(--font-body, system-ui, sans-serif);
    font-weight: 600;
    color: var(--fg-primary, #e8e0ff);
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .ssb-chip-muted {
    color: var(--fg-muted, #5a5a7a);
    font-weight: 500;
  }

  .ssb-divider {
    color: rgba(139, 92, 246, 0.2);
    font-size: 10px;
    flex-shrink: 0;
    user-select: none;
  }

  .ssb-mood-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    animation: moodPulse 2.4s ease-in-out infinite;
  }

  @keyframes moodPulse {
    0%, 100% { opacity: 0.7; transform: scale(1); }
    50% { opacity: 1; transform: scale(1.15); }
  }

  .ssb-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    border-radius: var(--rounded-sm, 4px);
    transition: transform var(--duration-fast, 120ms) var(--ease-out, ease-out),
                background var(--duration-fast, 120ms) var(--ease-out, ease-out);
  }

  .ssb-toggle.flipped {
    transform: rotate(180deg);
  }

  .ssb-summary:hover .ssb-toggle {
    background: rgba(139, 92, 246, 0.08);
  }

  /* ── Expanded details ── */
  .ssb-details {
    padding: 4px 24px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid rgba(139, 92, 246, 0.06);
    animation: detailsFadeIn 200ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes detailsFadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .ssb-detail-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .ssb-detail-label {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgba(139, 92, 246, 0.45);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    width: 64px;
    flex-shrink: 0;
  }

  .ssb-detail-value {
    font-size: var(--text-xs, 11px);
    font-family: var(--font-body, system-ui, sans-serif);
    color: var(--fg-secondary, #a0a0b8);
    line-height: 1.5;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .ssb-detail-italic {
    font-style: italic;
    color: var(--fg-muted, #5a5a7a);
  }

  /* Character tags */
  .ssb-char-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .ssb-char-tag {
    display: inline-block;
    padding: 2px 8px;
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-body, system-ui, sans-serif);
    color: rgba(196, 161, 255, 0.85);
    background: rgba(139, 92, 246, 0.1);
    border: 1px solid rgba(139, 92, 246, 0.15);
    border-radius: var(--rounded-sm, 4px);
    letter-spacing: 0.01em;
    transition: all var(--duration-fast, 120ms) var(--ease-out, ease-out);
  }

  .ssb-char-tag:hover {
    background: rgba(139, 92, 246, 0.18);
    border-color: rgba(139, 92, 246, 0.3);
  }

  .ssb-mood-dot-inline {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    display: inline-block;
  }
</style>
