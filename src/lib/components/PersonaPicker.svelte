<script lang="ts">
  import { browser } from '$app/environment';
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { selectedPersonaId } from '$lib/stores/personas';
  import { loadFileAsBlobUrl } from '$lib/utils/blobUrl';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  interface PersonaOption {
    id: string;
    name: string;
    avatarUrl: string | null;
  }

  let personas: PersonaOption[] = $state([]);
  let isOpen = $state(false);

  let selectedName = $derived(
    personas.find(p => p.id === $selectedPersonaId)?.name ?? 'None'
  );

  onMount(async () => {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const list = await ipc.listPersonas();
      personas = await Promise.all(list.map(async (p) => ({
        id: p.id,
        name: p.name,
        avatarUrl: p.avatar_path ? await loadFileAsBlobUrl(p.avatar_path).catch(() => null) : null,
      })));
      // Selected persona may have since been deleted — fall back to none.
      if ($selectedPersonaId && !personas.some(p => p.id === $selectedPersonaId)) {
        selectedPersonaId.set(null);
      }
    } catch (err) {
      console.error('Failed to load personas for picker:', err);
    }
  });

  function choose(id: string | null) {
    selectedPersonaId.set(id);
    isOpen = false;
  }
</script>

{#if isTauri && personas.length > 0}
  <div class="persona-picker">
    <button class="pp-trigger" onclick={() => isOpen = !isOpen} aria-label="Choose persona for this chat">
      <Icon name="user" size={12} color="var(--fg-muted)" />
      <span>Playing as: {selectedName}</span>
      <Icon name={isOpen ? 'chevron-up' : 'chevron-down'} size={11} color="var(--fg-muted)" />
    </button>
    {#if isOpen}
      <div class="pp-menu">
        <button class="pp-item" class:active={!$selectedPersonaId} onclick={() => choose(null)}>None</button>
        {#each personas as persona (persona.id)}
          <button class="pp-item" class:active={$selectedPersonaId === persona.id} onclick={() => choose(persona.id)}>
            {persona.name}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .persona-picker { position: relative; display: inline-block; }
  .pp-trigger {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 8px;
    background: rgba(139,92,246,0.06); border: 1px solid rgba(139,92,246,0.12);
    color: var(--fg-muted); font-size: 12px; cursor: pointer; transition: all 150ms;
  }
  .pp-trigger:hover { border-color: rgba(139,92,246,0.25); background: rgba(139,92,246,0.1); }
  .pp-menu {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 20;
    min-width: 160px; max-height: 220px; overflow-y: auto;
    background: #100f22; border: 1px solid rgba(139,92,246,0.15);
    border-radius: 10px; padding: 4px; box-shadow: 0 12px 32px rgba(0,0,0,0.5);
  }
  .pp-item {
    display: block; width: 100%; text-align: left; padding: 7px 10px;
    border-radius: 6px; background: none; border: none; cursor: pointer;
    color: #c8c8e0; font-size: 12.5px;
  }
  .pp-item:hover { background: rgba(139,92,246,0.1); }
  .pp-item.active { color: var(--accent-primary); font-weight: 600; }
</style>
