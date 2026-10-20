<script lang="ts">
  // The "new version available" card in the corner of the desktop app (#139).
  import Icon from './Icon.svelte';
  import { updates, installUpdate, dismissUpdate } from '$lib/stores/updates';
  import { releaseNotesLines } from '$lib/utils/updates';

  let showNotes = $state(false);

  const visible = $derived(
    ($updates.status === 'available' && !$updates.dismissed) ||
      $updates.status === 'downloading' ||
      $updates.status === 'installing',
  );
  const working = $derived($updates.status === 'downloading' || $updates.status === 'installing');
  const notes = $derived(releaseNotesLines($updates.notes, 8));
  const percent = $derived($updates.progress == null ? null : Math.round($updates.progress * 100));
</script>

{#if visible}
  <aside class="update-prompt" role="status" aria-live="polite" aria-label="Update available">
    <div class="update-head">
      <span class="update-icon"><Icon name="download" size={16} color="#c4a1ff" /></span>
      <div class="update-title">
        <strong>Janus {$updates.version} is available</strong>
        <span class="update-sub">
          {#if $updates.status === 'installing'}
            Installing, Janus will restart…
          {:else if $updates.status === 'downloading'}
            {percent == null ? 'Downloading…' : `Downloading… ${percent}%`}
          {:else}
            Updating restarts Janus and keeps your data.
          {/if}
        </span>
      </div>
    </div>

    {#if working}
      <div class="update-bar" role="progressbar" aria-label="Update download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent ?? undefined}>
        <div class="update-bar-fill" class:indeterminate={percent == null} style:width={percent == null ? undefined : `${percent}%`}></div>
      </div>
    {:else}
      {#if showNotes && notes.length}
        <ul class="update-notes">
          {#each notes as line (line)}<li>{line}</li>{/each}
        </ul>
      {/if}
      <div class="update-actions">
        <button class="update-btn primary" onclick={installUpdate}>Update now</button>
        {#if notes.length}
          <button class="update-btn" onclick={() => (showNotes = !showNotes)} aria-expanded={showNotes}>
            {showNotes ? 'Hide notes' : "What's new"}
          </button>
        {/if}
        <button class="update-btn quiet" onclick={dismissUpdate}>Later</button>
      </div>
    {/if}
  </aside>
{/if}

<style>
  .update-prompt {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 300;
    width: min(360px, calc(100vw - 32px));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border-radius: 14px;
    background: linear-gradient(175deg, #12122a, #0b0b1d);
    border: 1px solid rgba(139, 92, 246, 0.28);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5), 0 0 24px rgba(139, 92, 246, 0.12);
    animation: updateIn 260ms cubic-bezier(0.2, 0.7, 0.2, 1);
  }
  @keyframes updateIn {
    from { opacity: 0; transform: translateY(12px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .update-head { display: flex; gap: 10px; align-items: flex-start; }
  .update-icon {
    flex-shrink: 0;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: rgba(139, 92, 246, 0.14);
  }
  .update-title { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .update-title strong { font-size: var(--text-md); color: #e8e0ff; font-weight: 650; }
  .update-sub { font-size: var(--text-sm); color: var(--fg-muted); }
  .update-notes {
    margin: 0;
    padding: 0 0 0 18px;
    max-height: 160px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: var(--text-sm);
    color: var(--fg-muted);
  }
  .update-actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .update-btn {
    padding: 7px 12px;
    border-radius: 9px;
    border: 1px solid rgba(139, 92, 246, 0.25);
    background: transparent;
    color: #c9b8ff;
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
  }
  .update-btn:hover { background: rgba(139, 92, 246, 0.1); }
  .update-btn.primary { background: linear-gradient(135deg, #8b5cf6, #bf40ff); color: #fff; border-color: transparent; }
  .update-btn.primary:hover { filter: brightness(1.1); }
  .update-btn.quiet { border-color: transparent; color: var(--fg-muted); }
  .update-bar { height: 6px; border-radius: 999px; background: rgba(139, 92, 246, 0.14); overflow: hidden; }
  .update-bar-fill { height: 100%; border-radius: inherit; background: linear-gradient(90deg, #8b5cf6, #bf40ff); transition: width 200ms ease-out; }
  .update-bar-fill.indeterminate { width: 40%; animation: updateSlide 1.2s ease-in-out infinite; }
  @keyframes updateSlide {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(250%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .update-prompt { animation: none; }
    .update-bar-fill.indeterminate { animation: none; width: 100%; }
  }
</style>
