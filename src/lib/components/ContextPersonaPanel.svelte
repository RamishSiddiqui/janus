<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { onDestroy } from 'svelte';
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { loadFileAsBlobUrl, revokeIfSet } from '$lib/utils/blobUrl';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    conversationId = null,
    wide = false,
  }: {
    conversationId?: string | null;
    /** Renders persona rows in a responsive grid instead of a single
     *  stacked column — used when this panel fills the full chat area (see
     *  ChatExplorerView.svelte) rather than a narrow header popover. */
    wide?: boolean;
  } = $props();

  interface PersonaEntry {
    id: string;
    name: string;
    avatar_path: string | null;
  }

  let personas: PersonaEntry[] = $state([]);
  let selectedId: string | null = $state(null);
  let isLoading = $state(false);
  let isSwitching = $state(false);

  let avatarUrls: Record<string, string> = $state({});
  const avatarPathLoaded: Record<string, string> = {};

  $effect(() => {
    const _conv = conversationId;
    if (_conv && isTauri) {
      loadPersonas(_conv);
    } else {
      personas = [];
      selectedId = null;
    }
  });

  $effect(() => {
    for (const p of personas) {
      const path = p.avatar_path;
      if (path && avatarPathLoaded[p.id] !== path) {
        avatarPathLoaded[p.id] = path;
        loadFileAsBlobUrl(path).then(url => {
          revokeIfSet(avatarUrls[p.id]);
          avatarUrls = { ...avatarUrls, [p.id]: url };
        }).catch(() => { /* falls back to initial-circle placeholder */ });
      }
    }
  });

  onDestroy(() => {
    for (const url of Object.values(avatarUrls)) revokeIfSet(url);
  });

  async function loadPersonas(convId: string) {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const [list, conv] = await Promise.all([
        ipc.listPersonas(),
        ipc.getConversation(convId),
      ]);
      personas = list.map(p => ({ id: p.id, name: p.name, avatar_path: p.avatar_path }));
      selectedId = (conv as unknown as { persona_id: string | null }).persona_id ?? null;
    } catch (err) {
      console.error('Failed to load personas:', err);
      personas = [];
    }
    isLoading = false;
  }

  async function selectPersona(id: string | null) {
    if (!isTauri || !conversationId || isSwitching) return;
    isSwitching = true;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.setConversationPersona(conversationId, id);
      selectedId = id;
      const name = id ? personas.find(p => p.id === id)?.name : null;
      success(name ? `Now playing as ${name}` : 'Persona cleared');
    } catch {
      toastError('Failed to set persona');
    }
    isSwitching = false;
  }
</script>

<section class="ctx-section" aria-labelledby="persona-title">
  <div class="ctx-section-header">
    <span class="ctx-section-title" id="persona-title">PERSONA</span>
    <button class="lore-add-btn" title="Manage personas" aria-label="Manage personas" onclick={() => goto('/personas')}>
      <Icon name="settings" size={13} color="var(--accent-primary)" />
    </button>
  </div>

  {#if isLoading}
    <div class="lore-loading">
      <span class="loading-dot"></span>
      <span class="loading-dot d2"></span>
      <span class="loading-dot d3"></span>
    </div>
  {:else if personas.length === 0}
    <div class="lore-empty">
      <Icon name="user" size={16} color="var(--fg-muted)" />
      <span>No personas yet — create one to play a specific role</span>
    </div>
  {:else}
    <div class="cards-grid" class:wide>
    <button
      class="persona-row"
      class:active={selectedId === null}
      onclick={() => selectPersona(null)}
      disabled={isSwitching}
    >
      <div class="persona-avatar none" aria-hidden="true">
        <Icon name="x" size={12} color="var(--fg-muted)" />
      </div>
      <span class="persona-name">None</span>
      {#if selectedId === null}<Icon name="check" size={13} color="var(--accent-primary)" />{/if}
    </button>

    {#each personas as persona (persona.id)}
      <button
        class="persona-row"
        class:active={selectedId === persona.id}
        onclick={() => selectPersona(persona.id)}
        disabled={isSwitching}
      >
        <div class="persona-avatar" aria-hidden="true">
          {#if avatarUrls[persona.id]}
            <img src={avatarUrls[persona.id]} alt="" class="persona-avatar-img" />
          {:else}
            <span class="persona-avatar-initial">{persona.name.charAt(0)}</span>
          {/if}
        </div>
        <span class="persona-name">{persona.name}</span>
        {#if selectedId === persona.id}<Icon name="check" size={13} color="var(--accent-primary)" />{/if}
      </button>
    {/each}
    </div>
  {/if}
</section>

<style>
  .ctx-section { display: flex; flex-direction: column; gap: 10px; }
  .ctx-section-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-section-title {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.5px;
  }
  .lore-add-btn {
    background: none; border: 1px solid rgba(139,92,246,0.12);
    border-radius: 8px; padding: 4px; display: flex; cursor: pointer;
    transition: all 150ms;
  }
  .lore-add-btn:hover { border-color: rgba(139,92,246,0.3); background: rgba(139,92,246,0.06); }

  .lore-empty { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: #4a4a6a; font-size: var(--text-sm); }
  .lore-loading { display: flex; gap: 4px; padding: 14px; justify-content: center; }
  .loading-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: #5a5a7a; animation: dotPulse 1.2s ease-in-out infinite;
  }
  .loading-dot.d2 { animation-delay: 150ms; }
  .loading-dot.d3 { animation-delay: 300ms; }
  @keyframes dotPulse { 0%,100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1); } }

  /* `display: contents` by default keeps existing stacked-column behavior
     in the narrow popover; `.wide` (full chat-area explorer view) switches
     to an actual grid of persona rows. */
  .cards-grid { display: contents; }
  .cards-grid.wide {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
    align-items: start;
  }

  .persona-row {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: clamp(7px, 2cqi, 10px) clamp(10px, 3cqi, 14px);
    border-radius: 10px; background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    cursor: pointer; text-align: left; transition: all 150ms;
  }
  .persona-row:hover { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.15); }
  .persona-row.active { border-color: rgba(139,92,246,0.35); background: rgba(139,92,246,0.08); }
  .persona-row:disabled { opacity: 0.6; cursor: default; }
  .persona-row + .persona-row { margin-top: 4px; }

  .persona-avatar {
    width: 26px; height: 26px; min-width: 26px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    display: flex; align-items: center; justify-content: center;
  }
  .persona-avatar.none { background: rgba(90,90,122,0.3); }
  .persona-avatar-initial { font-size: 11px; font-weight: 700; color: #fff; text-transform: uppercase; }
  .persona-avatar-img { width: 100%; height: 100%; border-radius: 50%; object-fit: cover; }

  .persona-name {
    flex: 1; font-size: var(--text-sm); font-weight: 600; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
</style>
