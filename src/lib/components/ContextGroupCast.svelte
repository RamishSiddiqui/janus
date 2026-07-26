<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    characterId = null,
    characterName,
    conversationId = null,
    additionalCharacters = [],
  }: {
    characterId?: string | null;
    characterName: string;
    conversationId?: string | null;
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
  } = $props();

  // Group Cast — multi-character conversation management
  interface GroupChar {
    id: string;
    conversation_id: string;
    character_id: string;
    character_name: string;
    role: string;
    talkativeness: number;
    is_active: boolean;
    created_at: string;
  }
  let groupChars: GroupChar[] = $state([]);
  let isLoadingCast = $state(false);
  let showAddChar = $state(false);
  let allCharacters: { id: string; name: string; avatar_path: string | null }[] = $state([]);
  let isLoadingAllChars = $state(false);
  // Characters not already in the group
  let availableChars = $derived.by(() => {
    const inGroup = new Set(groupChars.map(c => c.character_id));
    // Also exclude the primary character
    if (characterId) inGroup.add(characterId);
    return allCharacters.filter(c => !inGroup.has(c.id));
  });

  // Load group cast when conversation changes
  $effect(() => {
    const _conv = conversationId;
    if (_conv && isTauri) {
      loadGroupCast(_conv);
    } else {
      groupChars = [];
    }
  });

  async function loadGroupCast(convId: string) {
    isLoadingCast = true;
    try {
      const ipc = await import('$lib/services/ipc');
      let chars = await ipc.listConversationCharacters(convId);

      // ── Auto-migration ──
      // If the conversation_characters table is empty but we have characters
      // from the older shared_character_ids mechanism, seed the table now.
      // This is a one-time transparent migration for pre-existing conversations.
      if (chars.length === 0 && additionalCharacters.length > 0) {
        // First, add the primary character
        if (characterId && characterName) {
          try {
            await ipc.addConversationCharacter(convId, characterId, characterName, 'primary', 70);
          } catch { /* may already exist */ }
        }
        // Then add all additional characters
        for (const ac of additionalCharacters) {
          try {
            await ipc.addConversationCharacter(convId, ac.id, ac.name, 'secondary', 50);
          } catch { /* may already exist */ }
        }
        // Re-fetch after seeding
        chars = await ipc.listConversationCharacters(convId);
      }

      groupChars = chars.map(c => ({
        id: c.id,
        conversation_id: c.conversation_id,
        character_id: c.character_id,
        character_name: c.character_name,
        role: c.role,
        talkativeness: c.talkativeness,
        is_active: c.is_active,
        created_at: c.created_at,
      }));
    } catch (err) {
      console.error('Failed to load group cast:', err);
      groupChars = [];
    }
    isLoadingCast = false;
  }

  async function loadAllCharacters() {
    if (allCharacters.length > 0) return; // already loaded
    isLoadingAllChars = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const chars = await ipc.listCharacters();
      allCharacters = chars.map(c => ({ id: c.id, name: c.name, avatar_path: c.avatar_path }));
    } catch (err) {
      console.error('Failed to load characters:', err);
    }
    isLoadingAllChars = false;
  }

  async function addCharToGroup(charId: string, charName: string) {
    if (!conversationId || !isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const created = await ipc.addConversationCharacter(conversationId, charId, charName);
      groupChars = [...groupChars, {
        id: created.id,
        conversation_id: created.conversation_id,
        character_id: created.character_id,
        character_name: created.character_name,
        role: created.role,
        talkativeness: created.talkativeness,
        is_active: created.is_active,
        created_at: created.created_at,
      }];
      showAddChar = false;
      success(`${charName} joined the group`);
    } catch {
      toastError('Failed to add character');
    }
  }

  async function removeCharFromGroup(charId: string) {
    if (!conversationId || !isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.removeConversationCharacter(conversationId, charId);
      const name = groupChars.find(c => c.character_id === charId)?.character_name ?? 'Character';
      groupChars = groupChars.filter(c => c.character_id !== charId);
      success(`${name} removed`);
    } catch {
      toastError('Failed to remove character');
    }
  }

  async function toggleCharActive(charId: string) {
    if (!conversationId || !isTauri) return;
    const char = groupChars.find(c => c.character_id === charId);
    if (!char) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.toggleCharacterActive(conversationId, charId, !char.is_active);
      groupChars = groupChars.map(c =>
        c.character_id === charId ? { ...c, is_active: !c.is_active } : c
      );
    } catch {
      toastError('Failed to toggle character');
    }
  }

  async function updateTalkativeness(charId: string, value: number) {
    if (!conversationId || !isTauri) return;
    // Optimistic update
    groupChars = groupChars.map(c =>
      c.character_id === charId ? { ...c, talkativeness: value } : c
    );
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.updateCharacterTalkativeness(conversationId, charId, value);
    } catch {
      toastError('Failed to update talkativeness');
    }
  }

  function getRoleBadgeStyle(role: string): string {
    switch (role) {
      case 'primary': return 'background: rgba(139,92,246,0.15); color: #c4a1ff;';
      case 'secondary': return 'background: rgba(0,242,255,0.12); color: #00f2ff;';
      case 'npc': return 'background: rgba(245,158,11,0.12); color: #F59E0B;';
      default: return 'background: rgba(90,90,122,0.15); color: #8b8ba7;';
    }
  }
</script>

<section class="ctx-section" aria-labelledby="cast-title">
  <div class="ctx-section-header">
    <span class="ctx-section-title" id="cast-title">GROUP CAST</span>
    <div class="lore-header-actions">
      <span class="ctx-section-meta">{groupChars.length} characters</span>
      <button
        class="lore-add-btn"
        title="Add character to group"
        aria-label="Add character to group"
        onclick={() => { showAddChar = !showAddChar; if (!showAddChar) return; loadAllCharacters(); }}
      >
        <Icon name={showAddChar ? 'x' : 'user-plus'} size={13} color="var(--accent-primary)" />
      </button>
    </div>
  </div>

  <!-- Add Character Picker -->
  {#if showAddChar}
    <div class="cast-picker">
      {#if isLoadingAllChars}
        <div class="lore-loading">
          <span class="loading-dot"></span>
          <span class="loading-dot d2"></span>
          <span class="loading-dot d3"></span>
        </div>
      {:else if availableChars.length === 0}
        <div class="lore-empty">
          <Icon name="users" size={14} color="var(--fg-muted)" />
          <span>No characters available</span>
        </div>
      {:else}
        {#each availableChars as char (char.id)}
          <button
            class="cast-pick-item"
            onclick={() => addCharToGroup(char.id, char.name)}
            aria-label={`Add ${char.name}`}
          >
            <div class="cast-pick-avatar" aria-hidden="true">
              <span class="cast-pick-initial">{char.name.charAt(0)}</span>
            </div>
            <span class="cast-pick-name">{char.name}</span>
            <Icon name="plus" size={11} color="var(--accent-primary)" />
          </button>
        {/each}
      {/if}
    </div>
  {/if}

  <!-- Cast List -->
  {#if isLoadingCast}
    <div class="lore-loading">
      <span class="loading-dot"></span>
      <span class="loading-dot d2"></span>
      <span class="loading-dot d3"></span>
    </div>
  {:else if groupChars.length === 0}
    <div class="lore-empty">
      <Icon name="users" size={16} color="var(--fg-muted)" />
      <span>No group characters</span>
    </div>
  {:else}
    {#each groupChars as char (char.id)}
      <div class="cast-card" class:muted={!char.is_active}>
        <div class="cast-card-top">
          <div class="cast-avatar" aria-hidden="true">
            <span class="cast-avatar-initial">{char.character_name.charAt(0)}</span>
          </div>
          <div class="cast-info">
            <span class="cast-name">{char.character_name}</span>
            <span class="cast-role-badge" style={getRoleBadgeStyle(char.role)}>{char.role}</span>
          </div>
          <div class="cast-actions">
            <button
              class="cast-action-btn"
              title={char.is_active ? 'Mute character' : 'Unmute character'}
              aria-label={char.is_active ? `Mute ${char.character_name}` : `Unmute ${char.character_name}`}
              onclick={() => toggleCharActive(char.character_id)}
            >
              <Icon
                name={char.is_active ? 'volume-2' : 'volume-x'}
                size={12}
                color={char.is_active ? 'var(--accent-primary)' : 'var(--fg-muted)'}
              />
            </button>
            {#if char.role !== 'primary'}
              <button
                class="cast-action-btn cast-remove-btn"
                title="Remove from group"
                aria-label={`Remove ${char.character_name}`}
                onclick={() => removeCharFromGroup(char.character_id)}
              >
                <Icon name="x" size={10} color="var(--fg-muted)" />
              </button>
            {/if}
          </div>
        </div>
        <div class="cast-slider-row">
          <span class="cast-slider-label">Talk</span>
          <input
            type="range"
            min="0"
            max="100"
            value={char.talkativeness}
            class="cast-slider"
            aria-label={`Talkativeness for ${char.character_name}`}
            oninput={(e) => updateTalkativeness(char.character_id, Number((e.target as HTMLInputElement).value))}
          />
          <span class="cast-slider-value">{char.talkativeness}</span>
        </div>
      </div>
    {/each}
  {/if}
</section>

<style>
  .ctx-section { display: flex; flex-direction: column; gap: 10px; }
  .ctx-section-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-section-title {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.5px;
  }
  .ctx-section-meta { font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono); }

  .lore-header-actions { display: flex; align-items: center; gap: 8px; }
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

  /* ══ Group Cast ══ */

  .cast-picker {
    display: flex; flex-direction: column; gap: 2px;
    padding: 8px; border-radius: 12px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.1);
    max-height: 180px; overflow-y: auto;
  }
  .cast-picker::-webkit-scrollbar { width: 3px; }
  .cast-picker::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }

  .cast-pick-item {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 8px; border-radius: 8px;
    background: none; border: none; cursor: pointer;
    transition: background 150ms; width: 100%; text-align: left;
  }
  .cast-pick-item:hover { background: rgba(139,92,246,0.06); }

  .cast-pick-avatar {
    width: 24px; height: 24px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(135deg, rgba(139,92,246,0.3), rgba(191,64,255,0.3));
    display: flex; align-items: center; justify-content: center;
  }
  .cast-pick-initial {
    font-size: 10px; font-weight: 700; color: #c4a1ff;
    text-transform: uppercase;
  }
  .cast-pick-name {
    flex: 1; min-width: 0; font-size: 11px; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    font-family: var(--font-body);
  }

  .cast-card {
    display: flex; flex-direction: column; gap: 8px;
    padding: 10px 12px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: all 200ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .cast-card:hover { background: rgba(139,92,246,0.04); border-color: rgba(139,92,246,0.1); }
  .cast-card.muted { opacity: 0.5; }

  .cast-card-top {
    display: flex; align-items: center; gap: 8px;
  }

  .cast-avatar {
    width: 28px; height: 28px; min-width: 28px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 0 10px rgba(139,92,246,0.15);
  }
  .cast-avatar-initial {
    font-size: 11px; font-weight: 700; color: #fff;
    text-transform: uppercase;
  }

  .cast-info {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
  }
  .cast-name {
    font-size: var(--text-sm); font-weight: 600; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .cast-role-badge {
    padding: 2px 7px; border-radius: 99px;
    font-size: 9px; font-weight: 700; letter-spacing: 0.5px;
    text-transform: uppercase; flex-shrink: 0;
  }

  .cast-actions {
    display: flex; align-items: center; gap: 4px; flex-shrink: 0;
  }

  .cast-action-btn {
    background: none; border: none; padding: 4px; border-radius: 6px;
    cursor: pointer; display: flex; align-items: center; justify-content: center;
    transition: all 150ms; min-width: 24px; min-height: 24px;
  }
  .cast-action-btn:hover { background: rgba(139,92,246,0.08); }

  .cast-remove-btn {
    opacity: 0; transition: opacity 150ms;
  }
  .cast-card:hover .cast-remove-btn { opacity: 0.5; }
  .cast-remove-btn:hover { opacity: 1 !important; }

  .cast-slider-row {
    display: flex; align-items: center; gap: 8px;
    padding-left: 36px; /* offset past avatar */
  }
  .cast-slider-label {
    font-size: 9px; font-weight: 600; color: #4a4a6a;
    font-family: var(--font-mono); letter-spacing: 0.5px;
    text-transform: uppercase; flex-shrink: 0;
  }
  .cast-slider {
    flex: 1; height: 4px; -webkit-appearance: none; appearance: none;
    background: rgba(139,92,246,0.1); border-radius: 2px;
    outline: none; cursor: pointer;
  }
  .cast-slider::-webkit-slider-thumb {
    -webkit-appearance: none; appearance: none;
    width: 12px; height: 12px; border-radius: 50%;
    background: var(--accent-primary, #8B5CF6);
    border: 2px solid rgba(14,14,30,0.8);
    box-shadow: 0 0 6px rgba(139,92,246,0.3);
    cursor: pointer; transition: transform 150ms;
  }
  .cast-slider::-webkit-slider-thumb:hover { transform: scale(1.2); }
  .cast-slider::-moz-range-thumb {
    width: 12px; height: 12px; border-radius: 50%;
    background: var(--accent-primary, #8B5CF6);
    border: 2px solid rgba(14,14,30,0.8);
    box-shadow: 0 0 6px rgba(139,92,246,0.3);
    cursor: pointer;
  }
  .cast-slider-value {
    font-size: 10px; font-weight: 600; color: #5a5a7a;
    font-family: var(--font-mono); min-width: 22px; text-align: right;
  }
</style>
