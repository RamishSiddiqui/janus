<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { activeConversationId, loadConversations } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  interface GalleryCharacter {
    id: string;
    name: string;
    description: string;
    tag: string;
    tagColor: string;
    tagBg: string;
    gradientStart: string;
    gradientEnd: string;
    isFavorite: boolean;
    avatarPath: string | null;
    avatarUrl: string | null;
  }

  let searchQuery = $state('');
  let isLoading = $state(true);
  let isImporting = $state(false);

  let characters: GalleryCharacter[] = $state([]);

  const mockCharacters: GalleryCharacter[] = [
    { id: '1', name: 'Aria Silverleaf', description: 'Half-elf with untamed elemental magic, determined to prove herself at the College of Magic.', tag: 'Fantasy', tagColor: '#8B5CF6', tagBg: 'rgba(139,92,246,0.12)', gradientStart: '#2d1b69', gradientEnd: '#8B5CF6', isFavorite: true, avatarPath: null, avatarUrl: '/avatars/aria-silverleaf.jpg' },
    { id: '2', name: 'Roran Ironfist', description: 'Royal blacksmith\'s son mastering runic enchantment magic to forge unbreakable weapons.', tag: 'Craft', tagColor: '#F59E0B', tagBg: 'rgba(245,158,11,0.12)', gradientStart: '#3B1B1B', gradientEnd: '#F59E0B', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/roran-ironfist.jpg' },
    { id: '3', name: 'Lila Stormwhisper', description: 'A farm girl who once commanded lightning, now determined to prove she belongs at the College.', tag: 'Storm Magic', tagColor: '#00F2FF', tagBg: 'rgba(0,242,255,0.12)', gradientStart: '#1B3A4B', gradientEnd: '#00F2FF', isFavorite: true, avatarPath: null, avatarUrl: '/avatars/lila-stormwhisper.jpg' },
    { id: '4', name: 'Finn Shadowcloak', description: 'Charming rogue from a line of thieves, mastering illusion magic to enhance his natural stealth.', tag: 'Stealth', tagColor: '#BF40FF', tagBg: 'rgba(191,64,255,0.12)', gradientStart: '#1a0a2e', gradientEnd: '#BF40FF', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/finn-shadowcloak.jpg' },
    { id: '5', name: 'Saffron Emberheart', description: 'Brilliant scholar who has read more books than half the faculty, obsessed with recovering lost magic.', tag: 'Scholar', tagColor: '#F43F5E', tagBg: 'rgba(244,63,94,0.12)', gradientStart: '#2e0a1a', gradientEnd: '#F43F5E', isFavorite: true, avatarPath: null, avatarUrl: '/avatars/saffron-emberheart.jpg' },
    { id: '6', name: 'Nyssa Wolfheart', description: 'Fierce warrior from the eastern steppes, guided by a seer\'s prophecy to find her purpose at the College.', tag: 'Warrior', tagColor: '#10B981', tagBg: 'rgba(16,185,129,0.12)', gradientStart: '#0a2e1a', gradientEnd: '#10B981', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/nyssa-wolfheart.jpg' },
    { id: '7', name: 'Rin', description: 'Cybernetically enhanced netrunner taking contracts most hackers wouldn\'t dare touch.', tag: 'Cyberpunk', tagColor: '#EC4899', tagBg: 'rgba(236,72,153,0.12)', gradientStart: '#2e0a2e', gradientEnd: '#EC4899', isFavorite: true, avatarPath: null, avatarUrl: '/avatars/rin.jpg' },
    { id: '8', name: 'Kai', description: 'Ex-corporate security specialist turned rogue, using insider knowledge to fight the megacorps.', tag: 'Cyberpunk', tagColor: '#6366F1', tagBg: 'rgba(99,102,241,0.12)', gradientStart: '#0a0a2e', gradientEnd: '#6366F1', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/kai.jpg' },
    { id: '9', name: 'Ryker', description: 'Street mercenary with a cybernetic arm and a strict code — no kids, no hospitals, no civilians.', tag: 'Mercenary', tagColor: '#EF4444', tagBg: 'rgba(239,68,68,0.12)', gradientStart: '#2e0a0a', gradientEnd: '#EF4444', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/ryker.jpg' },
    { id: '10', name: 'Echo', description: 'Ghost hacker who erased her own identity — she knows everyone\'s secrets but reveals nothing of her own.', tag: 'Mystery', tagColor: '#14B8A6', tagBg: 'rgba(20,184,166,0.12)', gradientStart: '#0a2e2e', gradientEnd: '#14B8A6', isFavorite: false, avatarPath: null, avatarUrl: '/avatars/echo.jpg' },
  ];

  let filteredCharacters = $derived(
    searchQuery
      ? characters.filter(c => c.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : characters
  );

  let favoriteCount = $derived(characters.filter(c => c.isFavorite).length);

  async function resolveAvatarUrl(avatarPath: string | null): Promise<string | null> {
    if (!avatarPath || !isTauri) return null;
    try {
      const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
      const bytes = await readFile(avatarPath, { baseDir: BaseDirectory.AppData });
      const ext = avatarPath.split('.').pop()?.toLowerCase() || 'jpeg';
      const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg';
      const blob = new Blob([bytes], { type: mime });
      return URL.createObjectURL(blob);
    } catch (e) {
      console.warn('Failed to resolve avatar:', avatarPath, e);
      return null;
    }
  }

  onMount(async () => {
    if (!isTauri) {
      characters = mockCharacters;
      isLoading = false;
      return;
    }

    try {
      const ipc = await import('$lib/services/ipc');
      const chars = await ipc.listCharacters();

      // Map backend characters to gallery format
      const tagColors = ['#8B5CF6', '#00F2FF', '#BF40FF', '#F59E0B', '#10B981', '#F43F5E'];
      const mapped = await Promise.all(chars.map(async (c, i) => {
        const color = tagColors[i % tagColors.length];
        let data: Record<string, unknown> = {};
        try { data = JSON.parse(c.data); } catch {}

        const avatarUrl = await resolveAvatarUrl(c.avatar_path);

        return {
          id: c.id,
          name: c.name,
          description: (data.description as string) || 'No description available.',
          tag: (data.tags as string[])?.[0] || 'General',
          tagColor: color,
          tagBg: `${color}1F`,
          gradientStart: darkenColor(color),
          gradientEnd: color,
          isFavorite: false,
          avatarPath: c.avatar_path,
          avatarUrl,
        };
      }));
      characters = mapped;
    } catch (err) {
      console.error('Failed to load characters:', err);
      characters = mockCharacters;
    }
    isLoading = false;
  });

  function toggleFavorite(id: string) {
    const char = characters.find(c => c.id === id);
    if (char) char.isFavorite = !char.isFavorite;
  }

  /** Import a character card PNG via file dialog */
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
      const newChar = await ipc.importCharacterCard(filePath);

      // Add to the gallery immediately
      const tagColors = ['#8B5CF6', '#00F2FF', '#BF40FF', '#F59E0B', '#10B981', '#F43F5E'];
      const color = tagColors[characters.length % tagColors.length];
      let data: Record<string, unknown> = {};
      try { data = JSON.parse(newChar.data); } catch {}

      const avatarUrl = await resolveAvatarUrl(newChar.avatar_path);

      characters = [{
        id: newChar.id,
        name: newChar.name,
        description: (data.description as string) || 'No description available.',
        tag: (data.tags as string[])?.[0] || 'Imported',
        tagColor: color,
        tagBg: `${color}1F`,
        gradientStart: darkenColor(color),
        gradientEnd: color,
        isFavorite: false,
        avatarPath: newChar.avatar_path,
        avatarUrl,
      }, ...characters];

      success(`Imported ${newChar.name}`);
    } catch (err) {
      const msg = (err as any)?.message ?? 'Unknown error';
      toastError(`Import failed: ${msg}`);
    }
    isImporting = false;
  }

  /** Navigate to chat with this character */
  async function startChat(charId: string) {
    if (!isTauri) {
      goto('/');
      return;
    }

    try {
      const { createConversation } = await import('$lib/stores/chat');
      await createConversation(charId, characters.find(c => c.id === charId)?.name);
      goto('/');
    } catch (err) {
      console.error('Failed to start chat:', err);
    }
  }

  function darkenColor(hex: string): string {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return `#${Math.floor(r * 0.3).toString(16).padStart(2, '0')}${Math.floor(g * 0.3).toString(16).padStart(2, '0')}${Math.floor(b * 0.3).toString(16).padStart(2, '0')}`;
  }

  // --- Character Editor Modal ---
  let showEditor = $state(false);
  let editingId: string | null = $state(null);
  let editorName = $state('');
  let editorDesc = $state('');
  let editorPersonality = $state('');
  let editorScenario = $state('');
  let editorFirstMessage = $state('');
  let editorTags = $state('');
  let editorSystemPrompt = $state('');
  let isSavingEditor = $state(false);

  function openCreateEditor() {
    editingId = null;
    editorName = '';
    editorDesc = '';
    editorPersonality = '';
    editorScenario = '';
    editorFirstMessage = '';
    editorTags = '';
    editorSystemPrompt = '';
    showEditor = true;
  }

  async function openEditEditor(charId: string) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const char = await ipc.getCharacter(charId);
      let data: Record<string, unknown> = {};
      try { data = JSON.parse(char.data); } catch {}

      editingId = charId;
      editorName = char.name;
      editorDesc = (data.description as string) || '';
      editorPersonality = (data.personality as string) || '';
      editorScenario = (data.scenario as string) || '';
      editorFirstMessage = (data.first_mes as string) || '';
      editorTags = ((data.tags as string[]) || []).join(', ');
      editorSystemPrompt = (data.system_prompt as string) || '';
      showEditor = true;
    } catch (err) {
      console.error('Failed to load character:', err);
    }
  }

  async function saveCharacter() {
    if (!isTauri || !editorName.trim()) return;
    isSavingEditor = true;

    const data: Record<string, unknown> = {
      description: editorDesc,
      personality: editorPersonality,
      scenario: editorScenario,
      first_mes: editorFirstMessage,
      tags: editorTags.split(',').map(t => t.trim()).filter(Boolean),
      system_prompt: editorSystemPrompt,
    };

    try {
      const ipc = await import('$lib/services/ipc');
      let saved: { id: string; name: string; data: string; avatar_path: string | null };

      if (editingId) {
        saved = await ipc.updateCharacter(editingId, editorName, data);
      } else {
        saved = await ipc.createCharacter(editorName, data);
      }

      // Refresh gallery
      const tagColors = ['#8B5CF6', '#00F2FF', '#BF40FF', '#F59E0B', '#10B981', '#F43F5E'];
      const color = tagColors[characters.length % tagColors.length];
      const parsedTags = editorTags.split(',').map(t => t.trim()).filter(Boolean);
      const avatarUrl = await resolveAvatarUrl(saved.avatar_path);

      const displayChar: GalleryCharacter = {
        id: saved.id,
        name: saved.name,
        description: editorDesc || 'No description available.',
        tag: parsedTags[0] || 'General',
        tagColor: color,
        tagBg: `${color}1F`,
        gradientStart: darkenColor(color),
        gradientEnd: color,
        isFavorite: false,
        avatarPath: saved.avatar_path,
        avatarUrl,
      };

      if (editingId) {
        characters = characters.map(c => c.id === editingId ? { ...c, ...displayChar } : c);
        success('Character updated');
      } else {
        characters = [displayChar, ...characters];
        success(`Created ${saved.name}`);
      }

      showEditor = false;
    } catch (err) {
      toastError('Failed to save character');
    }
    isSavingEditor = false;
  }

  async function handleDeleteCharacter(charId: string) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteCharacter(charId);
      const name = characters.find(c => c.id === charId)?.name ?? 'Character';
      characters = characters.filter(c => c.id !== charId);
      success(`Deleted ${name}`);
    } catch (err) {
      toastError('Failed to delete character');
    }
  }
</script>

<svelte:head>
  <title>Character Gallery — Mythic</title>
</svelte:head>

<div class="gallery-page">
  <!-- Header -->
  <header class="gallery-header">
    <div class="gallery-header-left">
      <h1 class="gallery-title">Character Gallery</h1>
      <span class="gallery-subtitle">{characters.length} characters • {favoriteCount} favorites</span>
    </div>
    <div class="gallery-header-right">
      <div class="gallery-search">
        <Icon name="search" size={14} color="var(--fg-muted)" />
        <input type="text" placeholder="Search characters..." aria-label="Search characters" bind:value={searchQuery} />
      </div>
      <button class="gallery-btn outline" aria-label="Import character" onclick={handleImport} disabled={isImporting}>
        <Icon name="download" size={14} color="var(--fg-secondary)" />
        <span>{isImporting ? 'Importing...' : 'Import'}</span>
      </button>
      <button class="gallery-btn primary" aria-label="Create new character" onclick={openCreateEditor}>
        <Icon name="plus" size={14} color="#FFFFFF" />
        <span>Create</span>
      </button>
    </div>
  </header>

  <!-- Card Grid -->
  <div class="card-grid">
    {#if isLoading}
      {#each Array(6) as _, i}
        <div class="char-card skeleton-card">
          <Skeleton variant="card" height="180px" />
          <div class="skeleton-body">
            <Skeleton variant="text" width="60%" height="14px" />
            <Skeleton variant="text" width="90%" height="10px" />
            <Skeleton variant="text" width="40%" height="10px" />
          </div>
        </div>
      {/each}
    {:else}
    {#each filteredCharacters as char, i (char.id)}
      <div class="char-card animate-fade-in-up stagger-{Math.min(i + 1, 6)}">
        <div 
          class="card-image"
          style="background: linear-gradient(135deg, {char.gradientStart}, {char.gradientEnd}30);"
          onclick={() => goto('/gallery/' + char.id)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && goto('/gallery/' + char.id)}
        >
          {#if char.avatarUrl}
            <img src={char.avatarUrl} alt={char.name} class="card-avatar-img" />
          {:else}
            <div style="width:100%;height:100%;background:linear-gradient(135deg, {char.gradientStart}, {char.gradientEnd}30)"></div>
          {/if}
          <div class="card-image-overlay"></div>
        </div>
        <div class="card-body">
          <div class="card-top">
            <span class="card-name">{char.name}</span>
            <span class="card-desc">{char.description}</span>
          </div>
          <div class="card-bottom">
            <div class="card-tags">
              <span class="card-tag" style="background: {char.tagBg}; color: {char.tagColor};">
                {char.tag}
              </span>
            </div>
            <div class="card-actions-row">
              <button 
                class="card-action-btn"
                title="Edit"
                aria-label="Edit {char.name}"
                onclick={() => openEditEditor(char.id)}
              >
                <Icon name="pencil" size={12} color="var(--fg-muted)" />
              </button>
              <button 
                class="card-action-btn danger"
                title="Delete"
                aria-label="Delete {char.name}"
                onclick={() => handleDeleteCharacter(char.id)}
              >
                <Icon name="trash-2" size={12} color="var(--fg-muted)" />
              </button>
              <button 
                class="fav-btn"
                class:active={char.isFavorite}
                aria-label={char.isFavorite ? `Remove ${char.name} from favorites` : `Add ${char.name} to favorites`}
                onclick={() => toggleFavorite(char.id)}
              >
                <Icon name="heart" size={14} color={char.isFavorite ? 'var(--danger)' : 'var(--fg-muted)'} />
              </button>
            </div>
          </div>
        </div>
      </div>
    {/each}

    {/if}

    {#if !isLoading && filteredCharacters.length === 0}
      <div class="empty-state">
        <Icon name="search" size={32} color="var(--fg-muted)" />
        <span class="empty-title">No characters found</span>
        <span class="empty-desc">Try a different search term or create a new character.</span>
      </div>
    {/if}
  </div>

  <!-- Character Editor Modal -->
  {#if showEditor}
    <div class="editor-backdrop" onclick={() => showEditor = false} onkeydown={(e) => e.key === 'Escape' && (showEditor = false)} role="dialog" aria-modal="true" aria-label={editingId ? 'Edit character' : 'Create character'} tabindex="-1">
      <div class="editor-card" onclick={(e) => e.stopPropagation()} role="document">
        <div class="editor-header">
          <span class="editor-title">{editingId ? 'Edit Character' : 'Create Character'}</span>
          <button class="editor-close" onclick={() => showEditor = false} aria-label="Close">
            <Icon name="x" size={16} color="var(--fg-muted)" />
          </button>
        </div>

        <div class="editor-body">
          <div class="editor-field">
            <label class="editor-label" for="ed-name">Name *</label>
            <input id="ed-name" class="editor-input" bind:value={editorName} placeholder="Character name" />
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-desc">Description</label>
            <textarea id="ed-desc" class="editor-textarea" rows="3" bind:value={editorDesc} placeholder="A brief description of the character..."></textarea>
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-personality">Personality</label>
            <textarea id="ed-personality" class="editor-textarea" rows="3" bind:value={editorPersonality} placeholder="Character personality traits, behaviors, speech patterns..."></textarea>
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-scenario">Scenario</label>
            <textarea id="ed-scenario" class="editor-textarea" rows="2" bind:value={editorScenario} placeholder="The setting or context for the conversation..."></textarea>
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-first-msg">First Message</label>
            <textarea id="ed-first-msg" class="editor-textarea" rows="3" bind:value={editorFirstMessage} placeholder="The character's opening message..."></textarea>
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-system">System Prompt</label>
            <textarea id="ed-system" class="editor-textarea" rows="2" bind:value={editorSystemPrompt} placeholder="Optional system-level instructions..."></textarea>
          </div>

          <div class="editor-field">
            <label class="editor-label" for="ed-tags">Tags</label>
            <input id="ed-tags" class="editor-input" bind:value={editorTags} placeholder="Fantasy, Adventure, Romance (comma-separated)" />
          </div>
        </div>

        <div class="editor-footer">
          <button class="gallery-btn outline" onclick={() => showEditor = false}>Cancel</button>
          <button class="gallery-btn primary" onclick={saveCharacter} disabled={isSavingEditor || !editorName.trim()}>
            {isSavingEditor ? 'Saving...' : editingId ? 'Save Changes' : 'Create Character'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .gallery-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
    position: relative;
  }

  /* ── Header ── */
  .gallery-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 20px 28px 18px; flex-shrink: 0; position: relative;
  }
  .gallery-header::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .gallery-header-left { display: flex; flex-direction: column; gap: 3px; }
  .gallery-title {
    font-size: var(--text-2xl); font-weight: 800; color: #e8e0ff;
    letter-spacing: -0.5px;
    background: linear-gradient(135deg, #e8e0ff, #c4a1ff);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent;
  }
  .gallery-subtitle {
    font-size: var(--text-sm); color: #5a5a7a; font-family: var(--font-mono);
    letter-spacing: 0.5px;
  }

  .gallery-header-right { display: flex; align-items: center; gap: 10px; }

  .gallery-search {
    display: flex; align-items: center; gap: 8px;
    width: 220px; height: 36px; padding: 0 12px;
    border-radius: 12px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.08);
    transition: all 250ms ease;
  }
  .gallery-search:focus-within {
    border-color: rgba(139,92,246,0.35);
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06);
    background: rgba(18,18,36,0.9); width: 260px;
  }
  .gallery-search input {
    flex: 1; background: none; border: none; outline: none;
    color: #e0e0f0; font-size: 13px; font-family: var(--font-body);
  }
  .gallery-search input::placeholder { color: #4a4a6a; }

  .gallery-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    font-size: 13px; font-family: var(--font-body); font-weight: 600;
    border: none; cursor: pointer; transition: all 180ms ease;
  }
  .gallery-btn.outline {
    background: transparent; border: 1px solid rgba(139,92,246,0.12);
    color: #8b8ba7;
  }
  .gallery-btn.outline:hover {
    background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2);
  }
  .gallery-btn.primary {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); color: #fff;
    box-shadow: 0 2px 12px rgba(139,92,246,0.25);
  }
  .gallery-btn.primary:hover {
    box-shadow: 0 4px 20px rgba(139,92,246,0.4);
    transform: translateY(-1px);
  }

  /* ── Card Grid ── */
  .card-grid {
    padding: 24px 28px; overflow-y: auto; flex: 1;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    grid-auto-rows: max-content;
    justify-content: center;
    gap: 20px;
    align-content: start;
  }
  .card-grid::-webkit-scrollbar { width: 4px; }
  .card-grid::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  /* ── Cards ── */
  .char-card {
    break-inside: avoid;
    border-radius: 16px; overflow: hidden;
    background: rgba(14,14,30,0.5);
    border: 1px solid rgba(139,92,246,0.06);
    display: flex; flex-direction: column;
    height: auto;
    min-height: min-content;
    transition: transform 280ms cubic-bezier(0.34,1.56,0.64,1),
                border-color 200ms ease, box-shadow 280ms ease;
    position: relative;
  }
  .char-card::before {
    content: ''; position: absolute; inset: -1px; border-radius: 17px; z-index: -1;
    background: linear-gradient(135deg, rgba(139,92,246,0.2), rgba(191,64,255,0.1));
    opacity: 0; transition: opacity 280ms ease;
  }
  .char-card:hover {
    transform: translateY(-4px) scale(1.01);
    border-color: rgba(139,92,246,0.15);
    box-shadow: 0 12px 40px rgba(0,0,0,0.35), 0 0 20px rgba(139,92,246,0.08);
  }
  .char-card:hover::before { opacity: 1; }

  .card-image {
    width: 100%; position: relative; overflow: hidden; cursor: pointer;
    aspect-ratio: 3 / 4;
    flex-shrink: 0;
  }
  .card-avatar-img {
    width: 100%; height: 100%; display: block; object-fit: cover;
    transition: transform 400ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .char-card:hover .card-avatar-img { transform: scale(1.06); }

  .card-image-overlay {
    position: absolute; inset: 0;
    background: linear-gradient(to bottom, transparent 40%, rgba(9,9,26,0.95) 100%);
  }

  .card-body {
    padding: 14px 16px 16px; display: flex; flex-direction: column; gap: 8px;
    flex-shrink: 0;
    flex-grow: 1;
  }
  .card-top { display: flex; flex-direction: column; gap: 4px; }
  .card-name {
    font-size: var(--text-lg); font-weight: 700; color: #e8e0ff; letter-spacing: -0.2px;
  }
  .card-desc {
    font-size: var(--text-sm); color: #6b6b8a; line-height: 1.5;
    display: -webkit-box; -webkit-line-clamp: 2;
    -webkit-box-orient: vertical; overflow: hidden;
  }

  .card-bottom { display: flex; justify-content: space-between; align-items: center; }
  .card-tags { display: flex; gap: 5px; }
  .card-tag {
    padding: 3px 9px; border-radius: 99px;
    font-size: var(--text-xs); font-weight: 700; letter-spacing: 0.3px;
  }

  .fav-btn {
    background: none; border: none; padding: 5px; border-radius: 8px;
    cursor: pointer;
    transition: transform 200ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .fav-btn:hover { transform: scale(1.25); }
  .fav-btn.active { animation: heartBounce 300ms cubic-bezier(0.34,1.56,0.64,1); }
  @keyframes heartBounce { 0% { transform: scale(1); } 50% { transform: scale(1.4); } 100% { transform: scale(1); } }

  .card-actions-row { display: flex; gap: 3px; align-items: center; }
  .card-action-btn {
    background: none; border: none; padding: 5px; border-radius: 8px;
    opacity: 0; cursor: pointer;
    transition: opacity 150ms, background 120ms, transform 100ms;
  }
  .char-card:hover .card-action-btn { opacity: 0.6; }
  .card-action-btn:hover { opacity: 1 !important; background: rgba(139,92,246,0.08); }
  .card-action-btn:active { transform: scale(0.9); }
  .card-action-btn.danger:hover { background: rgba(244,63,94,0.1); }

  /* ── Responsive ── */
  @media (max-width: 600px) {
    .card-grid { grid-template-columns: 1fr; padding: 16px; }
    .gallery-header {
      flex-direction: column; gap: 12px; align-items: flex-start; padding: 16px;
    }
    .gallery-header-right { width: 100%; flex-wrap: wrap; }
    .gallery-search { width: 100%; }
  }

  /* ── Empty State ── */
  .empty-state {
    grid-column: 1 / -1; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 10px;
    padding: 60px 16px;
  }
  .empty-title { font-size: var(--text-lg); font-weight: 600; color: #8b8ba7; }
  .empty-desc { font-size: var(--text-sm); color: #4a4a6a; }

  /* ── Editor Modal ── */
  .editor-backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.7); backdrop-filter: blur(8px);
    display: flex; align-items: center; justify-content: center; z-index: 200;
  }
  .editor-card {
    background: linear-gradient(175deg, #0e0e22, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12);
    border-radius: 20px; width: 540px; max-width: 92vw; max-height: 85vh;
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

  .editor-body {
    padding: 20px 24px; display: flex; flex-direction: column; gap: 14px;
    overflow-y: auto; flex: 1;
  }
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
  .editor-textarea {
    padding: 10px 14px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #e0e0f0; font-size: 14px; font-family: var(--font-body);
    line-height: 1.6; resize: vertical; outline: none;
    transition: border-color 200ms;
  }
  .editor-textarea:focus { border-color: rgba(139,92,246,0.35); }
  .editor-footer {
    display: flex; justify-content: flex-end; gap: 10px;
    padding: 0 24px 22px;
  }

  /* ── Skeleton ── */
  .skeleton-card { overflow: hidden; }
  .skeleton-body { padding: 14px; display: flex; flex-direction: column; gap: 8px; }

  /* ── Staggered Entrance ── */
  .animate-fade-in-up {
    animation: fadeInUp 400ms ease both;
  }
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
</style>
