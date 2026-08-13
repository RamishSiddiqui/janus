<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import Icon from '$lib/components/Icon.svelte';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { TrashItem, TrashItemType } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  interface Row extends TrashItem {
    avatarUrl: string | null;
  }

  let rows: Row[] = $state([]);
  let isLoading = $state(true);
  let filter: 'all' | TrashItemType = $state('all');
  let isEmptying = $state(false);
  let pendingAction: { kind: 'empty' } | { kind: 'delete-one'; row: Row } | null = $state(null);
  let restoringIds = $state<Set<string>>(new Set());
  let deletingIds = $state<Set<string>>(new Set());

  let filtered = $derived(filter === 'all' ? rows : rows.filter(r => r.item_type === filter));
  let counts = $derived({
    all: rows.length,
    conversation: rows.filter(r => r.item_type === 'conversation').length,
    character: rows.filter(r => r.item_type === 'character').length,
    persona: rows.filter(r => r.item_type === 'persona').length,
  });

  function typeLabel(t: TrashItemType): string {
    return t === 'conversation' ? 'Conversation' : t === 'character' ? 'Character' : 'Persona';
  }
  function typeIcon(t: TrashItemType): string {
    return t === 'conversation' ? 'message-circle' : t === 'character' ? 'users' : 'user';
  }
  function typeColor(t: TrashItemType): string {
    return t === 'conversation' ? '#00f2ff' : t === 'character' ? '#8B5CF6' : '#bf40ff';
  }

  function relativeTime(dateStr: string): string {
    if (!dateStr) return '';
    const date = new Date(dateStr);
    if (isNaN(date.getTime())) return '';
    const diff = Date.now() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days}d ago`;
    return date.toLocaleDateString();
  }

  async function resolveAvatarUrl(avatarPath: string | null): Promise<string | null> {
    if (!avatarPath || !isTauri) return null;
    try {
      const { loadFileAsBlobUrl } = await import('$lib/utils/blobUrl');
      return await loadFileAsBlobUrl(avatarPath);
    } catch {
      return null;
    }
  }

  async function loadTrash() {
    if (!isTauri) { isLoading = false; return; }
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const items = await ipc.listTrash();
      rows = await Promise.all(items.map(async (item) => ({
        ...item,
        avatarUrl: await resolveAvatarUrl(item.avatar_path),
      })));
    } catch (err) {
      console.error('Failed to load trash:', err);
      toastError('Failed to load Trash');
    }
    isLoading = false;
  }

  onMount(() => { loadTrash(); });

  async function restoreRow(row: Row) {
    if (!isTauri || restoringIds.has(row.id)) return;
    restoringIds = new Set(restoringIds).add(row.id);
    try {
      const ipc = await import('$lib/services/ipc');
      if (row.item_type === 'conversation') await ipc.restoreConversation(row.id);
      else if (row.item_type === 'character') await ipc.restoreCharacter(row.id);
      else await ipc.restorePersona(row.id);
      rows = rows.filter(r => r.id !== row.id);
      success(`Restored ${row.name}`);
    } catch (err) {
      toastError(`Failed to restore ${row.name}`);
    }
    const next = new Set(restoringIds);
    next.delete(row.id);
    restoringIds = next;
  }

  async function deleteForever(row: Row) {
    if (!isTauri || deletingIds.has(row.id)) return;
    deletingIds = new Set(deletingIds).add(row.id);
    try {
      const ipc = await import('$lib/services/ipc');
      if (row.item_type === 'conversation') await ipc.deleteConversation(row.id);
      else if (row.item_type === 'character') await ipc.deleteCharacter(row.id);
      else await ipc.deletePersona(row.id);
      rows = rows.filter(r => r.id !== row.id);
      success(`Permanently deleted ${row.name}`);
    } catch (err) {
      toastError(`Failed to delete ${row.name}`);
    }
    const next = new Set(deletingIds);
    next.delete(row.id);
    deletingIds = next;
    pendingAction = null;
  }

  async function emptyTrash() {
    if (!isTauri || isEmptying) return;
    isEmptying = true;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.emptyTrash();
      rows = [];
      success('Trash emptied');
    } catch (err) {
      toastError('Failed to empty Trash');
    }
    isEmptying = false;
    pendingAction = null;
  }
</script>

<svelte:head>
  <title>Trash — Janus</title>
</svelte:head>

<div class="trash-page">
  <header class="trash-header">
    <div class="trash-header-left">
      <h1 class="trash-title"><SplitHeading text="Trash" /></h1>
      <span class="trash-subtitle">{rows.length} item{rows.length === 1 ? '' : 's'}</span>
    </div>
    <div class="trash-header-right">
      <button
        class="trash-btn danger"
        disabled={rows.length === 0 || isEmptying}
        onclick={() => pendingAction = { kind: 'empty' }}
      >
        <Icon name="trash-2" size={14} color="#F43F5E" />
        <span>Empty Trash</span>
      </button>
    </div>
  </header>

  <div class="trash-filters">
    <button class="filter-chip" class:active={filter === 'all'} onclick={() => filter = 'all'}>
      All <span class="filter-count">{counts.all}</span>
    </button>
    <button class="filter-chip" class:active={filter === 'conversation'} onclick={() => filter = 'conversation'}>
      <Icon name="message-circle" size={12} color={filter === 'conversation' ? '#00f2ff' : 'var(--fg-muted)'} />
      Conversations <span class="filter-count">{counts.conversation}</span>
    </button>
    <button class="filter-chip" class:active={filter === 'character'} onclick={() => filter = 'character'}>
      <Icon name="users" size={12} color={filter === 'character' ? '#8B5CF6' : 'var(--fg-muted)'} />
      Characters <span class="filter-count">{counts.character}</span>
    </button>
    <button class="filter-chip" class:active={filter === 'persona'} onclick={() => filter = 'persona'}>
      <Icon name="user" size={12} color={filter === 'persona' ? '#bf40ff' : 'var(--fg-muted)'} />
      Personas <span class="filter-count">{counts.persona}</span>
    </button>
  </div>

  <div class="trash-list">
    {#if isLoading}
      {#each Array(4) as _}
        <div class="trash-row skeleton-row">
          <Skeleton variant="circle" width="36px" height="36px" />
          <Skeleton variant="text" width="40%" height="14px" />
        </div>
      {/each}
    {:else if filtered.length === 0}
      <div class="empty-state">
        <Icon name="trash-2" size={32} color="var(--fg-muted)" />
        <span class="empty-title">{rows.length === 0 ? 'Trash is empty' : 'No matching items'}</span>
        <span class="empty-desc">Deleted conversations, characters, and personas land here before they're gone for good.</span>
      </div>
    {:else}
      {#each filtered as row (row.id)}
        <div class="trash-row">
          <div class="row-avatar" style="background: color-mix(in srgb, {typeColor(row.item_type)} 14%, transparent)">
            {#if row.avatarUrl}
              <img src={row.avatarUrl} alt={row.name} class="row-avatar-img" />
            {:else}
              <Icon name={typeIcon(row.item_type)} size={16} color={typeColor(row.item_type)} />
            {/if}
          </div>
          <div class="row-body">
            <div class="row-top">
              <span class="row-name">{row.name || 'Untitled'}</span>
              <span class="row-badge" style="color:{typeColor(row.item_type)};background:color-mix(in srgb,{typeColor(row.item_type)} 12%,transparent)">
                {typeLabel(row.item_type)}
              </span>
            </div>
            <span class="row-meta">Trashed {relativeTime(row.deleted_at)}</span>
          </div>
          <div class="row-actions">
            <button
              class="row-btn"
              disabled={restoringIds.has(row.id)}
              onclick={() => restoreRow(row)}
            >
              <Icon name="refresh-cw" size={13} color="#10B981" />
              <span>{restoringIds.has(row.id) ? 'Restoring…' : 'Restore'}</span>
            </button>
            <button
              class="row-btn danger"
              disabled={deletingIds.has(row.id)}
              onclick={() => pendingAction = { kind: 'delete-one', row }}
            >
              <Icon name="trash-2" size={13} color="#F43F5E" />
              <span>Delete Forever</span>
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  {#if pendingAction}
    <div class="confirm-backdrop" onclick={() => pendingAction = null} onkeydown={(e) => e.key === 'Escape' && (pendingAction = null)} role="dialog" aria-modal="true" aria-label="Confirm" tabindex="-1">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="confirm-card" onclick={(e) => e.stopPropagation()}>
        <div class="confirm-icon">
          <Icon name="trash-2" size={22} color="#F43F5E" />
        </div>
        {#if pendingAction.kind === 'empty'}
          <span class="confirm-title">Empty Trash?</span>
          <span class="confirm-desc">
            This permanently deletes all {rows.length} item{rows.length === 1 ? '' : 's'} in Trash — conversations, characters, and personas alike. This cannot be undone.
          </span>
        {:else}
          <span class="confirm-title">Delete "{pendingAction.row.name}" forever?</span>
          <span class="confirm-desc">This permanently deletes this {typeLabel(pendingAction.row.item_type).toLowerCase()} and everything linked to it. This cannot be undone.</span>
        {/if}
        <div class="confirm-actions">
          <button class="trash-btn outline" onclick={() => pendingAction = null}>Cancel</button>
          <button
            class="trash-btn danger solid"
            onclick={() => pendingAction?.kind === 'empty' ? emptyTrash() : deleteForever(pendingAction!.row)}
          >
            {pendingAction.kind === 'empty' ? (isEmptying ? 'Emptying…' : 'Empty Trash') : 'Delete Forever'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .trash-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
    position: relative;
  }

  .trash-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 20px 28px 18px; flex-shrink: 0; position: relative;
  }
  .trash-header::after {
    content: ''; position: absolute; bottom: 0; left: 28px; right: 28px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .trash-header-left { display: flex; flex-direction: column; gap: 3px; }
  .trash-title {
    font-size: var(--text-2xl); font-weight: 600;
    letter-spacing: -0.5px;
  }
  .trash-subtitle {
    font-size: var(--text-md); color: #5a5a7a; font-family: var(--font-mono);
    letter-spacing: 0.5px;
  }

  .trash-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 16px; border-radius: 10px;
    font-size: 13px; font-family: var(--font-body); font-weight: 600;
    border: none; cursor: pointer; transition: all 180ms ease;
  }
  .trash-btn.outline {
    background: transparent; border: 1px solid rgba(139,92,246,0.12);
    color: #8b8ba7;
  }
  .trash-btn.outline:hover { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2); }
  .trash-btn.danger {
    background: rgba(244,63,94,0.08); border: 1px solid rgba(244,63,94,0.18);
    color: #F43F5E;
  }
  .trash-btn.danger:hover { background: rgba(244,63,94,0.14); border-color: rgba(244,63,94,0.3); }
  .trash-btn.danger.solid {
    background: linear-gradient(135deg, #F43F5E, #e11d48); color: #fff; border: none;
    box-shadow: 0 2px 12px rgba(244,63,94,0.3);
  }
  .trash-btn.danger.solid:hover { box-shadow: 0 4px 20px rgba(244,63,94,0.45); }
  .trash-btn:disabled { opacity: 0.45; cursor: not-allowed; }

  .trash-filters {
    display: flex; gap: 8px; padding: 14px 28px 4px; flex-shrink: 0; flex-wrap: wrap;
  }
  .filter-chip {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 99px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #6b6b8a; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms;
  }
  .filter-chip:hover { border-color: rgba(139,92,246,0.2); color: #a0a0c0; }
  .filter-chip.active {
    background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.3); color: #e0e0f0;
  }
  .filter-count {
    font-family: var(--font-mono); font-size: 10px; opacity: 0.7;
  }

  .trash-list {
    flex: 1; overflow-y: auto; padding: 12px 28px 28px;
    display: flex; flex-direction: column; gap: 8px;
  }

  .trash-row {
    display: flex; align-items: center; gap: 12px;
    padding: 10px 14px; border-radius: 12px;
    background: rgba(14,14,30,0.5); border: 1px solid rgba(139,92,246,0.06);
    transition: border-color 180ms, background 180ms;
    animation: rowIn 240ms ease both;
  }
  @keyframes rowIn { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: translateY(0); } }
  .trash-row:hover { border-color: rgba(139,92,246,0.16); background: rgba(16,16,34,0.7); }

  .row-avatar {
    width: 36px; height: 36px; border-radius: 10px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center; overflow: hidden;
  }
  .row-avatar-img { width: 100%; height: 100%; object-fit: cover; display: block; }

  .row-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .row-top { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .row-name {
    font-size: var(--text-md); font-weight: 600; color: #e0e0f0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .row-badge {
    flex-shrink: 0; padding: 1px 8px; border-radius: 99px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.3px;
  }
  .row-meta { font-size: var(--text-xs); color: #4a4a6a; font-family: var(--font-mono); }

  .row-actions { display: flex; gap: 6px; flex-shrink: 0; }
  .row-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 6px 11px; border-radius: 8px;
    background: rgba(16,185,129,0.08); border: 1px solid rgba(16,185,129,0.18);
    color: #10B981; font-size: 12px; font-weight: 600; font-family: var(--font-body);
    cursor: pointer; transition: all 150ms;
  }
  .row-btn:hover { background: rgba(16,185,129,0.16); border-color: rgba(16,185,129,0.32); }
  .row-btn.danger {
    background: rgba(244,63,94,0.06); border-color: rgba(244,63,94,0.14); color: #F43F5E;
  }
  .row-btn.danger:hover { background: rgba(244,63,94,0.12); border-color: rgba(244,63,94,0.28); }
  .row-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .empty-state {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 10px;
    padding: 60px 16px; text-align: center;
  }
  .empty-title { font-size: var(--text-lg); font-weight: 600; color: #8b8ba7; }
  .empty-desc { font-size: var(--text-sm); color: #4a4a6a; max-width: 360px; line-height: 1.5; }

  .skeleton-row { gap: 12px; }

  .confirm-backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.7); backdrop-filter: blur(8px);
    display: flex; align-items: center; justify-content: center; z-index: 200;
  }
  .confirm-card {
    background: linear-gradient(175deg, #0e0e22, #0a0a1a);
    border: 1px solid rgba(244,63,94,0.18);
    border-radius: 20px; width: 400px; max-width: 92vw;
    display: flex; flex-direction: column; align-items: center; text-align: center;
    padding: 28px 26px 24px;
    box-shadow: 0 24px 60px rgba(0,0,0,0.6), 0 0 30px rgba(244,63,94,0.08);
  }
  .confirm-icon {
    width: 52px; height: 52px; border-radius: 50%;
    background: rgba(244,63,94,0.1); border: 1px solid rgba(244,63,94,0.2);
    display: flex; align-items: center; justify-content: center;
    margin-bottom: 14px;
  }
  .confirm-title { font-size: var(--text-lg); font-weight: 700; color: #e8e0ff; margin-bottom: 8px; }
  .confirm-desc { font-size: var(--text-sm); color: #8b8ba7; line-height: 1.5; margin-bottom: 20px; }
  .confirm-actions { display: flex; gap: 10px; }

  @media (max-width: 600px) {
    .trash-header { flex-direction: column; gap: 12px; align-items: flex-start; padding: 16px; }
    .trash-filters { padding: 12px 16px 4px; }
    .trash-list { padding: 12px 16px 20px; }
    .row-actions { flex-direction: column; }
  }
</style>
