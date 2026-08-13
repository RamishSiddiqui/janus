<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { onMount, onDestroy } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import { settings } from '$lib/stores/settings';
  import { loadFileAsBlobUrl, revokeIfSet } from '$lib/utils/blobUrl';
  import MemoryGraph from './MemoryGraph.svelte';
  import MemoryTimeline from './MemoryTimeline.svelte';
  import TimelineFilter from './TimelineFilter.svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import {
    sceneGenerations, getSceneGenerationState, trackPortraitGeneration,
    portraitGenerationKey, describeProgress, trackGenerationByKey,
  } from '$lib/stores/sceneGeneration';

  const refreshKey = (characterId: string) => `npc-refresh-${characterId}`;
  const uploadKey = (characterId: string) => `npc-upload-${characterId}`;

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let {
    conversationId = null,
    characterId = null,
    characterName = '',
    primaryAvatarUrl = null,
    additionalCharacters = [],
    onAttentionChange,
    wide = false,
  }: {
    conversationId?: string | null;
    /** The conversation's primary character — used for the one-time
     *  auto-migration seed below (ported from the old Group Cast panel)
     *  and to show its real portrait on the primary's roster card. */
    characterId?: string | null;
    characterName?: string;
    /** Primary character's portrait — Gallery-origin roster rows (primary/
     *  secondary) have no avatar_path of their own on the lightweight
     *  conversation_characters row, so the parent (which already resolved
     *  this for the header) passes it through rather than this panel
     *  re-fetching the full character record. */
    primaryAvatarUrl?: string | null;
    additionalCharacters?: { id: string; name: string; avatarUrl?: string | null }[];
    /** Fired whenever the "needs attention" state changes — lets a parent
     *  (e.g. a header trigger button) show a badge without this panel's
     *  popover being open. */
    onAttentionChange?: (hasAttention: boolean) => void;
    /** Renders cards in a responsive grid instead of a single stacked
     *  column — used when this panel fills the full chat area (see
     *  ChatExplorerView.svelte) rather than a narrow header popover. */
    wide?: boolean;
  } = $props();

  // ── Unified roster ──
  // This panel merges what used to be two separate header buttons: "Group
  // Cast" (conversation_characters roster — mute/talkativeness/add/remove,
  // any role) and "Cast" (auto-detected NPCs — review/portrait/promote).
  // `roster` is the single source of truth for who's in this conversation
  // and in what role; `npcDetails` layers the richer NPC-only fields
  // (avatar, description, portrait status) on top for npc/transient rows.
  interface RosterRow {
    id: string;
    character_id: string;
    character_name: string;
    role: string;
    talkativeness: number;
    is_active: boolean;
  }
  interface NpcDetail {
    id: string;
    name: string;
    avatar_path: string | null;
    data: Record<string, unknown>;
    origin: string;
    portrait_status: string;
    profile_reviewed: boolean;
  }

  let roster: RosterRow[] = $state([]);
  let npcDetails: Record<string, NpcDetail> = $state({});
  let isLoading = $state(false);
  let expandedId: string | null = $state(null);
  let editName = $state('');
  let editDescription = $state('');
  let editPersonality = $state('');
  let editScenario = $state('');
  let isSaving = $state(false);
  /** Same two exact strings `register_placeholder`/`register_transient_speaker`
   *  write in the Rust pipeline (see the matching constant in pipeline.rs) —
   *  a character still carrying one of these has never had a real profile
   *  written, which is what "Outdated" flags on the roster card. */
  const PLACEHOLDER_DESCRIPTIONS = [
    "Just arrived in the story — their role isn't clear yet.",
    "Just spoke for the first time — their role in the story isn't clear yet.",
  ];
  const isOutdated = (n: NpcDetail) => PLACEHOLDER_DESCRIPTIONS.includes(String(n.data?.description ?? '').trim());
  /** Full-size portrait lightbox — clicking a roster card's avatar opens
   *  this instead of toggling the card's own expand/collapse, which is
   *  what clicking anywhere else on the card row does. */
  let previewAvatar: { url: string; name: string } | null = $state(null);

  // Role sort order for display: primary first, then secondary, npc, transient.
  const ROLE_ORDER: Record<string, number> = { primary: 0, secondary: 1, npc: 2, transient: 3 };
  let sortedRoster = $derived(
    [...roster].sort((a, b) => (ROLE_ORDER[a.role] ?? 9) - (ROLE_ORDER[b.role] ?? 9))
  );

  // ── Add Character (ported from the old Group Cast panel) ──
  let showAddChar = $state(false);
  let allCharacters: { id: string; name: string }[] = $state([]);
  let isLoadingAllChars = $state(false);
  let availableChars = $derived.by(() => {
    const inRoster = new Set(roster.map(r => r.character_id));
    if (characterId) inRoster.add(characterId);
    return allCharacters.filter(c => !inRoster.has(c.id));
  });

  async function loadAllCharacters() {
    if (allCharacters.length > 0) return;
    isLoadingAllChars = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const chars = await ipc.listCharacters();
      allCharacters = chars.map(c => ({ id: c.id, name: c.name }));
    } catch (err) {
      console.error('Failed to load characters:', err);
    }
    isLoadingAllChars = false;
  }

  async function addCharToRoster(charId: string, charName: string) {
    if (!conversationId || !isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const created = await ipc.addConversationCharacter(conversationId, charId, charName);
      roster = [...roster, {
        id: created.id,
        character_id: created.character_id,
        character_name: created.character_name,
        role: created.role,
        talkativeness: created.talkativeness,
        is_active: created.is_active,
      }];
      showAddChar = false;
      success(`${charName} joined the cast`);
    } catch {
      toastError('Failed to add character');
    }
  }

  async function removeFromRoster(charId: string) {
    if (!conversationId || !isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.removeConversationCharacter(conversationId, charId);
      const name = roster.find(r => r.character_id === charId)?.character_name ?? 'Character';
      roster = roster.filter(r => r.character_id !== charId);
      success(`${name} removed`);
    } catch {
      toastError('Failed to remove character');
    }
  }

  async function toggleRosterActive(charId: string) {
    if (!conversationId || !isTauri) return;
    const row = roster.find(r => r.character_id === charId);
    if (!row) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.toggleCharacterActive(conversationId, charId, !row.is_active);
      roster = roster.map(r => r.character_id === charId ? { ...r, is_active: !r.is_active } : r);
    } catch {
      toastError('Failed to toggle character');
    }
  }

  async function updateRosterTalkativeness(charId: string, value: number) {
    if (!conversationId || !isTauri) return;
    roster = roster.map(r => r.character_id === charId ? { ...r, talkativeness: value } : r);
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

  /** Resolves a roster row's portrait, in priority order: a freshly
   *  fetched/generated character record (avatarUrls, blob-resolved from
   *  npcDetails — covers auto-detected AND any gallery character that's
   *  been expanded at least once), then whichever avatar prop the parent
   *  already had on hand (header's primary portrait, or the additional-
   *  characters list), so nobody ever falls back to the placeholder icon
   *  unless there's truly no portrait anywhere yet. */
  function avatarFor(rowCharacterId: string): string | null {
    if (avatarUrls[rowCharacterId]) return avatarUrls[rowCharacterId];
    if (characterId && rowCharacterId === characterId) return primaryAvatarUrl;
    return additionalCharacters.find(c => c.id === rowCharacterId)?.avatarUrl ?? null;
  }

  // ── Cast Graph (memories shared between this conversation's characters) ──
  // Defaults to the roster list; switching to graph/timeline mirrors the
  // standalone Memories page's own view (same stats strip, same Graph/
  // Timeline toggle, same components) instead of a separate small popup.
  let castView: 'roster' | 'graph' | 'timeline' = $state('roster');
  let castGraphData = $state<MemoryGraphData | null>(null);
  let isLoadingCastGraph = $state(false);

  let castTotalMemories = $derived(castGraphData?.memories?.length ?? 0);
  let castCanonCount = $derived(castGraphData?.memories?.filter(m => m.is_canon).length ?? 0);
  let castLinkCount = $derived(castGraphData?.links?.length ?? 0);
  let castTimelineCount = $derived(castGraphData?.conversations?.length ?? 0);

  // ── Timeline/graph visibility filter — shared between both views so
  // switching Graph <-> Timeline doesn't lose the current selection. ──
  let castConvOptions = $derived((castGraphData?.conversations ?? []).map(c => ({ id: c.id, title: c.title })));
  let selectedCastConvIds = $state<Set<string>>(new Set());
  let knownCastConvIdsKey = $state('');

  $effect(() => {
    const key = castConvOptions.map(c => c.id).sort().join('|');
    if (key !== knownCastConvIdsKey) {
      knownCastConvIdsKey = key;
      selectedCastConvIds = new Set(castConvOptions.map(c => c.id));
    }
  });

  function toggleCastConvFilter(id: string): void {
    const next = new Set(selectedCastConvIds);
    if (next.has(id)) next.delete(id); else next.add(id);
    selectedCastConvIds = next;
  }

  function toggleCastConvFilterAll(): void {
    selectedCastConvIds = selectedCastConvIds.size === castConvOptions.length
      ? new Set()
      : new Set(castConvOptions.map(c => c.id));
  }

  async function openCastGraph() {
    castView = 'graph';
    if (!conversationId || !isTauri) return;
    isLoadingCastGraph = true;
    try {
      const ipc = await import('$lib/services/ipc');
      castGraphData = await ipc.getCastMemoryGraph(conversationId);
    } catch (err) {
      console.error('Failed to load cast memory graph:', err);
      castGraphData = null;
    }
    isLoadingCastGraph = false;
  }

  // avatarUrls: characterId -> blob url currently shown.
  // avatarPathLoaded: characterId -> the avatar_path that blob url was built
  // from, so we only reload when the path actually changes.
  let avatarUrls: Record<string, string> = $state({});
  const avatarPathLoaded: Record<string, string> = {};

  // Portraits for the cast memory graph — MemoryGraph takes an `avatars`
  // map (character_id -> url) to render on its character nodes; without it
  // every node falls back to a generic person icon, primary included, even
  // though we already have (or can resolve) a real portrait for everyone
  // in the roster.
  let castAvatars = $derived.by(() => {
    const map: Record<string, string | null> = {};
    for (const row of roster) {
      map[row.character_id] = avatarFor(row.character_id);
    }
    return map;
  });

  const needsAttention = (n: NpcDetail) => !n.profile_reviewed || n.portrait_status === 'pending_review';
  let anyNeedsAttention = $derived(Object.values(npcDetails).some(needsAttention));
  $effect(() => {
    onAttentionChange?.(anyNeedsAttention);
  });

  $effect(() => {
    const _conv = conversationId;
    if (_conv && isTauri) {
      loadCast(_conv);
    } else {
      roster = [];
      npcDetails = {};
    }
  });

  $effect(() => {
    for (const npc of Object.values(npcDetails)) {
      const path = npc.avatar_path;
      if (path && avatarPathLoaded[npc.id] !== path) {
        avatarPathLoaded[npc.id] = path;
        loadFileAsBlobUrl(path).then(url => {
          revokeIfSet(avatarUrls[npc.id]);
          avatarUrls = { ...avatarUrls, [npc.id]: url };
        }).catch(() => { /* falls back to initial-circle placeholder */ });
      }
    }
  });

  // Picks up a portrait generation that finished while this panel was
  // unmounted (switched to another explorer view, or closed back to chat
  // and reopened) — the job itself keeps running server-side regardless,
  // but a fresh mount's `npcDetails` starts empty, so without this the new
  // avatar_path would never get pulled in until the user happened to
  // expand that card again. `completedAt` exists on the store specifically
  // so a consumer can react to a completion it wasn't around to see fire —
  // same pattern SceneDisplay.svelte uses for scene generation.
  const handledPortraitCompletion: Record<string, number> = {};
  $effect(() => {
    if (!isTauri) return;
    for (const row of roster) {
      const state = getSceneGenerationState($sceneGenerations, portraitGenerationKey(row.character_id));
      if (state.completedAt && handledPortraitCompletion[row.character_id] !== state.completedAt) {
        handledPortraitCompletion[row.character_id] = state.completedAt;
        refreshCharacterDetail(row.character_id);
      }
    }
  });

  async function refreshCharacterDetail(characterId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const full = await ipc.getCharacter(characterId);
      npcDetails = { ...npcDetails, [characterId]: full as unknown as NpcDetail };
    } catch {
      // Best-effort — the card just won't refresh until manually re-expanded.
    }
  }

  onDestroy(() => {
    for (const url of Object.values(avatarUrls)) revokeIfSet(url);
  });

  onMount(() => {
    if (!isTauri) return;
    // This panel is deliberately mounted more than once at a time (a
    // permanently-hidden instance in ChatHeader for the "needs attention"
    // dot, plus a full instance whenever the Cast explorer tab is open) —
    // without returning/calling the unlisten function, every open of the
    // Cast tab registered a fresh listener that outlived the tab being
    // closed, so listeners accumulated for the rest of the session and each
    // npc_created event triggered loadCast (and its auto-migration side
    // effects) once per accumulated listener.
    let unlisten: (() => void) | undefined;
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<{ conversation_id: string }>('npc_created', (event) => {
        if (conversationId && event.payload.conversation_id === conversationId) {
          loadCast(conversationId);
        }
      }).then(fn => { unlisten = fn; });
    });
    return () => unlisten?.();
  });

  async function loadCast(convId: string) {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      let castRows = await ipc.listConversationCharacters(convId);

      // ── Auto-migration ──
      // Every conversation with a primary character should have at least
      // one 'primary' row here — solo conversations included, not just
      // group ones. Previously this only seeded when `additionalCharacters`
      // was non-empty, so any single-character conversation's roster
      // stayed permanently empty (nothing to show but "no characters yet"
      // even mid-story) until a second character joined.
      const hasPrimary = castRows.some(c => c.role === 'primary');
      if (!hasPrimary && characterId && characterName) {
        try {
          await ipc.addConversationCharacter(convId, characterId, characterName, 'primary', 70);
        } catch { /* may already exist */ }
        for (const ac of additionalCharacters) {
          try {
            await ipc.addConversationCharacter(convId, ac.id, ac.name, 'secondary', 50);
          } catch { /* may already exist */ }
        }
        castRows = await ipc.listConversationCharacters(convId);
      }

      roster = castRows.map(c => ({
        id: c.id,
        character_id: c.character_id,
        character_name: c.character_name,
        role: c.role,
        talkativeness: c.talkativeness,
        is_active: c.is_active,
      }));

      const npcResult = await ipc.listConversationNpcs(convId);
      npcDetails = Object.fromEntries(
        (npcResult as unknown as NpcDetail[]).map(n => [n.id, n])
      );
    } catch (err) {
      console.error('Failed to load cast:', err);
      roster = [];
      npcDetails = {};
    }
    isLoading = false;
  }

  async function generatePortrait(npc: NpcDetail) {
    if (!isTauri || !conversationId) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const updated = await trackPortraitGeneration(npc.id, () =>
        ipc.generateNpcPortrait(npc.id, conversationId!, $settings.autoApproveNpcPortraits)
      );
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, avatar_path: (updated as unknown as NpcDetail).avatar_path, portrait_status: (updated as unknown as NpcDetail).portrait_status } };
      if (!npcDetails[npc.id].avatar_path) {
        toastError('No image provider configured — add one in Settings → Models');
      } else {
        success(npcDetails[npc.id].portrait_status === 'pending_review' ? 'Portrait generated — awaiting your approval' : 'Portrait generated');
      }
    } catch (err) {
      toastError((err as { message?: string })?.message || 'Failed to generate portrait');
    }
  }

  /** "Upload Portrait" — bypasses AI generation, sets the image directly.
   *  The file dialog itself isn't tracked as "busy" (it's a blocking native
   *  picker the user is actively driving, not background work) — only the
   *  actual upload, via the same remount-safe global-store tracking the
   *  AI-generated portrait flow uses, once a file's been picked. */
  async function uploadPortrait(npc: NpcDetail) {
    if (!isTauri) return;
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
    });
    if (!selected) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const updated = await trackGenerationByKey(uploadKey(npc.id), () => ipc.uploadCharacterAvatar(npc.id, selected as string));
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, avatar_path: (updated as unknown as NpcDetail).avatar_path, portrait_status: (updated as unknown as NpcDetail).portrait_status } };
      delete avatarPathLoaded[npc.id];
      success('Portrait uploaded');
    } catch {
      toastError('Failed to upload portrait');
    }
  }

  async function approvePortrait(npc: NpcDetail) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.approveNpcPortrait(npc.id);
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, portrait_status: 'approved' } };
      success('Portrait approved');
    } catch {
      toastError('Failed to approve portrait');
    }
  }

  async function rejectPortrait(npc: NpcDetail) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.rejectNpcPortrait(npc.id);
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, avatar_path: null, portrait_status: 'approved' } };
      delete avatarPathLoaded[npc.id];
      revokeIfSet(avatarUrls[npc.id]);
      const { [npc.id]: _removed, ...rest } = avatarUrls;
      avatarUrls = rest;
      success('Portrait rejected');
    } catch {
      toastError('Failed to reject portrait');
    }
  }

  let loadingDetailId: string | null = $state(null);

  async function toggleExpand(row: RosterRow) {
    if (expandedId === row.character_id) {
      expandedId = null;
      return;
    }
    expandedId = row.character_id;

    // Gallery-origin rows (primary/secondary) aren't covered by
    // list_conversation_npcs — fetch the full character record on first
    // expand so they get the exact same edit/portrait UI as auto-detected
    // ones instead of being dead-ended at "View in Gallery" only.
    if (!npcDetails[row.character_id] && isTauri) {
      loadingDetailId = row.character_id;
      try {
        const ipc = await import('$lib/services/ipc');
        const full = await ipc.getCharacter(row.character_id);
        npcDetails = { ...npcDetails, [row.character_id]: full as unknown as NpcDetail };
      } catch {
        toastError('Failed to load character details');
        loadingDetailId = null;
        return;
      }
      loadingDetailId = null;
    }

    const npc = npcDetails[row.character_id];
    if (!npc) return;
    const data = npc.data ?? {};
    editName = npc.name;
    editDescription = String(data.description ?? '');
    editPersonality = String(data.personality ?? '');
    editScenario = String(data.scenario ?? '');

    if (!npc.profile_reviewed && isTauri) {
      try {
        const ipc = await import('$lib/services/ipc');
        await ipc.markNpcReviewed(npc.id);
        npcDetails = { ...npcDetails, [npc.id]: { ...npc, profile_reviewed: true } };
      } catch {
        // Non-critical — the dot just won't clear this time.
      }
    }
  }

  async function saveProfile(npc: NpcDetail) {
    if (!isTauri) return;
    isSaving = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const mergedData = {
        ...npc.data,
        description: editDescription,
        personality: editPersonality,
        scenario: editScenario,
      };
      const trimmedName = editName.trim();
      const nameChanged = trimmedName.length > 0 && trimmedName !== npc.name;
      await ipc.updateCharacter(npc.id, nameChanged ? trimmedName : undefined, mergedData);
      const newName = nameChanged ? trimmedName : npc.name;
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, data: mergedData, name: newName } };
      if (nameChanged) {
        roster = roster.map(r => r.character_id === npc.id ? { ...r, character_name: newName } : r);
      }
      success(`${newName}'s profile updated`);
    } catch {
      toastError('Failed to update profile');
    }
    isSaving = false;
  }

  async function promote(npc: NpcDetail) {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.promoteNpcToGallery(npc.id);
      npcDetails = { ...npcDetails, [npc.id]: { ...npc, origin: 'gallery' } };
      success(`${npc.name} promoted to your Character Gallery`);
    } catch {
      toastError('Failed to promote character');
    }
  }

  /** Manual override for the "Unconfirmed" badge — promotes straight to
   *  role: 'npc' without waiting on the automatic detector debounce, which
   *  can leave an obviously-recurring character stuck as "Unconfirmed"
   *  indefinitely with no visible reason (see confirm_npc's doc comment). */
  async function confirmNpc(npc: NpcDetail) {
    if (!isTauri || !conversationId) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.confirmNpc(conversationId, npc.id);
      roster = roster.map(r => r.character_id === npc.id ? { ...r, role: 'npc' } : r);
      success(`${npc.name} confirmed as a cast member`);
    } catch {
      toastError('Failed to confirm character');
    }
  }

  /** Refines description/personality/scenario against how this character has
   *  actually appeared in the conversation so far — the manual counterpart
   *  to the automatic still-placeholder trigger in the detection pipeline.
   *  A character shared across multiple conversations gets the refresh
   *  saved as a memory instead of an edit to the shared card (see the
   *  backend's `ProfileRefreshResult.scope` — surfaced here via the toast
   *  copy so the "nothing changed on screen" case doesn't read as a no-op). */
  async function refreshFromStory(npc: NpcDetail) {
    if (!isTauri || !conversationId) return;
    try {
      const ipc = await import('$lib/services/ipc');
      const result = await trackGenerationByKey(refreshKey(npc.id), () =>
        ipc.refreshCharacterProfile(npc.id, conversationId!, $settings.profileRefreshPrompt)
      );
      if (result.scope === 'character') {
        const updated = result.character;
        const updatedData = (updated.data ?? {}) as Record<string, unknown>;
        npcDetails = { ...npcDetails, [npc.id]: { ...npc, data: updatedData, name: updated.name, profile_reviewed: updated.profile_reviewed ?? npc.profile_reviewed } };
        editDescription = (updatedData.description as string) ?? editDescription;
        editPersonality = (updatedData.personality as string) ?? editPersonality;
        editScenario = (updatedData.scenario as string) ?? editScenario;
        success(`${npc.name}'s profile refreshed from the story`);
      } else {
        success(`${npc.name} appears in other conversations too, so this refresh was saved as a memory for this story instead of changing their shared profile`);
      }
    } catch {
      toastError('Failed to refresh profile from story');
    }
  }
</script>

<section class="ctx-section" class:cast-graph-open={wide && castView !== 'roster'} aria-labelledby="npc-title">
  <div class="ctx-section-header">
    <span class="ctx-section-title" id="npc-title">
      CAST
      {#if anyNeedsAttention}<span class="npc-attention-dot" aria-label="Needs attention"></span>{/if}
    </span>
    {#if castView === 'roster'}
      <div class="lore-header-actions">
        <span class="ctx-section-meta">{roster.length} character{roster.length === 1 ? '' : 's'}</span>
        {#if conversationId}
          <button
            class="cast-action-btn"
            class:danger={showAddChar}
            title={showAddChar ? 'Close the add-character picker' : "Add a character to this conversation's cast"}
            aria-label={showAddChar ? 'Close add character picker' : 'Add character to cast'}
            onclick={() => { showAddChar = !showAddChar; if (!showAddChar) return; loadAllCharacters(); }}
          >
            <Icon name={showAddChar ? 'x' : 'user-plus'} size={12} color={showAddChar ? '#f87171' : 'var(--accent-primary)'} />
            <span>{showAddChar ? 'Close' : 'Add'}</span>
          </button>
          <button
            class="cast-action-btn"
            title="View the memory graph shared between this cast"
            aria-label="View cast memory graph"
            onclick={openCastGraph}
          >
            <Icon name="network" size={12} color="var(--accent-primary)" />
            <span>Graph</span>
          </button>
        {/if}
      </div>
    {:else}
      <div class="lore-header-actions">
        <div class="cast-view-switch">
          <button class="cast-switch-btn" class:active={castView === 'graph'} onclick={() => castView = 'graph'}>
            <Icon name="network" size={12} color={castView === 'graph' ? '#e8e0ff' : '#5a5a7a'} />
            <span>Graph</span>
          </button>
          <button class="cast-switch-btn" class:active={castView === 'timeline'} onclick={() => castView = 'timeline'}>
            <Icon name="clock" size={12} color={castView === 'timeline' ? '#e8e0ff' : '#5a5a7a'} />
            <span>Timeline</span>
          </button>
          <div class="cast-switch-indicator" class:right={castView === 'timeline'}></div>
        </div>
        <button class="lore-add-btn" title="Refresh" aria-label="Refresh cast graph" onclick={openCastGraph} disabled={isLoadingCastGraph}>
          <Icon name="refresh-cw" size={12} color="var(--accent-primary)" />
        </button>
        <button class="lore-add-btn" title="Back to Cast" aria-label="Back to Cast list" onclick={() => castView = 'roster'}>
          <Icon name="x" size={12} color="var(--fg-muted)" />
        </button>
      </div>
    {/if}
  </div>

  <!-- Add Character Picker -->
  {#if castView === 'roster' && showAddChar}
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
            onclick={() => addCharToRoster(char.id, char.name)}
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

  {#if castView !== 'roster'}
    {#if castTotalMemories > 0}
      <div class="cast-stats-strip">
        <div class="cast-stat">
          <span class="cast-stat-value">{castTotalMemories}</span>
          <span class="cast-stat-label">Memories</span>
        </div>
        <div class="cast-stat">
          <span class="cast-stat-value cast-canon">{castCanonCount}</span>
          <span class="cast-stat-label">Canon</span>
        </div>
        <div class="cast-stat">
          <span class="cast-stat-value">{castTimelineCount}</span>
          <span class="cast-stat-label">Timelines</span>
        </div>
        <div class="cast-stat">
          <span class="cast-stat-value cast-link">{castLinkCount}</span>
          <span class="cast-stat-label">Links</span>
        </div>
        <TimelineFilter
          conversations={castConvOptions}
          selected={selectedCastConvIds}
          onToggle={toggleCastConvFilter}
          onToggleAll={toggleCastConvFilterAll}
        />
      </div>
    {/if}

    <div class="cast-graph-canvas" class:wide>
      {#if isLoadingCastGraph}
        <div class="lore-empty"><span>Loading…</span></div>
      {:else if !castGraphData || (castGraphData.memories.length === 0 && (castGraphData.characters?.length ?? 0) === 0)}
        <div class="lore-empty">
          <Icon name="network" size={16} color="var(--fg-muted)" />
          <span>No cast memories to graph yet</span>
        </div>
      {:else if castView === 'graph'}
        <MemoryGraph data={castGraphData} avatars={castAvatars} onRefresh={openCastGraph} visibleConvIds={selectedCastConvIds} />
      {:else}
        <MemoryTimeline data={castGraphData} onRefresh={openCastGraph} visibleConvIds={selectedCastConvIds} />
      {/if}
    </div>
  {:else if isLoading}
    <div class="lore-loading">
      <span class="loading-dot"></span>
      <span class="loading-dot d2"></span>
      <span class="loading-dot d3"></span>
    </div>
  {:else if roster.length === 0}
    <div class="lore-empty">
      <Icon name="users" size={16} color="var(--fg-muted)" />
      <span>No characters in this conversation yet</span>
    </div>
  {:else}
    <div class="cards-grid" class:wide>
    {#each sortedRoster as row (row.character_id)}
      {@const npc = npcDetails[row.character_id]}
      {@const isExpanded = expandedId === row.character_id}
      {@const isGalleryOrigin = npc ? npc.origin === 'gallery' : (row.role === 'primary' || row.role === 'secondary')}
      {@const portrait = avatarFor(row.character_id)}
      <!-- One unified card for every roster row — hand-picked Gallery
           characters and auto-detected NPCs alike get the same portrait/
           edit affordances; only which actions apply differs by role. -->
      <div class="npc-card" class:expanded-span={wide && isExpanded} class:muted={!row.is_active} class:is-open={isExpanded}>
        <div
          class="npc-card-top"
          onclick={() => toggleExpand(row)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleExpand(row); } }}
          role="button"
          tabindex="0"
          aria-expanded={isExpanded}
        >
          <button
            class="cast-avatar"
            class:cast-avatar-clickable={!!portrait}
            disabled={!portrait}
            title={portrait ? `View ${row.character_name}'s portrait` : undefined}
            aria-label={portrait ? `View ${row.character_name}'s portrait` : `${row.character_name}'s portrait placeholder`}
            onclick={(e) => { e.stopPropagation(); if (portrait) previewAvatar = { url: portrait, name: row.character_name }; }}
          >
            {#if portrait}
              <img src={portrait} alt="" class="cast-avatar-img" />
            {:else}
              <span class="cast-avatar-initial">{row.character_name.charAt(0)}</span>
            {/if}
          </button>
          <div class="npc-info">
            <span class="cast-name">{row.character_name}</span>
            {#if row.role === 'primary' || row.role === 'secondary'}
              <span class="cast-role-badge" style={getRoleBadgeStyle(row.role)}>{row.role}</span>
            {:else if isGalleryOrigin}
              <span class="npc-badge promoted">Promoted</span>
            {:else if row.role === 'transient'}
              <span class="npc-badge unconfirmed" title="Just started speaking — not yet confirmed as a recurring character">Unconfirmed</span>
            {/if}
            {#if npc?.data?.identity_concealed}
              <span class="npc-badge concealed" title="Identity not yet revealed by the story">Concealed</span>
            {/if}
            {#if npc && isOutdated(npc)}
              <span class="npc-badge outdated" title="Still carrying the placeholder written when first spotted — try Refresh from Story">Outdated</span>
            {/if}
            {#if npc && needsAttention(npc)}<span class="npc-attention-dot" aria-label="Needs attention"></span>{/if}
          </div>
          {#if loadingDetailId === row.character_id}
            <span class="npc-spinner" aria-hidden="true"></span>
          {:else}
            <Icon name={isExpanded ? 'chevron-up' : 'chevron-down'} size={13} color="var(--fg-muted)" />
          {/if}
        </div>

        <div class="cast-slider-row">
          <button
            class="cast-action-btn"
            title={row.is_active ? 'Mute character' : 'Unmute character'}
            aria-label={row.is_active ? `Mute ${row.character_name}` : `Unmute ${row.character_name}`}
            onclick={() => toggleRosterActive(row.character_id)}
          >
            <Icon name={row.is_active ? 'volume-2' : 'volume-x'} size={12} color={row.is_active ? 'var(--accent-primary)' : 'var(--fg-muted)'} />
          </button>
          <span class="cast-slider-label">Talk</span>
          <input
            type="range" min="0" max="100" value={row.talkativeness} class="cast-slider"
            aria-label={`Talkativeness for ${row.character_name}`}
            oninput={(e) => updateRosterTalkativeness(row.character_id, Number((e.target as HTMLInputElement).value))}
          />
          <span class="cast-slider-value">{row.talkativeness}</span>
          {#if isGalleryOrigin}
            <button
              class="cast-action-btn"
              title="View in Gallery"
              aria-label={`View ${row.character_name} in Gallery`}
              onclick={() => goto(`/gallery/${row.character_id}`)}
            >
              <Icon name="chevron-right" size={12} color="var(--fg-muted)" />
            </button>
          {/if}
          {#if row.role !== 'primary'}
            <button
              class="cast-action-btn cast-remove-btn"
              title="Remove from cast"
              aria-label={`Remove ${row.character_name}`}
              onclick={() => removeFromRoster(row.character_id)}
            >
              <Icon name="x" size={10} color="var(--fg-muted)" />
            </button>
          {/if}
        </div>

        {#if isExpanded}
          <div class="npc-expand" transition:slide={{ duration: 220, easing: cubicOut }}>
            {#if !npc}
              <div class="npc-expand-loading">
                <span class="loading-dot"></span>
                <span class="loading-dot d2"></span>
                <span class="loading-dot d3"></span>
              </div>
            {:else}
              <!-- Generation status reads purely from the global sceneGenerations
                   store, keyed per-character — never from local component state,
                   which resets on remount. This panel unmounts/remounts on every
                   explorer-view switch (Cast -> Lore -> Cast, or closing back to
                   chat and reopening), so a locally-tracked "isGenerating" flag
                   would silently forget an in-flight generation the moment you
                   navigated away and back, even though the job itself kept running
                   server-side. -->
              {@const portraitState = getSceneGenerationState($sceneGenerations, portraitGenerationKey(npc.id))}
              {@const uploadState = getSceneGenerationState($sceneGenerations, uploadKey(npc.id))}
              {@const refreshState = getSceneGenerationState($sceneGenerations, refreshKey(npc.id))}
              {@const portraitBusy = portraitState.isLoading || uploadState.isLoading}
              <div class="npc-sheet">
                <div class="npc-sheet-portrait">
                  <div class="npc-sheet-frame" class:has-image={!!portrait}>
                    <button
                      class="npc-sheet-frame-btn"
                      disabled={!portrait}
                      title={portrait ? `View ${npc.name}'s portrait` : undefined}
                      aria-label={portrait ? `View ${npc.name}'s portrait` : `${npc.name}'s portrait placeholder`}
                      onclick={() => { if (portrait) previewAvatar = { url: portrait, name: npc.name }; }}
                    >
                      {#if portrait}
                        <img src={portrait} alt="" class="npc-sheet-frame-img" />
                      {:else}
                        <span class="npc-sheet-frame-initial">{npc.name.charAt(0)}</span>
                      {/if}
                    </button>
                    {#if portraitState.isLoading}
                      <div class="npc-sheet-frame-overlay">
                        <span class="npc-portrait-status-dot"></span>
                      </div>
                    {/if}
                  </div>
                  <div class="npc-sheet-portrait-actions">
                    {#if npc.portrait_status === 'pending_review'}
                      <button class="npc-icon-btn" onclick={() => approvePortrait(npc)} disabled={portraitBusy} title="Approve portrait" aria-label="Approve portrait">
                        <Icon name="check" size={14} color="currentColor" />
                      </button>
                      <button class="npc-icon-btn" onclick={() => generatePortrait(npc)} disabled={portraitBusy} title="Regenerate portrait" aria-label="Regenerate portrait">
                        <Icon name="refresh-cw" size={14} color="currentColor" />
                      </button>
                      <button class="npc-icon-btn npc-icon-btn-danger" onclick={() => rejectPortrait(npc)} disabled={portraitBusy} title="Reject portrait" aria-label="Reject portrait">
                        <Icon name="x" size={14} color="currentColor" />
                      </button>
                    {:else}
                      <button class="npc-icon-btn" onclick={() => generatePortrait(npc)} disabled={portraitBusy} title={npc.avatar_path ? 'Regenerate portrait' : 'Generate portrait'} aria-label={npc.avatar_path ? 'Regenerate portrait' : 'Generate portrait'}>
                        <Icon name="image" size={14} color="currentColor" />
                      </button>
                    {/if}
                    <button class="npc-icon-btn" onclick={() => uploadPortrait(npc)} disabled={portraitBusy} title="Upload portrait" aria-label="Upload portrait">
                      <Icon name="upload" size={14} color="currentColor" />
                    </button>
                  </div>
                  {#if portraitState.isLoading}
                    <div class="npc-portrait-status" transition:slide={{ duration: 180, easing: cubicOut }}>
                      <span>{describeProgress(portraitState.progress)}</span>
                    </div>
                  {/if}
                </div>

                <div class="npc-sheet-fields">
                  <label class="npc-sheet-name-label" for={`npc-name-${npc.id}`}>
                    <input
                      id={`npc-name-${npc.id}`} class="npc-sheet-name-input" type="text" bind:value={editName}
                      placeholder="Rename once the story reveals who they really are"
                    />
                  </label>

                  <div class="npc-sheet-field">
                    <div class="npc-sheet-field-label"><Icon name="book-open" size={12} color="currentColor" /><span>Description</span></div>
                    <textarea id={`npc-desc-${npc.id}`} class="npc-sheet-textarea" rows="4" bind:value={editDescription}></textarea>
                  </div>

                  <div class="npc-sheet-field">
                    <div class="npc-sheet-field-label"><Icon name="brain" size={12} color="currentColor" /><span>Personality</span></div>
                    <textarea id={`npc-pers-${npc.id}`} class="npc-sheet-textarea" rows="2" bind:value={editPersonality}></textarea>
                  </div>

                  <div class="npc-sheet-field">
                    <div class="npc-sheet-field-label"><Icon name="map-pin" size={12} color="currentColor" /><span>Scenario</span></div>
                    <textarea id={`npc-scen-${npc.id}`} class="npc-sheet-textarea" rows="2" bind:value={editScenario}></textarea>
                  </div>
                </div>
              </div>

              <div class="npc-expand-actions">
                <button class="npc-btn npc-btn-primary" onclick={() => saveProfile(npc)} disabled={isSaving}>
                  <Icon name="check" size={13} color="currentColor" />
                  <span>{isSaving ? 'Saving…' : 'Save'}</span>
                </button>
                <button class="npc-btn npc-btn-magic" onclick={() => refreshFromStory(npc)} disabled={refreshState.isLoading} title="Re-derive description/personality/scenario from how this character has actually appeared in the story so far">
                  <Icon name="refresh-cw" size={13} color="currentColor" />
                  <span>{refreshState.isLoading ? 'Refreshing…' : 'Refresh from Story'}</span>
                </button>
                {#if row.role === 'transient'}
                  <button class="npc-btn npc-btn-confirm" onclick={() => confirmNpc(npc)} title="Skip the automatic detection wait and confirm this character as a cast member now">
                    <Icon name="check" size={13} color="currentColor" />
                    <span>Confirm</span>
                  </button>
                {/if}
                {#if npc.origin !== 'gallery'}
                  <button class="npc-btn npc-btn-promote" onclick={() => promote(npc)}>
                    <Icon name="sparkles" size={13} color="currentColor" />
                    <span>Promote to Gallery</span>
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
    </div>
  {/if}
</section>

{#if previewAvatar}
  <div
    class="avatar-lightbox-backdrop"
    onclick={() => previewAvatar = null}
    onkeydown={(e) => e.key === 'Escape' && (previewAvatar = null)}
    role="dialog"
    aria-modal="true"
    aria-label={`${previewAvatar.name}'s portrait`}
    tabindex="-1"
  >
    <button class="avatar-lightbox-close" onclick={() => previewAvatar = null} aria-label="Close">
      <Icon name="x" size={18} color="#e8e0ff" />
    </button>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img
      src={previewAvatar.url}
      alt={`${previewAvatar.name}'s portrait`}
      class="avatar-lightbox-img"
      onclick={(e) => e.stopPropagation()}
    />
    <span class="avatar-lightbox-name">{previewAvatar.name}</span>
  </div>
{/if}

<style>
  .ctx-section { display: flex; flex-direction: column; gap: 10px; }
  /* When the wide (full explorer-view) cast graph/timeline is open, stretch
     this section to fill the explorer body's remaining height instead of
     leaving the fixed-height canvas below stranded above dead space —
     relies on ChatExplorerView's `.explorer-body` also being a flex column
     so this `flex: 1` has a definite height to resolve against. */
  .ctx-section.cast-graph-open { flex: 1; min-height: 0; }
  .ctx-section-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-section-title {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.5px;
    display: flex; align-items: center; gap: 6px;
  }
  .ctx-section-meta { font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono); }

  .lore-header-actions { display: flex; align-items: center; gap: 8px; }

  /* ── Add Character picker (ported from Group Cast) ── */
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
    width: clamp(22px, 6cqi, 30px); height: clamp(22px, 6cqi, 30px); border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(135deg, rgba(139,92,246,0.3), rgba(191,64,255,0.3));
    display: flex; align-items: center; justify-content: center;
  }
  .cast-pick-initial { font-size: 10px; font-weight: 700; color: #c4a1ff; text-transform: uppercase; }
  .cast-pick-name {
    flex: 1; min-width: 0; font-size: 11px; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    font-family: var(--font-body);
  }

  /* ── Cast Graph: view switch + stats strip (mirrors /memories page) ── */
  .cast-view-switch {
    display: flex; position: relative;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.08);
    border-radius: 8px; padding: 2px;
  }
  .cast-switch-btn {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 10px; font-size: 11px; font-weight: 600; color: #5a5a7a;
    background: none; border: none; cursor: pointer; position: relative; z-index: 1;
    transition: color 250ms; font-family: var(--font-body); white-space: nowrap;
  }
  .cast-switch-btn.active { color: #e8e0ff; }
  .cast-switch-indicator {
    position: absolute; top: 2px; left: 2px;
    width: calc(50% - 2px); height: calc(100% - 4px);
    background: rgba(139,92,246,0.14); border: 1px solid rgba(139,92,246,0.18);
    border-radius: 6px; transition: transform 300ms cubic-bezier(0.4,0,0.2,1);
  }
  .cast-switch-indicator.right { transform: translateX(100%); }

  .cast-stats-strip {
    display: flex; align-items: center; gap: 0;
    padding: 8px 4px; border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
  }
  .cast-stat { display: flex; align-items: center; gap: 6px; padding: 0 clamp(10px, 3cqi, 16px); }
  .cast-stat-value { font-size: 13px; font-weight: 700; color: #c4a1ff; font-family: var(--font-mono); }
  .cast-stat-value.cast-canon { color: #daa520; text-shadow: 0 0 8px rgba(218,165,32,0.3); }
  .cast-stat-value.cast-link { color: #00f2ff; text-shadow: 0 0 8px rgba(0,242,255,0.25); }
  .cast-stat-label { font-size: 9px; color: #4a4a6a; text-transform: uppercase; letter-spacing: 0.6px; font-weight: 600; }

  .cast-graph-canvas {
    /* MemoryGraph's own wrapper is `height: 100%`, which only resolves
       against a parent with a *definite* height — `min-height` alone
       doesn't count for that (a classic CSS percentage-height gotcha), and
       this panel's ancestor chain (ChatExplorerView's scrollable body)
       never gives this element a definite height via flex either. Without
       an explicit `height` here, SvelteFlow measures a zero-height
       container on mount and silently renders nothing, even with real
       data loaded — exactly what showed up as a blank canvas with a
       non-zero stats strip above it. */
    height: 420px; border-radius: 12px; overflow: hidden;
    border: 1px solid rgba(139,92,246,0.08);
  }
  /* Wide (full explorer-view) context: the flex chain above now gives this
     a real definite height to grow into, so let it fill it instead of
     capping at the popover-sized 420px. */
  .cast-graph-canvas.wide { height: auto; flex: 1; min-height: 0; }
  .lore-add-btn {
    background: none; border: 1px solid rgba(139,92,246,0.12);
    border-radius: 8px; padding: 4px; display: flex; cursor: pointer;
    transition: all 150ms;
  }
  .lore-add-btn:hover { border-color: rgba(139,92,246,0.3); background: rgba(139,92,246,0.06); }

  .cast-action-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 4px 9px; border-radius: 7px;
    background: rgba(139,92,246,0.05); border: 1px solid rgba(139,92,246,0.16);
    color: #b8a8e8; font-size: 11px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms; white-space: nowrap;
  }
  .cast-action-btn:hover { border-color: rgba(139,92,246,0.34); background: rgba(139,92,246,0.1); color: #e8e0ff; }
  .cast-action-btn.danger { border-color: rgba(248,113,113,0.2); color: #f87171; }
  .cast-action-btn.danger:hover { border-color: rgba(248,113,113,0.4); background: rgba(248,113,113,0.08); }

  .lore-empty { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: #4a4a6a; font-size: var(--text-sm); }
  .lore-loading { display: flex; gap: 4px; padding: 14px; justify-content: center; }
  .loading-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: #5a5a7a; animation: dotPulse 1.2s ease-in-out infinite;
  }
  .loading-dot.d2 { animation-delay: 150ms; }
  .loading-dot.d3 { animation-delay: 300ms; }
  @keyframes dotPulse { 0%,100% { opacity: 0.3; transform: scale(0.8); } 50% { opacity: 1; transform: scale(1); } }

  /* ══ Needs-attention indicator ══ */
  .npc-attention-dot {
    width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0;
    background: #F59E0B;
    animation: npcPulse 1.1s ease-in-out infinite;
  }
  @keyframes npcPulse {
    0%, 100% { opacity: 0.3; transform: scale(0.75); box-shadow: 0 0 0 0 rgba(245,158,11,0); }
    50%      { opacity: 1;   transform: scale(1);    box-shadow: 0 0 6px 2px rgba(245,158,11,0.35); }
  }

  /* `display: contents` by default makes this wrapper transparent to
     layout — the card children keep stacking in the parent's flex column
     exactly as before. `.wide` (the full chat-area explorer view) switches
     it to an actual grid of cards. */
  .cards-grid { display: contents; }
  .cards-grid.wide {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 14px;
    align-items: start;
  }

  .npc-card {
    display: flex; flex-direction: column;
    border-radius: 10px;
    background: rgba(14,14,30,0.4); border: 1px solid rgba(139,92,246,0.06);
    transition: background 200ms cubic-bezier(0.16, 1, 0.3, 1),
                border-color 200ms cubic-bezier(0.16, 1, 0.3, 1),
                box-shadow 200ms cubic-bezier(0.16, 1, 0.3, 1),
                transform 200ms cubic-bezier(0.16, 1, 0.3, 1);
    overflow: hidden;
  }
  .npc-card.expanded-span { grid-column: 1 / -1; }
  .npc-card:hover {
    background: rgba(139,92,246,0.05); border-color: rgba(139,92,246,0.14);
    box-shadow: 0 4px 20px rgba(0,0,0,0.2);
  }
  .wide .npc-card:hover { transform: translateY(-1px); }
  /* A card holding an open profile sheet reads as a focused workspace, not
     just another list row — a touch more presence than the plain hover
     state, and it persists while expanded regardless of hover. */
  .npc-card.is-open {
    background: rgba(139,92,246,0.045);
    border-color: rgba(139,92,246,0.18);
    box-shadow: 0 12px 36px rgba(0,0,0,0.35), 0 0 0 1px rgba(139,92,246,0.05) inset;
  }

  .npc-card-top {
    display: flex; align-items: center; gap: 8px;
    padding: clamp(9px, 2.6cqi, 14px) clamp(10px, 3cqi, 16px);
    background: none; border: none; cursor: pointer; width: 100%; text-align: left;
  }

  .cast-avatar {
    width: clamp(26px, 7cqi, 36px); height: clamp(26px, 7cqi, 36px); min-width: clamp(26px, 7cqi, 36px); border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    display: flex; align-items: center; justify-content: center;
    outline: 1.5px solid rgba(139,92,246,0.18); outline-offset: 2px;
    box-shadow: 0 0 10px rgba(139,92,246,0.15);
    /* Now a <button> (so a real portrait can open a lightbox preview
       independently of the card's own expand/collapse click) — reset the
       native button chrome back to how the plain div looked before. */
    border: none; padding: 0; cursor: default;
    transition: transform 150ms cubic-bezier(0.34,1.56,0.64,1), box-shadow 150ms;
  }
  .cast-avatar-clickable { cursor: pointer; }
  .cast-avatar-clickable:hover { transform: scale(1.08); box-shadow: 0 0 16px rgba(139,92,246,0.35); }
  .cast-avatar-clickable:active { transform: scale(0.96); }
  .cast-avatar:disabled { cursor: default; }
  .cast-avatar-initial { font-size: 11px; font-weight: 700; color: #fff; text-transform: uppercase; }
  .cast-avatar-img { width: 100%; height: 100%; border-radius: 50%; object-fit: cover; }

  .npc-info { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; }
  .cast-name {
    font-size: var(--text-sm); font-weight: 600; color: #8b8ba7;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .npc-badge {
    padding: 2px 7px; border-radius: 99px;
    font-size: 9px; font-weight: 700; letter-spacing: 0.5px;
    text-transform: uppercase; flex-shrink: 0;
    background: rgba(34,197,94,0.12); color: #4ade80;
  }
  .npc-badge.concealed { background: rgba(139,92,246,0.14); color: #c4a1ff; }
  .npc-badge.unconfirmed { background: rgba(245,158,11,0.12); color: #F59E0B; }
  .npc-badge.outdated { background: rgba(249,115,22,0.14); color: #fb923c; }
  .cast-role-badge {
    padding: 2px 7px; border-radius: 99px;
    font-size: 9px; font-weight: 700; letter-spacing: 0.5px;
    text-transform: uppercase; flex-shrink: 0;
  }

  .npc-expand {
    display: flex; flex-direction: column; gap: 4px;
    padding: 4px clamp(10px, 3cqi, 16px) 14px;
  }

  /* ── Profile sheet — portrait pane + fields pane, side by side when there's
     room, stacking on narrower popovers. Replaces the old flat label/
     textarea form styling with something closer to a character sheet. ── */
  .npc-sheet {
    display: flex; gap: 16px; align-items: flex-start;
    padding-top: 10px; margin-top: 6px;
    border-top: 1px solid rgba(139,92,246,0.08);
  }

  .npc-sheet-portrait {
    display: flex; flex-direction: column; align-items: center; gap: 8px;
    flex: 0 0 auto; width: 92px;
  }
  .npc-sheet-frame {
    width: 92px; height: 92px; border-radius: 14px; position: relative;
    display: flex; align-items: center; justify-content: center; overflow: hidden;
    background: linear-gradient(155deg, rgba(139,92,246,0.16), rgba(191,64,255,0.08));
    border: 1px solid rgba(139,92,246,0.16);
    box-shadow: 0 6px 20px rgba(0,0,0,0.35), inset 0 1px 0 rgba(255,255,255,0.04);
  }
  .npc-sheet-frame.has-image { background: #0b0b1c; }
  .npc-sheet-frame-btn {
    width: 100%; height: 100%; padding: 0; margin: 0; border: none; background: none;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer;
  }
  .npc-sheet-frame-btn:disabled { cursor: default; }
  .npc-sheet-frame-img { width: 100%; height: 100%; object-fit: cover; transition: filter 150ms ease, transform 150ms ease; }
  .npc-sheet-frame-btn:not(:disabled):hover .npc-sheet-frame-img { filter: brightness(1.12); transform: scale(1.04); }
  .npc-sheet-frame-btn:not(:disabled):active .npc-sheet-frame-img { transform: scale(0.98); }
  .npc-sheet-frame-initial {
    font-family: Georgia, 'Iowan Old Style', 'Palatino Linotype', Palatino, serif; font-size: 34px; font-weight: 600;
    color: rgba(232,224,255,0.5);
  }
  .npc-sheet-frame-overlay {
    position: absolute; inset: 0; display: flex; align-items: center; justify-content: center;
    background: rgba(7,7,20,0.55); backdrop-filter: blur(2px);
  }
  .npc-sheet-portrait-actions { display: flex; gap: 4px; }
  .npc-icon-btn {
    width: 26px; height: 26px; border-radius: 8px; display: flex; align-items: center; justify-content: center;
    background: transparent; border: 1px solid rgba(139,92,246,0.14); color: #8b8ba7;
    cursor: pointer; transition: all 150ms ease; flex-shrink: 0;
  }
  .npc-icon-btn:hover { background: rgba(139,92,246,0.1); border-color: rgba(139,92,246,0.25); color: #c8c8e0; }
  .npc-icon-btn:active { transform: scale(0.92); }
  .npc-icon-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .npc-icon-btn-danger { color: #F43F5E; border-color: rgba(244,63,94,0.18); }
  .npc-icon-btn-danger:hover { background: rgba(244,63,94,0.12); color: #fb7185; }

  .npc-sheet-fields { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }

  .npc-sheet-name-label { display: block; }
  .npc-sheet-name-input {
    width: 100%; font-family: Georgia, 'Iowan Old Style', 'Palatino Linotype', Palatino, serif; font-size: 19px; font-weight: 600;
    color: var(--fg-primary, #e8e0ff); background: transparent;
    border: none; border-bottom: 1px solid rgba(139,92,246,0.14);
    padding: 2px 1px 8px; letter-spacing: 0.2px;
    transition: border-color 150ms ease;
  }
  .npc-sheet-name-input::placeholder { font-family: var(--font-body); font-size: var(--text-sm); font-weight: 400; color: #5a5a7a; }
  .npc-sheet-name-input:focus { outline: none; border-bottom-color: rgba(139,92,246,0.5); }

  .npc-sheet-field { display: flex; flex-direction: column; gap: 6px; }
  .npc-sheet-field-label {
    display: flex; align-items: center; gap: 6px;
    font-size: 10px; font-weight: 700; color: #a78bfa;
    letter-spacing: 0.6px; text-transform: uppercase;
  }
  .npc-sheet-textarea {
    width: 100%; resize: vertical; font-family: var(--font-body); font-size: var(--text-sm);
    line-height: 1.6; color: #c8c8e0;
    background: rgba(10,9,22,0.55); border: 1px solid rgba(139,92,246,0.1);
    border-radius: 10px; padding: 10px 12px;
    box-shadow: inset 0 1px 2px rgba(0,0,0,0.2);
    transition: border-color 150ms ease, box-shadow 150ms ease;
  }
  .npc-sheet-textarea:focus {
    outline: none; border-color: rgba(139,92,246,0.35);
    box-shadow: inset 0 1px 2px rgba(0,0,0,0.2), 0 0 0 3px rgba(139,92,246,0.08);
  }

  @container (max-width: 340px) {
    .npc-sheet { flex-direction: column; align-items: stretch; }
    .npc-sheet-portrait { flex-direction: row; width: auto; justify-content: flex-start; }
  }

  .npc-expand-actions { display: flex; align-items: center; gap: 8px; margin-top: 12px; }
  .npc-btn-promote { margin-left: auto; }
  /* Resolves the "Unconfirmed" badge — an emerald accent reads as the
     approve/confirm action, distinct from Save's primary violet and
     Refresh's magic-glow violet. */
  .npc-btn-confirm { border-color: rgba(52,211,153,0.2); color: #34d399; }
  .npc-btn-confirm:hover {
    border-color: rgba(52,211,153,0.4); background: rgba(52,211,153,0.08); color: #6ee7b7;
    box-shadow: 0 0 0 1px rgba(52,211,153,0.12), 0 4px 16px rgba(52,211,153,0.16);
  }
  /* "Refresh from Story" is an AI-driven rewrite, not a plain utility action
     — a quiet violet glow on hover nods to that without competing with the
     Save button's primary-gradient weight. */
  .npc-btn-magic:hover {
    border-color: rgba(139,92,246,0.35); color: #c4a1ff;
    box-shadow: 0 0 0 1px rgba(139,92,246,0.15), 0 4px 16px rgba(139,92,246,0.18);
  }
  /* Same outline/primary/danger pill language as Settings' Export/Import/
     Clear-All buttons and the Image Presets Delete/Add-Preset buttons —
     kept in lockstep with `.settings-btn` rather than the smaller, more
     subdued buttons this used to have. */
  .npc-btn {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    padding: 9px 16px; border-radius: 10px; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); background: transparent;
    border: 1px solid rgba(139,92,246,0.12); color: #8b8ba7;
    cursor: pointer; transition: all 180ms ease;
  }
  .npc-btn:hover { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2); color: #c8c8e0; }
  .npc-btn:disabled { opacity: 0.5; cursor: default; pointer-events: none; }
  .npc-btn-primary {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff); border: none; color: #fff;
    box-shadow: 0 2px 12px rgba(139,92,246,0.3);
  }
  .npc-btn-primary:hover { transform: translateY(-1px); box-shadow: 0 4px 20px rgba(139,92,246,0.45); background: none; }

  /* Live AI Horde progress — same status language (pulsing dot + phase
     text) as the Scene panel's own generation status. The pulsing dot now
     lives inside the portrait frame overlay itself (closer to what's
     actually loading); this is just the compact phase-text pill beneath it. */
  .npc-portrait-status {
    text-align: center; font-size: 10px; color: #a78bfa; font-family: var(--font-mono);
    line-height: 1.4; max-width: 92px;
  }
  .npc-portrait-status-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: #c4a1ff; box-shadow: 0 0 8px rgba(196,161,255,0.7);
    animation: npcPulse 1.1s ease-in-out infinite;
  }

  .npc-card.muted { opacity: 0.55; }

  .cast-action-btn {
    background: none; border: none; padding: 4px; border-radius: 6px;
    cursor: pointer; display: flex; align-items: center; justify-content: center;
    transition: background 150ms, transform 100ms; min-width: 24px; min-height: 24px;
    flex-shrink: 0;
  }
  .cast-action-btn:hover { background: rgba(139,92,246,0.08); }
  .cast-action-btn:active { transform: scale(0.92); }
  .cast-remove-btn { opacity: 0; transition: opacity 150ms; }
  .npc-card:hover .cast-remove-btn { opacity: 0.5; }
  .cast-remove-btn:hover { opacity: 1 !important; }

  /* Small inline spinner shown in the chevron slot while a Gallery
     character's full record is being fetched on first expand. */
  .npc-spinner {
    width: 13px; height: 13px; border-radius: 50%; flex-shrink: 0;
    border: 1.5px solid rgba(139,92,246,0.2); border-top-color: #c4a1ff;
    animation: npcSpin 700ms linear infinite;
  }
  @keyframes npcSpin { to { transform: rotate(360deg); } }

  .npc-expand-loading {
    display: flex; gap: 4px; padding: 14px 4px; justify-content: center;
  }

  .cast-slider-row {
    display: flex; align-items: center; gap: 8px;
    padding: 0 clamp(10px, 3cqi, 16px) clamp(9px, 2.6cqi, 14px);
  }
  .npc-card .cast-slider-row {
    padding-left: clamp(10px, 3cqi, 16px);
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

  /* ── Portrait lightbox ── */
  .avatar-lightbox-backdrop {
    position: fixed; inset: 0; z-index: 300;
    background: rgba(4,4,12,0.88); backdrop-filter: blur(10px);
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px;
    cursor: pointer;
    animation: lightboxIn 180ms cubic-bezier(0.16,1,0.3,1) both;
  }
  @keyframes lightboxIn { from { opacity: 0; } to { opacity: 1; } }
  .avatar-lightbox-img {
    max-width: min(78vw, 560px); max-height: 72vh;
    border-radius: 16px; object-fit: contain;
    box-shadow: 0 24px 80px rgba(0,0,0,0.6), 0 0 0 1px rgba(139,92,246,0.15);
    cursor: default;
    animation: lightboxImgIn 220ms cubic-bezier(0.16,1,0.3,1) both;
  }
  @keyframes lightboxImgIn { from { opacity: 0; transform: scale(0.96); } to { opacity: 1; transform: scale(1); } }
  .avatar-lightbox-name {
    font-size: var(--text-md); font-weight: 600; color: #e8e0ff;
    letter-spacing: -0.1px;
  }
  .avatar-lightbox-close {
    position: absolute; top: 20px; right: 24px;
    width: 38px; height: 38px; border-radius: 10px;
    background: rgba(20,20,40,0.7); border: 1px solid rgba(139,92,246,0.2);
    display: flex; align-items: center; justify-content: center; cursor: pointer;
    transition: background 150ms, border-color 150ms;
  }
  .avatar-lightbox-close:hover { background: rgba(139,92,246,0.15); border-color: rgba(139,92,246,0.4); }
</style>
