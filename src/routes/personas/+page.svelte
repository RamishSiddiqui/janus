<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import { success, error as toastError, addToast } from '$lib/stores/toast';
  import { parseCharacterData } from '$lib/utils/character';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  interface GalleryPersona {
    id: string;
    name: string;
    description: string;
    avatarPath: string | null;
    avatarUrl: string | null;
  }

  let searchQuery = $state('');
  let isLoading = $state(true);
  let isImporting = $state(false);

  let personas: GalleryPersona[] = $state([]);

  let filteredPersonas = $derived(
    searchQuery
      ? personas.filter(p => p.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : personas
  );

  async function resolveAvatarUrl(avatarPath: string | null): Promise<string | null> {
    if (!avatarPath || !isTauri) return null;
    try {
      const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
      return await loadFileAsBlobUrl(avatarPath);
    } catch (e) {
      console.warn('Failed to resolve avatar:', avatarPath, e);
      return null;
    }
  }

  onMount(async () => {
    if (!isTauri) {
      isLoading = false;
      return;
    }
    try {
      const ipc = await import('$lib/services/ipc');
      const list = await ipc.listPersonas();
      const mapped = await Promise.all(list.map(async (p) => {
        const data = parseCharacterData(p.data);
        const avatarUrl = await resolveAvatarUrl(p.avatar_path);
        return {
          id: p.id,
          name: p.name,
          description: (data.description as string) || 'No description yet.',
          avatarPath: p.avatar_path,
          avatarUrl,
        };
      }));
      personas = mapped.sort((a, b) => a.name.localeCompare(b.name));
    } catch (err) {
      console.error('Failed to load personas:', err);
    }
    isLoading = false;
  });

  /** Import a persona card PNG via file dialog */
  async function handleImport() {
    if (!isTauri) return;
    isImporting = true;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Character Card', extensions: ['png'] }],
      });
      if (!selected) {
        isImporting = false;
        return;
      }
      const filePath = selected as string;
      const ipc = await import('$lib/services/ipc');
      const newPersona = await ipc.importPersonaCard(filePath);
      const data = parseCharacterData(newPersona.data);
      const avatarUrl = await resolveAvatarUrl(newPersona.avatar_path);
      personas = [{
        id: newPersona.id,
        name: newPersona.name,
        description: (data.description as string) || 'No description yet.',
        avatarPath: newPersona.avatar_path,
        avatarUrl,
      }, ...personas];
      success(`Imported ${newPersona.name}`);
    } catch (err) {
      const msg = (err as any)?.message ?? 'Unknown error';
      toastError(`Import failed: ${msg}`);
    }
    isImporting = false;
  }

  // --- Quick-create (name only — the rest is filled in on the detail page) ---
  let showCreate = $state(false);
  let createName = $state('');
  let isCreating = $state(false);

  async function handleCreate() {
    if (!isTauri || !createName.trim()) return;
    isCreating = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const persona = await ipc.createPersona(createName.trim(), {});
      showCreate = false;
      createName = '';
      goto('/personas/' + persona.id);
    } catch (err) {
      toastError('Failed to create persona');
    }
    isCreating = false;
  }

  /**
   * Moves a persona to Trash immediately — a real, durable backend
   * soft-delete. The Undo toast's action calls restorePersona, which is
   * just as immediate, so there's no window where the delete could be
   * silently lost. Permanent removal only happens from the Trash page.
   */
  async function handleDeletePersona(id: string) {
    if (!isTauri) return;
    const removed = personas.find(p => p.id === id);
    const name = removed?.name ?? 'Persona';

    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.trashPersona(id);
    } catch (err) {
      toastError(`Failed to delete ${name}`);
      return;
    }

    personas = personas.filter(p => p.id !== id);

    addToast(`Moved ${name} to Trash`, 'info', 5500, {
      label: 'Undo',
      onClick: async () => {
        try {
          const ipc = await import('$lib/services/ipc');
          await ipc.restorePersona(id);
          if (removed) personas = [...personas, removed].sort((a, b) => a.name.localeCompare(b.name));
        } catch {
          toastError('Failed to restore persona');
        }
      },
    });
  }
</script>

<svelte:head>
  <title>Personas — Janus</title>
</svelte:head>

<div class="personas-page">
  <header class="personas-header">
    <div class="personas-header-left">
      <h1 class="personas-title"><SplitHeading text="Personas" /></h1>
      <span class="personas-subtitle">{personas.length} personas</span>
    </div>
    <div class="personas-header-right">
      <div class="personas-search">
        <Icon name="search" size={14} color="var(--fg-muted)" />
        <input type="text" placeholder="Search personas..." aria-label="Search personas" bind:value={searchQuery} />
      </div>
      <button class="personas-btn outline" aria-label="Import persona" onclick={handleImport} disabled={isImporting}>
        <Icon name="download" size={14} color="var(--fg-secondary)" />
        <span>{isImporting ? 'Importing...' : 'Import'}</span>
      </button>
      <button class="personas-btn primary" aria-label="Create new persona" onclick={() => showCreate = true}>
        <Icon name="plus" size={14} color="#FFFFFF" />
        <span>Create</span>
      </button>
    </div>
  </header>

  <div class="card-grid">
    {#if isLoading}
      {#each Array(4) as _, i}
        <div class="persona-card skeleton-card">
          <Skeleton variant="card" height="180px" />
          <div class="skeleton-body">
            <Skeleton variant="text" width="60%" height="14px" />
            <Skeleton variant="text" width="90%" height="10px" />
          </div>
        </div>
      {/each}
    {:else}
      {#each filteredPersonas as persona, i (persona.id)}
        <div class="persona-card animate-fade-in-up stagger-{Math.min(i + 1, 6)}">
          <div
            class="card-image"
            onclick={() => goto('/personas/' + persona.id)}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && goto('/personas/' + persona.id)}
          >
            {#if persona.avatarUrl}
              <img src={persona.avatarUrl} alt={persona.name} class="card-avatar-img" />
            {:else}
              <div class="card-avatar-fallback">
                <Icon name="user" size={32} color="rgba(232,224,255,0.35)" />
              </div>
            {/if}
            <div class="card-image-overlay"></div>
          </div>
          <div class="card-body">
            <div class="card-top">
              <span class="card-name">{persona.name}</span>
              <span class="card-desc">{persona.description}</span>
            </div>
            <div class="card-actions-row">
              <button
                class="card-action-btn danger"
                title="Delete"
                aria-label="Delete {persona.name}"
                onclick={(e) => { e.stopPropagation(); handleDeletePersona(persona.id); }}
              >
                <Icon name="trash-2" size={12} color="var(--fg-muted)" />
              </button>
            </div>
          </div>
        </div>
      {/each}
    {/if}

    {#if !isLoading && filteredPersonas.length === 0}
      <div class="empty-state">
        <Icon name="user" size={32} color="var(--fg-muted)" />
        <span class="empty-title">No personas yet</span>
        <span class="empty-desc">Create one to play as someone specific in your chats.</span>
      </div>
    {/if}
  </div>

  {#if showCreate}
    <div class="editor-backdrop" onclick={() => showCreate = false} onkeydown={(e) => e.key === 'Escape' && (showCreate = false)} role="dialog" aria-modal="true" aria-label="Create persona" tabindex="-1">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="editor-card" onclick={(e) => e.stopPropagation()}>
        <div class="editor-header">
          <span class="editor-title">Create Persona</span>
          <button class="editor-close" onclick={() => showCreate = false} aria-label="Close">
            <Icon name="x" size={16} color="var(--fg-muted)" />
          </button>
        </div>
        <div class="editor-body">
          <div class="editor-field">
            <label class="editor-label" for="pe-name">Name *</label>
            <input id="pe-name" class="editor-input" bind:value={createName} placeholder="Persona name" onkeydown={(e) => e.key === 'Enter' && handleCreate()} />
          </div>
        </div>
        <div class="editor-footer">
          <button class="personas-btn outline" onclick={() => showCreate = false}>Cancel</button>
          <button class="personas-btn primary" onclick={handleCreate} disabled={isCreating || !createName.trim()}>
            {isCreating ? 'Creating...' : 'Create & Continue'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .personas-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
    position: relative;
  }

  .personas-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 20px 28px 18px; flex-shrink: 0; position: relative;
  }
  .personas-header::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .personas-header-left { display: flex; flex-direction: column; gap: 3px; }
  .personas-title {
    font-size: var(--text-2xl); font-weight: 600;
    letter-spacing: -0.5px;
  }
  .personas-subtitle {
    font-size: var(--text-md); color: #5a5a7a; font-family: var(--font-mono);
    letter-spacing: 0.5px;
  }

  .personas-header-right { display: flex; align-items: center; gap: 10px; }

  .personas-search {
    display: flex; align-items: center; gap: 8px;
    width: 220px; height: 36px; padding: 0 12px;
    border-radius: 12px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.08);
    transition: all 250ms ease;
  }
  .personas-search:focus-within {
    border-color: rgba(139,92,246,0.35);
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06);
    background: rgba(18,18,36,0.9); width: 260px;
  }
  .personas-search input {
    flex: 1; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: 13px; font-family: var(--font-body);
  }
  .personas-search input::placeholder { color: #4a4a6a; }

  .personas-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    font-size: 13px; font-family: var(--font-body); font-weight: 600;
    border: none; cursor: pointer; transition: all 180ms ease;
  }
  .personas-btn.outline {
    background: transparent; border: 1px solid rgba(139,92,246,0.12);
    color: #8b8ba7;
  }
  .personas-btn.outline:hover {
    background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2);
  }
  .personas-btn.primary {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); color: #fff;
    box-shadow: 0 2px 12px rgba(139,92,246,0.25);
  }
  .personas-btn.primary:hover {
    box-shadow: 0 4px 20px rgba(139,92,246,0.4);
    transform: translateY(-1px);
  }
  .personas-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .card-grid {
    padding: 24px 28px; overflow-y: auto; flex: 1;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    grid-auto-rows: max-content;
    justify-content: center;
    gap: 20px;
    align-content: start;
  }

  .persona-card {
    border-radius: 16px; overflow: hidden;
    background: rgba(14,14,30,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    display: flex; flex-direction: column;
    transition: transform 280ms cubic-bezier(0.34,1.56,0.64,1),
                border-color 200ms ease, box-shadow 280ms ease;
    position: relative;
  }
  .persona-card:hover {
    transform: translateY(-4px) scale(1.01);
    border-color: rgba(139,92,246,0.15);
    box-shadow: 0 12px 40px rgba(0,0,0,0.35), 0 0 20px rgba(139,92,246,0.08);
  }

  .card-image {
    width: 100%; position: relative; overflow: hidden; cursor: pointer;
    aspect-ratio: 1 / 1;
    flex-shrink: 0;
    background: linear-gradient(135deg, #2d1b69, #8B5CF630);
  }
  .card-avatar-img {
    width: 100%; height: 100%; display: block; object-fit: cover;
    transition: transform 400ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .persona-card:hover .card-avatar-img { transform: scale(1.06); }
  .card-avatar-fallback {
    width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;
  }
  .card-image-overlay {
    position: absolute; inset: 0;
    background: linear-gradient(to bottom, transparent 50%, rgba(9,9,26,0.9) 100%);
  }

  .card-body {
    padding: 12px 14px 14px; display: flex; flex-direction: column; gap: 6px;
  }
  .card-top { display: flex; flex-direction: column; gap: 4px; }
  .card-name { font-size: var(--text-md); font-weight: 700; color: #e8e0ff; letter-spacing: -0.2px; }
  .card-desc {
    font-size: var(--text-xs); color: #6b6b8a; line-height: 1.5;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2;
    -webkit-box-orient: vertical; overflow: hidden;
  }
  .card-actions-row { display: flex; justify-content: flex-end; }
  .card-action-btn {
    background: none; border: none; padding: 5px; border-radius: 8px;
    opacity: 0; cursor: pointer;
    transition: opacity 150ms, background 120ms, transform 100ms;
  }
  .persona-card:hover .card-action-btn { opacity: 0.6; }
  .card-action-btn:hover { opacity: 1 !important; background: rgba(244,63,94,0.1); }
  .card-action-btn:active { transform: scale(0.9); }

  .empty-state {
    grid-column: 1 / -1; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 10px;
    padding: 60px 16px;
  }
  .empty-title { font-size: var(--text-lg); font-weight: 600; color: #8b8ba7; }
  .empty-desc { font-size: var(--text-sm); color: #4a4a6a; }

  .editor-backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.7); backdrop-filter: blur(8px);
    display: flex; align-items: center; justify-content: center; z-index: 200;
  }
  .editor-card {
    background: linear-gradient(175deg, #0e0e22, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12);
    border-radius: 20px; width: 420px; max-width: 92vw;
    display: flex; flex-direction: column;
    box-shadow: 0 24px 60px rgba(0,0,0,0.6), 0 0 30px rgba(139,92,246,0.08);
  }
  .editor-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 22px 24px 0;
  }
  .editor-title { font-size: var(--text-xl); font-weight: 700; color: #e8e0ff; }
  .editor-close {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: background 150ms;
  }
  .editor-close:hover { background: rgba(139,92,246,0.08); }
  .editor-body { padding: 20px 24px; display: flex; flex-direction: column; gap: 14px; }
  .editor-field { display: flex; flex-direction: column; gap: 6px; }
  .editor-label {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    text-transform: uppercase; letter-spacing: 1.2px;
    font-family: var(--font-mono);
  }
  .editor-input {
    height: 40px; padding: 0 14px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 14px; font-family: var(--font-body);
    outline: none; transition: border-color 200ms;
  }
  .editor-input:focus { border-color: rgba(139,92,246,0.35); }
  .editor-footer { display: flex; justify-content: flex-end; gap: 10px; padding: 0 24px 22px; }

  .skeleton-card { overflow: hidden; }
  .skeleton-body { padding: 14px; display: flex; flex-direction: column; gap: 8px; }

  .animate-fade-in-up { animation: fadeInUp 400ms ease both; }
  .stagger-1 { animation-delay: 40ms; }
  .stagger-2 { animation-delay: 80ms; }
  .stagger-3 { animation-delay: 120ms; }
  .stagger-4 { animation-delay: 160ms; }
  .stagger-5 { animation-delay: 200ms; }
  .stagger-6 { animation-delay: 240ms; }
  @keyframes fadeInUp {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @media (max-width: 600px) {
    .card-grid { grid-template-columns: 1fr 1fr; padding: 16px; }
    .personas-header { flex-direction: column; gap: 12px; align-items: flex-start; padding: 16px; }
    .personas-header-right { width: 100%; flex-wrap: wrap; }
    .personas-search { width: 100%; }
  }
</style>
