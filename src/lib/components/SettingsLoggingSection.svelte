<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { browser } from '$app/environment';
  import { success, error as toastError } from '$lib/stores/toast';
  import { frontendLogs, formatFrontendLogsAsText, clearFrontendLogs } from '$lib/stores/logs';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  // Backend lines are paged in from the log file (newest page first, older
  // pages prepended as the user scrolls up) instead of reading the whole
  // file at once — a long-running session's log can grow large, and there's
  // no reason to hold all of it in memory just to show the tail.
  const BACKEND_LOG_PAGE_SIZE = 150;

  let logSubTab = $state<'backend' | 'frontend'>('backend');
  let backendLogLines = $state<string[]>([]);
  let backendLogCursor = $state<number | null>(null);
  let backendLogPath = $state('');
  let isLoadingBackendLogs = $state(false);
  let isLoadingMoreBackendLogs = $state(false);
  let isExportingLogs = $state(false);
  let logSearch = $state('');
  let logViewerEl = $state<HTMLDivElement | undefined>(undefined);

  async function scrollLogViewerToBottom() {
    await tick();
    if (logViewerEl) logViewerEl.scrollTop = logViewerEl.scrollHeight;
  }

  async function loadBackendLogs() {
    if (!isTauri) return;
    isLoadingBackendLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const page = await ipc.getBackendLogsPage(undefined, BACKEND_LOG_PAGE_SIZE);
      backendLogLines = page.lines;
      backendLogCursor = page.nextCursor;
      if (!backendLogPath) backendLogPath = await ipc.getBackendLogPath();
    } catch {
      toastError('Failed to load backend logs');
    }
    isLoadingBackendLogs = false;
    scrollLogViewerToBottom();
  }

  async function loadOlderBackendLogs() {
    if (!isTauri || backendLogCursor == null || isLoadingMoreBackendLogs) return;
    isLoadingMoreBackendLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const page = await ipc.getBackendLogsPage(backendLogCursor, BACKEND_LOG_PAGE_SIZE);
      const el = logViewerEl;
      const prevScrollHeight = el?.scrollHeight ?? 0;
      const prevScrollTop = el?.scrollTop ?? 0;
      backendLogLines = [...page.lines, ...backendLogLines];
      backendLogCursor = page.nextCursor;
      await tick();
      // Loading older lines prepends above the current view — restore the
      // reader's position relative to the content they were looking at,
      // instead of letting the prepend silently scroll them to the top.
      if (el) el.scrollTop = prevScrollTop + (el.scrollHeight - prevScrollHeight);
    } catch {
      toastError('Failed to load more logs');
    }
    isLoadingMoreBackendLogs = false;
  }

  function handleLogViewerScroll() {
    if (logSubTab !== 'backend' || !logViewerEl) return;
    if (logViewerEl.scrollTop < 100 && backendLogCursor != null && !isLoadingMoreBackendLogs) {
      loadOlderBackendLogs();
    }
  }

  function selectLogSubTab(tab: 'backend' | 'frontend') {
    logSubTab = tab;
    scrollLogViewerToBottom();
  }

  // Load once when this section mounts, and re-scroll to bottom every time
  // it's revisited (the section is torn down and remounted when the user
  // navigates away and back, since only one settings tab is ever mounted).
  let hasLoadedBackendLogs = false;
  $effect(() => {
    if (isTauri && !hasLoadedBackendLogs) {
      hasLoadedBackendLogs = true;
      loadBackendLogs();
    } else {
      scrollLogViewerToBottom();
    }
  });

  let filteredBackendLines = $derived(
    logSearch.trim()
      ? backendLogLines.filter(l => l.toLowerCase().includes(logSearch.trim().toLowerCase()))
      : backendLogLines
  );
  let filteredFrontendEntries = $derived(
    logSearch.trim()
      ? $frontendLogs.filter(e => e.message.toLowerCase().includes(logSearch.trim().toLowerCase()))
      : $frontendLogs
  );

  function backendLogLineClass(line: string): string {
    if (/\bERROR\b/.test(line)) return 'log-line-error';
    if (/\bWARN\b/.test(line)) return 'log-line-warn';
    if (/\bDEBUG\b/.test(line)) return 'log-line-debug';
    return '';
  }

  async function handleExportLogs() {
    isExportingLogs = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { writeTextFile } = await import('@tauri-apps/plugin-fs');
      const fullBackendLog = await ipc.getBackendLogs(5000);
      const combined = [
        `Janus log export — ${new Date().toISOString()}`,
        '',
        '===== BACKEND LOG =====',
        fullBackendLog || '(empty)',
        '',
        '===== FRONTEND LOG =====',
        formatFrontendLogsAsText($frontendLogs) || '(empty)',
      ].join('\n');
      const savePath = await save({
        filters: [{ name: 'Log File', extensions: ['log', 'txt'] }],
        defaultPath: `mythic-logs-${Date.now()}.log`,
      });
      if (savePath) {
        await writeTextFile(savePath, combined);
        success('Logs exported');
      }
    } catch (err) {
      toastError('Failed to export logs');
      console.error('Log export failed:', err);
    }
    isExportingLogs = false;
  }
</script>

<div class="panel-heading animate-fade-in-up stagger-4">
  <span class="panel-heading-title">Logging</span>
  <span class="panel-heading-desc">Backend and frontend activity, for diagnosing issues without guessing</span>
</div>

<section class="settings-section animate-fade-in-up stagger-4">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="terminal" size={16} color="var(--accent-primary)" />
      <span class="section-title">Application Logs</span>
    </div>
    <button class="reset-btn" onclick={loadBackendLogs} disabled={isLoadingBackendLogs}>
      {isLoadingBackendLogs ? 'Refreshing…' : 'Refresh'}
    </button>
  </div>

  <div class="log-toolbar">
    <div class="log-subtabs">
      <button class="log-subtab" class:active={logSubTab === 'backend'} onclick={() => selectLogSubTab('backend')}>
        Backend
      </button>
      <button class="log-subtab" class:active={logSubTab === 'frontend'} onclick={() => selectLogSubTab('frontend')}>
        Frontend <span class="log-subtab-count">{$frontendLogs.length}</span>
      </button>
    </div>
    <input class="log-search" type="text" placeholder="Search logs…" bind:value={logSearch} />
  </div>

  {#if logSubTab === 'backend'}
    {#if backendLogPath}
      <span class="prompt-hint log-path" title={backendLogPath}>{backendLogPath}</span>
    {/if}
    <div class="log-viewer" bind:this={logViewerEl} onscroll={handleLogViewerScroll}>
      {#if isLoadingMoreBackendLogs}
        <div class="log-loading-more">Loading older lines…</div>
      {/if}
      {#if filteredBackendLines.length === 0}
        <div class="log-empty">{backendLogLines.length ? 'No lines match your search.' : (isLoadingBackendLogs ? 'Loading…' : 'No backend logs yet.')}</div>
      {:else}
        {#each filteredBackendLines as line, i (i)}
          <div class="log-line {backendLogLineClass(line)}">{line}</div>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="log-viewer" bind:this={logViewerEl}>
      {#if filteredFrontendEntries.length === 0}
        <div class="log-empty">{$frontendLogs.length ? 'No lines match your search.' : 'No frontend activity captured yet.'}</div>
      {:else}
        {#each filteredFrontendEntries as entry (entry.timestamp + entry.message)}
          <div class="log-line log-line-{entry.level}">
            [{new Date(entry.timestamp).toLocaleTimeString()}] {entry.level.toUpperCase()} {entry.message}
          </div>
        {/each}
      {/if}
    </div>
    <button class="reset-btn" onclick={clearFrontendLogs}>Clear frontend logs</button>
  {/if}

  <div class="log-actions">
    <button class="settings-btn primary" onclick={handleExportLogs} disabled={isExportingLogs}>
      <Icon name="download" size={14} color="#fff" />
      <span>{isExportingLogs ? 'Exporting…' : 'Export Logs'}</span>
    </button>
  </div>
</section>

<style>
  /* ── Logging ── */
  .log-toolbar {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    margin-bottom: 10px; flex-wrap: wrap;
  }
  .log-subtabs {
    display: flex; gap: 4px; padding: 3px; border-radius: 10px;
    background: rgba(0,0,0,0.2); border: 1px solid rgba(139,92,246,0.08);
  }
  .log-subtab {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 7px; border: none; background: none;
    color: #8b8ba7; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer; transition: all 150ms;
  }
  .log-subtab:hover { color: #c8c8e0; }
  .log-subtab.active { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); }
  .log-subtab-count {
    font-size: 10px; font-family: var(--font-mono); color: inherit; opacity: 0.7;
  }
  .log-search {
    flex: 1; min-width: 160px; max-width: 280px;
    padding: 7px 12px; border-radius: 8px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.1);
    color: #c8c8e0; font-size: var(--text-sm); font-family: var(--font-body);
    outline: none; transition: border-color 150ms;
  }
  .log-search:focus { border-color: rgba(139,92,246,0.3); }
  .log-path {
    display: block; margin-bottom: 8px; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; opacity: 0.7;
  }
  .log-viewer {
    max-height: 420px; overflow-y: auto; padding: 10px 12px; border-radius: 10px;
    background: rgba(7,7,18,0.7); border: 1px solid rgba(139,92,246,0.08);
    font-family: var(--font-mono); font-size: 11px; line-height: 1.6;
  }
  .log-loading-more {
    text-align: center; padding: 6px 0 10px; color: #6a6a86; font-size: 10.5px;
    font-family: var(--font-mono); letter-spacing: 0.02em;
  }
  .log-line {
    white-space: pre-wrap; word-break: break-word; color: #7d7d99;
    padding: 1px 0;
  }
  .log-line-error { color: #fb7185; }
  .log-line-warn { color: #fbbf24; }
  .log-line-debug { color: #5a5a7a; }
  .log-empty {
    padding: 20px 0; text-align: center; color: #4a4a6a;
    font-family: var(--font-body); font-size: var(--text-sm);
  }
  .log-actions { display: flex; justify-content: flex-end; margin-top: 12px; }
</style>
