<script lang="ts">
  import Icon from './Icon.svelte';
  import JanusLoader from './JanusLoader.svelte';

  let { reasoning = '', isThinking = false, startedAt }: {
    /** Accumulated chain-of-thought text — may still be growing if isThinking. */
    reasoning?: string;
    /** True while reasoning is actively streaming and no real reply has
     *  arrived yet. False (or undefined) for finished/reloaded messages. */
    isThinking?: boolean;
    /** Wall-clock ms when reasoning started — used to compute the live
     *  elapsed counter and the final "Thought for Ns" label. Absent for
     *  messages reloaded from the database (no live timing available). */
    startedAt?: number;
  } = $props();

  let expanded = $state(false);

  // ── Live elapsed timer while thinking ──
  let liveElapsedMs = $state(0);
  $effect(() => {
    if (!isThinking || !startedAt) return;
    const handle = setInterval(() => { liveElapsedMs = Date.now() - startedAt; }, 250);
    return () => clearInterval(handle);
  });

  // ── Capture the final duration the instant thinking ends ──
  let durationMs = $state<number | null>(null);
  let wasThinking = false;
  $effect(() => {
    if (wasThinking && !isThinking && startedAt && durationMs === null) {
      durationMs = Date.now() - startedAt;
    }
    wasThinking = !!isThinking;
  });

  let headerLabel = $derived.by(() => {
    if (isThinking) {
      const secs = Math.floor(liveElapsedMs / 1000);
      return secs > 0 ? `Thinking · ${secs}s` : 'Thinking';
    }
    if (durationMs !== null) {
      return `Thought for ${Math.max(1, Math.round(durationMs / 1000))}s`;
    }
    return 'Thoughts';
  });
</script>

{#if reasoning || isThinking}
  <div class="thinking-block" class:is-thinking={isThinking}>
    <button
      class="thinking-header"
      type="button"
      onclick={() => expanded = !expanded}
      aria-expanded={expanded}
      aria-controls="thinking-body-{startedAt ?? 'static'}"
    >
      <span class="thinking-icon" aria-hidden="true">
        {#if isThinking}
          <JanusLoader size={13} label="" />
        {:else}
          <Icon name="brain" size={12} color="var(--fg-muted)" />
        {/if}
      </span>
      <span class="thinking-label">{headerLabel}</span>
      <span class="thinking-chevron" class:expanded>
        <Icon name="chevron-down" size={11} color="var(--fg-muted)" />
      </span>
    </button>
    <div class="thinking-body-wrap" class:expanded>
      <div class="thinking-body-inner">
        <div class="thinking-body" id="thinking-body-{startedAt ?? 'static'}">{reasoning}</div>
      </div>
    </div>
  </div>
{/if}

<style>
  .thinking-block {
    margin-bottom: 10px;
    width: fit-content;
    max-width: 100%;
  }

  .thinking-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 999px;
    background: rgba(139, 92, 246, 0.05);
    border: 1px solid rgba(139, 92, 246, 0.12);
    cursor: pointer;
    transition: background 180ms ease, border-color 180ms ease;
    max-width: 100%;
  }
  .thinking-header:hover {
    background: rgba(139, 92, 246, 0.09);
    border-color: rgba(139, 92, 246, 0.22);
  }
  .thinking-header:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px rgba(139, 92, 246, 0.25);
  }
  .is-thinking .thinking-header {
    background: rgba(139, 92, 246, 0.08);
    border-color: rgba(139, 92, 246, 0.2);
  }

  .thinking-icon { display: flex; flex-shrink: 0; }

  .thinking-label {
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    letter-spacing: 0.4px;
    color: rgba(196, 161, 255, 0.75);
    white-space: nowrap;
  }

  .thinking-chevron {
    display: flex; flex-shrink: 0;
    transition: transform 220ms cubic-bezier(0.16, 1, 0.3, 1);
    opacity: 0.7;
  }
  .thinking-chevron.expanded { transform: rotate(180deg); }

  /* Grid-rows accordion — animates height without measuring content. */
  .thinking-body-wrap {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows 240ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .thinking-body-wrap.expanded { grid-template-rows: 1fr; }
  .thinking-body-inner { overflow: hidden; min-height: 0; }

  .thinking-body {
    margin-top: 6px;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgba(139, 92, 246, 0.03);
    border: 1px solid rgba(139, 92, 246, 0.08);
    border-left: 2px solid rgba(139, 92, 246, 0.25);
    font-size: 12.5px;
    font-style: italic;
    line-height: 1.6;
    color: rgba(158, 158, 190, 0.85);
    white-space: pre-wrap;
    max-height: 240px;
    overflow-y: auto;
  }
  .thinking-body::-webkit-scrollbar { width: 3px; }
  .thinking-body::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }

  @media (prefers-reduced-motion: reduce) {
    .thinking-body-wrap, .thinking-chevron {
      transition: none;
      animation: none;
    }
  }
</style>
