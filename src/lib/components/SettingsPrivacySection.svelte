<script lang="ts">
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError } from '$lib/stores/toast';
  import { browser } from '$app/environment';
  import { loadConversations } from '$lib/stores/chat';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  /** Called after a successful import so the orchestrator can force every
   *  other section to remount (and re-read its local state fresh from the
   *  now-updated $settings store) — see the `{#key importGeneration}`
   *  wrapper in +page.svelte. Each section owns its own state end-to-end
   *  now, so this section can no longer reach into their locals directly
   *  the way the old monolithic page did. */
  let { onImported }: { onImported: () => void } = $props();

  let localStorageOnly = $state($settings.localStorageOnly);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { localStorageOnly };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  let showClearConfirm = $state(false);
  let showPrivacyConfirm = $state(false);
  let isExporting = $state(false);
  let isImporting = $state(false);

  // Live progress line shown under the Export/Import buttons while busy —
  // both operations can involve hundreds of sequential IPC calls for a
  // large library, so silence for 10+ seconds would look hung.
  let backupStatus = $state('');

  /**
   * Exports the full local library as a self-contained JSON backup:
   * every character (+ its lorebook), every conversation (+ its full
   * message history and group-cast roster), and every pinned/canon memory.
   * Settings are included too, matching the pre-existing behavior.
   *
   * list_conversations caps at 200/page server-side, so this paginates
   * rather than assuming a single call returns everything (the previous
   * version silently exported only the 50 most recent conversations).
   * Memories are fetched per-character rather than via the no-args
   * list_memories call, which caps at 100 rows globally.
   */
  async function handleExport() {
    if (!isTauri) return;
    isExporting = true;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ipc = await import('$lib/services/ipc');

      backupStatus = 'Gathering conversations...';
      const conversations: Awaited<ReturnType<typeof ipc.listConversations>> = [];
      let offset = 0;
      while (true) {
        const page = await ipc.listConversations(200, offset);
        conversations.push(...page);
        if (page.length < 200) break;
        offset += 200;
      }

      const characters = await ipc.listCharacters();

      const messagesByConversation: Record<string, unknown> = {};
      const castByConversation: Record<string, unknown> = {};
      for (const [i, conv] of conversations.entries()) {
        backupStatus = `Exporting conversation ${i + 1}/${conversations.length}...`;
        messagesByConversation[conv.id] = await ipc.getConversationMessages(conv.id);
        try {
          castByConversation[conv.id] = await ipc.listConversationCharacters(conv.id);
        } catch {
          castByConversation[conv.id] = [];
        }
      }

      const lorebookByCharacter: Record<string, unknown> = {};
      const memoriesByCharacter: Record<string, unknown> = {};
      for (const [i, char] of characters.entries()) {
        backupStatus = `Exporting character ${i + 1}/${characters.length}...`;
        lorebookByCharacter[char.id] = await ipc.listLorebookEntries(char.id);
        // Per-character (not the no-args global call) so this isn't capped at 100 rows.
        memoriesByCharacter[char.id] = await ipc.listMemories(char.id);
      }

      const exportData = {
        version: '2.0',
        exportedAt: new Date().toISOString(),
        settings: $settings,
        characters,
        lorebookByCharacter,
        conversations,
        messagesByConversation,
        castByConversation,
        memoriesByCharacter,
      };

      backupStatus = '';
      const savePath = await save({
        filters: [{ name: 'Janus Export', extensions: ['json'] }],
        defaultPath: `mythic-export-${Date.now()}.json`,
      });

      if (savePath) {
        const { writeTextFile } = await import('@tauri-apps/plugin-fs');
        await writeTextFile(savePath, JSON.stringify(exportData, null, 2));
        success(`Exported ${characters.length} characters, ${conversations.length} conversations`);
      }
    } catch (err) {
      toastError('Failed to export data');
      console.error('Export failed:', err);
    }
    backupStatus = '';
    isExporting = false;
  }

  /**
   * Restores a backup written by handleExport. Every record is recreated
   * with a fresh ID (never reusing the file's original IDs), so this is
   * additive and safe to run against a non-empty library — nothing existing
   * is touched or overwritten. All cross-references (character_id,
   * conversation_id, parent_id, ...) are remapped through old→new ID maps
   * built up as each record is created.
   *
   * Recreates, in dependency order: characters → lorebook entries →
   * conversations → messages (oldest-first per conversation, so a message's
   * parent always exists before it does, then the active/tip pointer is
   * restored) → group-cast rosters → memories (promoting canon ones and
   * restoring non-default importance afterward, since create_memory takes
   * neither directly).
   *
   * Branch-to-branch links (parent_conversation_id / branch_point_message_id)
   * are intentionally NOT restored — each conversation comes back as a
   * flattened, standalone conversation with its own message tree intact.
   * Reconstructing cross-conversation branch ancestry correctly would need
   * import ordered by original creation time with forward-reference
   * handling for branches created from each other, which isn't worth the
   * complexity for a backup/restore feature.
   *
   * Files exported before this fix (version "1.0") only ever contained
   * settings — nothing else to restore from those.
   */
  async function handleImport() {
    if (!isTauri) return;
    isImporting = true;
    backupStatus = '';
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Janus Export', extensions: ['json'] }],
      });
      if (!selected) { isImporting = false; return; }

      const { readTextFile } = await import('@tauri-apps/plugin-fs');
      const raw = await readTextFile(selected as string);
      const data = JSON.parse(raw);
      const ipc = await import('$lib/services/ipc');

      if (data.settings) {
        settings.set({ ...$settings, ...data.settings });
        localStorageOnly = $settings.localStorageOnly;
        // Every other section's own local state is stale now — the
        // orchestrator remounts them via `{#key importGeneration}` so each
        // re-reads fresh from the just-updated $settings store.
        onImported();
      }

      if (data.version !== '2.0' || !Array.isArray(data.characters)) {
        success('Settings imported successfully');
        isImporting = false;
        return;
      }

      const charIdMap = new Map<string, string>();
      const convIdMap = new Map<string, string>();
      let charsOk = 0, charsFailed = 0;
      let loreOk = 0, loreFailed = 0;
      let convsOk = 0, convsFailed = 0;
      let msgsOk = 0, msgsFailed = 0;
      let castFailed = 0;
      let memOk = 0, memFailed = 0;

      // 1. Characters
      for (const [i, char] of data.characters.entries()) {
        backupStatus = `Importing character ${i + 1}/${data.characters.length}...`;
        try {
          const created = await ipc.createCharacter(char.name, char.data as Record<string, unknown>);
          charIdMap.set(char.id, created.id);
          charsOk++;
        } catch (err) {
          charsFailed++;
          console.error('Import: failed to create character', char.name, err);
        }
      }

      // 2. Lorebook entries, keyed by the original character id
      const lorebookByChar: Record<string, any[]> = data.lorebookByCharacter ?? {};
      for (const [oldCharId, entries] of Object.entries(lorebookByChar)) {
        const newCharId = charIdMap.get(oldCharId);
        if (!newCharId) continue;
        for (const entry of entries) {
          try {
            await ipc.createLorebookEntry(
              newCharId,
              entry.name ?? entry.title ?? '',
              entry.keys ?? [],
              entry.content ?? '',
              entry.always_active ?? entry.alwaysActive ?? false,
            );
            loreOk++;
          } catch (err) {
            loreFailed++;
            console.error('Import: failed to create lorebook entry', err);
          }
        }
      }

      // 3. Conversations (flattened — see doc comment above on branch links)
      const conversations: any[] = data.conversations ?? [];
      for (const [i, conv] of conversations.entries()) {
        backupStatus = `Importing conversation ${i + 1}/${conversations.length}...`;
        const newCharId = conv.character_id ? charIdMap.get(conv.character_id) : undefined;
        try {
          const created = await ipc.createConversation(newCharId, conv.title);
          convIdMap.set(conv.id, created.id);
          convsOk++;
          if (conv.memory_scope && conv.memory_scope !== 'character') {
            await ipc.setMemoryScope(created.id, conv.memory_scope);
          }
        } catch (err) {
          convsFailed++;
          console.error('Import: failed to create conversation', conv.title, err);
        }
      }

      // 4. Messages, oldest-first per conversation so parents exist before children
      const messagesByConv: Record<string, any[]> = data.messagesByConversation ?? {};
      for (const [oldConvId, msgs] of Object.entries(messagesByConv)) {
        const newConvId = convIdMap.get(oldConvId);
        if (!newConvId) continue;
        const msgIdMap = new Map<string, string>();
        const sorted = [...msgs].sort((a, b) => (a.created_at ?? '').localeCompare(b.created_at ?? ''));
        let lastCreatedId: string | undefined;
        for (const msg of sorted) {
          const newParentId = msg.parent_id ? msgIdMap.get(msg.parent_id) : undefined;
          try {
            const created = await ipc.createMessage(newConvId, msg.role, msg.content, newParentId, msg.metadata ?? undefined);
            msgIdMap.set(msg.id, created.id);
            lastCreatedId = created.id;
            msgsOk++;
          } catch (err) {
            msgsFailed++;
            console.error('Import: failed to create message', err);
          }
        }
        // Restore the active/tip pointer (falls back to the last message created)
        const origConv = conversations.find((c) => c.id === oldConvId);
        const activeNewId = (origConv?.active_message_id && msgIdMap.get(origConv.active_message_id)) || lastCreatedId;
        if (activeNewId) {
          try { await ipc.setActiveMessage(newConvId, activeNewId); } catch { /* non-fatal */ }
        }
      }

      // 5. Group cast rosters (multi-character conversations)
      const castByConv: Record<string, any[]> = data.castByConversation ?? {};
      for (const [oldConvId, cast] of Object.entries(castByConv)) {
        const newConvId = convIdMap.get(oldConvId);
        if (!newConvId) continue;
        for (const member of cast) {
          const newMemberCharId = charIdMap.get(member.character_id);
          if (!newMemberCharId) continue;
          try {
            await ipc.addConversationCharacter(newConvId, newMemberCharId, member.character_name, member.role, member.talkativeness);
            if (member.is_active === false) {
              await ipc.toggleCharacterActive(newConvId, newMemberCharId, false);
            }
          } catch (err) {
            castFailed++;
            console.error('Import: failed to add group cast member', err);
          }
        }
      }

      // 6. Memories (canon status and non-default importance need follow-up calls —
      // create_memory takes neither directly)
      const memoriesByChar: Record<string, any[]> = data.memoriesByCharacter ?? {};
      for (const [oldCharId, mems] of Object.entries(memoriesByChar)) {
        const newCharId = charIdMap.get(oldCharId);
        if (!newCharId) continue;
        for (const mem of mems) {
          const newConvId = mem.conversation_id ? convIdMap.get(mem.conversation_id) : undefined;
          try {
            const created = await ipc.createMemory(mem.content, newCharId, newConvId, mem.source);
            if (mem.is_canon) await ipc.promoteToCanon(created.id);
            if (typeof mem.importance === 'number' && mem.importance !== 5) {
              await ipc.setMemoryImportance(created.id, mem.importance);
            }
            memOk++;
          } catch (err) {
            memFailed++;
            console.error('Import: failed to create memory', err);
          }
        }
      }

      await loadConversations();

      const failed = charsFailed + loreFailed + convsFailed + msgsFailed + castFailed + memFailed;
      success(`Imported ${charsOk} characters, ${convsOk} conversations, ${msgsOk} messages, ${loreOk} lorebook entries, ${memOk} memories`);
      if (failed > 0) {
        toastError(`${failed} item(s) failed to import — check the console for details`);
      }
    } catch (err) {
      toastError('Failed to import data');
      console.error('Import failed:', err);
    }
    backupStatus = '';
    isImporting = false;
  }

  /** Clear all conversations after user confirmation */
  async function clearAllConversations() {
    if (!isTauri) { showClearConfirm = false; return; }
    try {
      const ipc = await import('$lib/services/ipc');
      const convs = await ipc.listConversations();
      let cleared = 0;
      for (const conv of convs) {
        await ipc.deleteConversation(conv.id);
        cleared++;
      }
      showClearConfirm = false;
      success(`Cleared ${cleared} conversation${cleared !== 1 ? 's' : ''}`);
    } catch (err) {
      toastError('Failed to clear conversations');
      console.error('Clear failed:', err);
    }
  }
</script>

<div class="panel-heading animate-fade-in-up stagger-3">
  <span class="panel-heading-title">Data & Privacy</span>
  <span class="panel-heading-desc">What Janus stores, and how to back it up or wipe it</span>
</div>
<section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
  <div class="setting-row">
    <div class="setting-label">
      <span class="setting-name">Local Storage Only</span>
      <span class="setting-desc">All data stays on your device — no cloud sync or telemetry</span>
    </div>
    <button
      class="toggle-switch"
      class:on={localStorageOnly}
      onclick={() => {
        if (localStorageOnly) {
          // Turning OFF privacy mode — confirm
          showPrivacyConfirm = true;
        } else {
          localStorageOnly = true;
          success('Privacy mode enabled — all data stays local');
        }
      }}
      role="switch"
      aria-checked={localStorageOnly}
      aria-label="Toggle local storage only"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>

  <div class="button-row">
    <button class="settings-btn outline" onclick={handleExport} disabled={isExporting || isImporting}>
      <Icon name="download" size={14} color="var(--fg-secondary)" />
      <span>{isExporting ? 'Exporting...' : 'Export Data'}</span>
    </button>
    <button class="settings-btn outline" onclick={handleImport} disabled={isExporting || isImporting}>
      <Icon name="upload" size={14} color="var(--fg-secondary)" />
      <span>{isImporting ? 'Importing...' : 'Import Data'}</span>
    </button>
  </div>
  {#if backupStatus}
    <span class="backup-status">{backupStatus}</span>
  {/if}

  {#if showClearConfirm}
    <div class="clear-confirm">
      <span class="clear-warn">This will permanently delete all conversations. Are you sure?</span>
      <div class="button-row">
        <button class="settings-btn outline" onclick={() => showClearConfirm = false}>Cancel</button>
        <button class="settings-btn danger" onclick={clearAllConversations}>
          <Icon name="trash-2" size={14} color="var(--danger)" />
          <span>Yes, Delete All</span>
        </button>
      </div>
    </div>
  {:else}
    <button class="settings-btn danger" onclick={() => showClearConfirm = true}>
      <Icon name="trash-2" size={14} color="var(--danger)" />
      <span>Clear All Conversations</span>
    </button>
  {/if}

  {#if showPrivacyConfirm}
    <div class="clear-confirm">
      <span class="clear-warn">Disabling privacy mode may allow future cloud features to sync your data externally. Continue?</span>
      <div class="button-row">
        <button class="settings-btn outline" onclick={() => showPrivacyConfirm = false}>Keep Private</button>
        <button class="settings-btn danger" onclick={() => { localStorageOnly = false; showPrivacyConfirm = false; success('Privacy mode disabled'); }}>
          <Icon name="shield-off" size={14} color="var(--danger)" />
          <span>Disable Privacy</span>
        </button>
      </div>
    </div>
  {/if}
</section>
