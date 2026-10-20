<script lang="ts">
  // Settings → Updates: the installed version, a manual check, and the automatic-check switch (#139).
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { updates, checkForUpdates, installUpdate } from '$lib/stores/updates';
  import { releaseNotesLines } from '$lib/utils/updates';
  import { agoLabel } from '$lib/utils/webAccount';

  let checkForUpdatesOn = $state($settings.checkForUpdates);
  let appVersion = $state<string | null>(null);

  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { checkForUpdates: checkForUpdatesOn };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update((prev) => ({ ...prev, ...snapshot }));
    }, 50);
  });

  onMount(async () => {
    try {
      const ipc = await import('$lib/services/ipc');
      appVersion = (await ipc.getAppInfo()).version;
    } catch (err) {
      console.error('[settings] Failed to read app info:', err);
    }
  });

  const busy = $derived(
    $updates.status === 'checking' || $updates.status === 'downloading' || $updates.status === 'installing',
  );
  const notes = $derived(releaseNotesLines($updates.notes, 10));
  const percent = $derived($updates.progress == null ? null : Math.round($updates.progress * 100));
  const lastChecked = $derived($updates.lastChecked ? agoLabel($updates.lastChecked) : null);
</script>

<div class="panel-heading animate-fade-in-up stagger-2">
  <span class="panel-heading-title">Updates</span>
  <span class="panel-heading-desc">Keep Janus current. Installing an update restarts the app and keeps all your data.</span>
</div>

<section class="settings-section settings-section-bounded animate-fade-in-up stagger-2">
  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Janus {appVersion ?? ''}</span>
      <span class="setting-desc" aria-live="polite">
        {#if $updates.status === 'checking'}
          Checking for updates…
        {:else if $updates.status === 'available' || $updates.status === 'downloading' || $updates.status === 'installing'}
          Version {$updates.version} is available.
        {:else if $updates.status === 'uptodate'}
          You are on the latest version{lastChecked ? ` (checked ${lastChecked})` : ''}.
        {:else if $updates.status === 'error'}
          {$updates.error}
        {:else}
          Not checked yet this session.
        {/if}
      </span>
    </div>
    {#if $updates.status === 'available'}
      <button class="settings-btn primary update-action" onclick={installUpdate}>
        <Icon name="download" size={13} color="currentColor" /> Update now
      </button>
    {:else}
      <button class="settings-btn outline update-action" disabled={busy} onclick={() => checkForUpdates(true)}>
        <Icon name="refresh-cw" size={13} color="currentColor" /> Check now
      </button>
    {/if}
  </div>

  {#if $updates.status === 'downloading' || $updates.status === 'installing'}
    <div class="update-progress" role="progressbar" aria-label="Update download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent ?? undefined}>
      <div class="update-progress-bar" class:indeterminate={percent == null} style:width={percent == null ? undefined : `${percent}%`}></div>
    </div>
    <span class="setting-desc">{$updates.status === 'installing' ? 'Installing, Janus will restart…' : percent == null ? 'Downloading…' : `Downloading… ${percent}%`}</span>
  {/if}

  {#if $updates.status === 'available' && notes.length}
    <div class="setting-label">
      <span class="setting-name">What is new in {$updates.version}</span>
      <ul class="update-notes">
        {#each notes as line (line)}<li>{line}</li>{/each}
      </ul>
    </div>
  {/if}

  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Check for updates automatically</span>
      <span class="setting-desc">At launch and every few hours. This asks GitHub for the latest release and sends nothing about you or your data.</span>
    </div>
    <button class="toggle-switch" class:on={checkForUpdatesOn} role="switch" aria-checked={checkForUpdatesOn} aria-label="Check for updates automatically" onclick={() => (checkForUpdatesOn = !checkForUpdatesOn)}>
      <span class="toggle-knob"></span>
    </button>
  </div>
</section>

<style>
  /* The shared outline button stretches to fill its row; these sit at their own width. */
  button.update-action { flex: 0 0 auto; white-space: nowrap; }
  .update-progress {
    height: 6px;
    border-radius: 999px;
    background: rgba(139, 92, 246, 0.14);
    overflow: hidden;
  }
  .update-progress-bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #8b5cf6, #bf40ff);
    transition: width 200ms ease-out;
  }
  .update-progress-bar.indeterminate {
    width: 40%;
    animation: updateSlide 1.2s ease-in-out infinite;
  }
  @keyframes updateSlide {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(250%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .update-progress-bar.indeterminate { animation: none; width: 100%; }
  }
  .update-notes {
    margin: 6px 0 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: var(--text-sm);
    color: var(--fg-muted);
  }
</style>
