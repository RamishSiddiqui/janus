<script lang="ts">
  import Icon from './Icon.svelte';
  import { success, error as toastError } from '$lib/stores/toast';

  // ── Props ──────────────────────────────────────────────────────────
  interface Props {
    memory: {
      id: string;
      content: string;
      source: string;
      version: number;
      is_canon: boolean;
      character_id: string | null;
      conversation_id: string | null;
      parent_id: string | null;
    } | null;
    links: Array<{
      id: string;
      source_memory_id: string;
      target_conversation_id: string;
      link_type: 'copy' | 'sync';
      direction: 'one_way' | 'two_way';
      sync_mode: 'auto' | 'manual';
      linked_memory_id: string | null;
    }>;
    conversations: Array<{
      id: string;
      title: string;
      character_id: string;
      memory_count: number;
    }>;
    onClose: () => void;
    onShare: (config: {
      sourceMemoryId: string;
      targetConversationId: string;
      linkType: 'copy' | 'sync';
      direction: 'one_way' | 'two_way';
      syncMode: 'auto' | 'manual';
    }) => void;
    onUnlink: (linkId: string) => void;
    onDelete: (memoryId: string) => void;
  }

  let {
    memory,
    links,
    conversations,
    onClose,
    onShare,
    onUnlink,
    onDelete,
  }: Props = $props();

  // ── Local state ────────────────────────────────────────────────────
  let selectedConvId = $state<string | null>(null);
  let linkType = $state<'copy' | 'sync'>('copy');
  let direction = $state<'one_way' | 'two_way'>('one_way');
  let syncMode = $state<'auto' | 'manual'>('auto');
  let convDropdownOpen = $state(false);
  let confirmDeleteId = $state<string | null>(null);
  let confirmDeleteMemory = $state(false);
  let isClosing = $state(false);
  let panelEl: HTMLElement | undefined = $state();
  let dropdownEl: HTMLDivElement | undefined = $state();

  // ── Category parsing (mirrors MemoryNode) ──────────────────────────
  const categoryMeta: Record<string, { icon: string; label: string }> = {
    trait:         { icon: 'sparkles',      label: 'Trait' },
    event:        { icon: 'clock',          label: 'Event' },
    relationship: { icon: 'heart',          label: 'Relationship' },
    preference:   { icon: 'star',           label: 'Preference' },
    goal:         { icon: 'send',           label: 'Goal' },
    discovery:    { icon: 'search',         label: 'Discovery' },
    fact:         { icon: 'file-text',      label: 'Fact' },
  };

  let parsed = $derived.by(() => {
    if (!memory) return { category: 'fact', text: '', icon: 'file-text', label: 'Fact' };
    const match = memory.content?.match(/^\[(\w+)\]\s*/);
    const category = match ? match[1].toLowerCase() : 'fact';
    const text = match ? memory.content.slice(match[0].length) : memory.content;
    const meta = categoryMeta[category] ?? categoryMeta.fact;
    return { category, text, ...meta };
  });

  // Accent color per category
  const categoryColors: Record<string, string> = {
    trait: '#c4a1ff',
    event: '#00f2ff',
    relationship: '#f472b6',
    preference: '#fbbf24',
    goal: '#34d399',
    discovery: '#818cf8',
    fact: '#8b8ba7',
  };

  let accentColor = $derived(categoryColors[parsed.category] ?? '#c4a1ff');

  // ── Derived: links for THIS memory ─────────────────────────────────
  let activeLinks = $derived(
    memory ? links.filter(l => l.source_memory_id === memory.id) : []
  );

  // ── Derived: conversations available to share to ───────────────────
  // Filter out conversations that already have a link from this memory
  let linkedConvIds = $derived(new Set(activeLinks.map(l => l.target_conversation_id)));
  let availableConversations = $derived(
    conversations.filter(c => !linkedConvIds.has(c.id))
  );

  // ── Derived: can create link ───────────────────────────────────────
  let canCreateLink = $derived(!!selectedConvId && !!memory);

  // ── Conversation lookup helper ─────────────────────────────────────
  function getConversationTitle(convId: string): string {
    return conversations.find(c => c.id === convId)?.title ?? 'Unknown Timeline';
  }

  // ── Actions ────────────────────────────────────────────────────────
  function handleShare() {
    if (!memory || !selectedConvId) return;
    onShare({
      sourceMemoryId: memory.id,
      targetConversationId: selectedConvId,
      linkType,
      direction,
      syncMode,
    });
    selectedConvId = null;
    success('Link created');
  }

  function handleUnlink(linkId: string) {
    if (confirmDeleteId === linkId) {
      onUnlink(linkId);
      confirmDeleteId = null;
      success('Link removed');
    } else {
      confirmDeleteId = linkId;
      // Auto-dismiss confirm after 3s
      setTimeout(() => { if (confirmDeleteId === linkId) confirmDeleteId = null; }, 3000);
    }
  }

  function handleDeleteMemory() {
    if (!memory) return;
    if (confirmDeleteMemory) {
      onDelete(memory.id);
      confirmDeleteMemory = false;
    } else {
      confirmDeleteMemory = true;
      setTimeout(() => { confirmDeleteMemory = false; }, 3000);
    }
  }

  function handleClose() {
    isClosing = true;
    setTimeout(() => {
      isClosing = false;
      onClose();
    }, 200);
  }

  // ── Keyboard & click-outside ───────────────────────────────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') handleClose();
  }

  function handleBackdropClick(e: MouseEvent) {
    // Close dropdown if clicking outside it
    if (dropdownEl && !dropdownEl.contains(e.target as Node)) {
      convDropdownOpen = false;
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if (panelEl && !panelEl.contains(e.target as Node)) {
      handleClose();
    }
  }

  // Reset form state when memory changes
  $effect(() => {
    if (memory) {
      selectedConvId = null;
      linkType = 'copy';
      direction = 'one_way';
      syncMode = 'auto';
      confirmDeleteId = null;
      confirmDeleteMemory = false;
    }
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if memory}
  <!-- Backdrop overlay -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="map-backdrop"
    class:closing={isClosing}
    onclick={handleOverlayClick}
    onmousedown={handleBackdropClick}
  >
    <!-- Panel -->
    <aside
      class="action-panel"
      class:closing={isClosing}
      bind:this={panelEl}
      role="dialog"
      aria-label="Memory actions"
      aria-modal="true"
    >
      <!-- ═══ Scrollable content wrapper ═══ -->
      <div class="panel-scroll">

        <!-- ── 1. Header ──────────────────────────────────────── -->
        <header class="panel-header">
          <div class="header-top">
            <div class="category-label" style="color: {accentColor};">
              <Icon name={parsed.icon} size={14} color={accentColor} />
              <span>{parsed.label}</span>
            </div>
            <button class="close-btn" onclick={handleClose} aria-label="Close panel">
              <Icon name="x" size={15} color="#5a5a7a" />
            </button>
          </div>

          <div class="header-badges">
            <span class="badge source-badge" class:auto={memory.source === 'auto'}>
              <Icon
                name={memory.source === 'auto' ? 'cpu' : 'pin'}
                size={10}
                color={memory.source === 'auto' ? '#818cf8' : '#f59e0b'}
              />
              {memory.source === 'auto' ? 'AI' : 'Pinned'}
            </span>
            {#if memory.version > 1}
              <span class="badge version-badge">v{memory.version}</span>
            {/if}
            {#if memory.is_canon}
              <span class="badge canon-badge">
                <span class="canon-dot"></span>
                Canon
              </span>
            {/if}
          </div>
        </header>

        <!-- ── 2. Content Preview ─────────────────────────────── -->
        <section class="content-section">
          <div class="content-card" style="--card-accent: {accentColor};">
            <div class="content-accent-strip"></div>
            <p class="content-text">{parsed.text}</p>
          </div>
        </section>

        <!-- ── 3. Active Links ────────────────────────────────── -->
        {#if activeLinks.length > 0}
          <section class="links-section">
            <div class="section-header">
              <span class="section-title">ACTIVE LINKS</span>
              <span class="link-count-badge">{activeLinks.length}</span>
            </div>

            <div class="link-list">
              {#each activeLinks as link (link.id)}
                <div class="link-card" class:confirming={confirmDeleteId === link.id}>
                  <div class="link-info">
                    <span class="link-conv-title">{getConversationTitle(link.target_conversation_id)}</span>
                    <div class="link-meta-row">
                      <span
                        class="link-type-pill"
                        class:sync={link.link_type === 'sync'}
                        class:copy={link.link_type === 'copy'}
                      >
                        {#if link.link_type === 'sync'}
                          <span class="sync-pulse-dot"></span>
                        {/if}
                        {link.link_type === 'copy' ? 'Copy' : 'Sync'}
                      </span>
                      <span class="link-direction">
                        {link.direction === 'one_way' ? '→' : '⇌'}
                      </span>
                      <span class="link-sync-mode">{link.sync_mode}</span>
                    </div>
                  </div>
                  <button
                    class="unlink-btn"
                    class:confirming={confirmDeleteId === link.id}
                    onclick={() => handleUnlink(link.id)}
                    title={confirmDeleteId === link.id ? 'Click again to confirm' : 'Remove link'}
                    aria-label="Remove link"
                  >
                    {#if confirmDeleteId === link.id}
                      <span class="confirm-text">Confirm?</span>
                    {:else}
                      <Icon name="trash-2" size={13} color="currentColor" />
                    {/if}
                  </button>
                </div>
              {/each}
            </div>
          </section>
        {/if}

        <!-- ── 4. Share Action ────────────────────────────────── -->
        <section class="share-section">
          <div class="section-header">
            <span class="section-title">SHARE TO TIMELINE</span>
            <Icon name="send" size={11} color="#5a5a7a" />
          </div>

          <!-- Conversation Picker -->
          <div class="conv-picker" bind:this={dropdownEl}>
            <button
              class="conv-trigger"
              onclick={() => convDropdownOpen = !convDropdownOpen}
              disabled={availableConversations.length === 0}
            >
              <Icon name="message-circle" size={13} color={selectedConvId ? '#c4a1ff' : '#5a5a7a'} />
              <span class="conv-trigger-label">
                {#if selectedConvId}
                  {getConversationTitle(selectedConvId)}
                {:else if availableConversations.length === 0}
                  No conversations available
                {:else}
                  Select conversation...
                {/if}
              </span>
              <span class="conv-chevron" class:open={convDropdownOpen}>
                <Icon name="chevron-down" size={12} color="#5a5a7a" />
              </span>
            </button>

            {#if convDropdownOpen && availableConversations.length > 0}
              <div class="conv-dropdown">
                {#each availableConversations as conv (conv.id)}
                  <button
                    class="conv-option"
                    class:selected={selectedConvId === conv.id}
                    onclick={() => { selectedConvId = conv.id; convDropdownOpen = false; }}
                  >
                    <span class="conv-option-title">{conv.title}</span>
                    <span class="conv-option-count">{conv.memory_count} memories</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>

          <!-- Link Type Toggle -->
          <div class="toggle-group">
            <span class="toggle-label">Link Type</span>
            <div class="pill-toggle">
              <button
                class="pill-btn"
                class:active={linkType === 'copy'}
                class:purple={linkType === 'copy'}
                onclick={() => linkType = 'copy'}
              >
                <Icon name="copy" size={11} color={linkType === 'copy' ? '#fff' : '#6b6b8a'} />
                Copy
              </button>
              <button
                class="pill-btn"
                class:active={linkType === 'sync'}
                class:cyan={linkType === 'sync'}
                onclick={() => linkType = 'sync'}
              >
                <Icon name="refresh-cw" size={11} color={linkType === 'sync' ? '#fff' : '#6b6b8a'} />
                Sync
              </button>
            </div>
            <span class="toggle-desc">
              {linkType === 'copy' ? 'Frozen snapshot' : 'Live-linked'}
            </span>
          </div>

          <!-- Direction Toggle (only for sync) -->
          {#if linkType === 'sync'}
            <div class="toggle-group" style="animation: fadeSlideIn 200ms ease-out;">
              <span class="toggle-label">Direction</span>
              <div class="pill-toggle">
                <button
                  class="pill-btn"
                  class:active={direction === 'one_way'}
                  class:purple={direction === 'one_way'}
                  onclick={() => direction = 'one_way'}
                >
                  <span class="direction-arrow">→</span>
                  One-way
                </button>
                <button
                  class="pill-btn"
                  class:active={direction === 'two_way'}
                  class:cyan={direction === 'two_way'}
                  onclick={() => direction = 'two_way'}
                >
                  <span class="direction-arrow">⇌</span>
                  Two-way
                </button>
              </div>
            </div>
          {/if}

          <!-- Sync Mode Toggle -->
          <div class="toggle-group">
            <span class="toggle-label">Sync Mode</span>
            <div class="pill-toggle">
              <button
                class="pill-btn"
                class:active={syncMode === 'auto'}
                class:purple={syncMode === 'auto'}
                onclick={() => syncMode = 'auto'}
              >
                Auto
              </button>
              <button
                class="pill-btn"
                class:active={syncMode === 'manual'}
                class:purple={syncMode === 'manual'}
                onclick={() => syncMode = 'manual'}
              >
                Manual
              </button>
            </div>
          </div>

          <!-- Create Link Button -->
          <button
            class="create-link-btn"
            disabled={!canCreateLink}
            onclick={handleShare}
          >
            <Icon name="send" size={13} color="#fff" />
            Create Link
          </button>
        </section>

        <!-- ── 5. Actions Footer ──────────────────────────────── -->
        <footer class="actions-footer">
          <button
            class="delete-memory-btn"
            class:confirming={confirmDeleteMemory}
            onclick={handleDeleteMemory}
          >
            <Icon name="trash-2" size={13} color="currentColor" />
            {confirmDeleteMemory ? 'Click again to confirm deletion' : 'Delete Memory'}
          </button>
        </footer>

      </div>
    </aside>
  </div>
{/if}

<style>
  /* ═══════════════════════════════════════════════════════════════════
     Backdrop
     ═══════════════════════════════════════════════════════════════════ */
  .map-backdrop {
    position: fixed;
    inset: 0;
    z-index: 900;
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(2px);
    animation: backdropIn 250ms ease-out both;
  }

  .map-backdrop.closing {
    animation: backdropOut 200ms ease-in both;
  }

  @keyframes backdropIn {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  @keyframes backdropOut {
    from { opacity: 1; }
    to   { opacity: 0; }
  }

  /* ═══════════════════════════════════════════════════════════════════
     Panel
     ═══════════════════════════════════════════════════════════════════ */
  .action-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 380px;
    height: 100%;
    background: rgba(10, 10, 26, 0.96);
    backdrop-filter: blur(24px);
    border-left: 1px solid rgba(139, 92, 246, 0.15);
    box-shadow:
      -8px 0 40px rgba(0, 0, 0, 0.5),
      -1px 0 0 rgba(139, 92, 246, 0.06),
      inset 1px 0 0 rgba(139, 92, 246, 0.03);
    z-index: 910;
    display: flex;
    flex-direction: column;
    font-family: var(--font-body, 'Inter', -apple-system, sans-serif);
    animation: panelSlideIn 380ms cubic-bezier(0.32, 0.72, 0, 1) both;
  }

  .action-panel.closing {
    animation: panelSlideOut 200ms cubic-bezier(0.4, 0, 1, 1) both;
  }

  @keyframes panelSlideIn {
    from { transform: translateX(100%); }
    to   { transform: translateX(0); }
  }

  @keyframes panelSlideOut {
    from { transform: translateX(0); }
    to   { transform: translateX(100%); }
  }

  /* ── Scrollable Content ── */
  .panel-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 20px 18px 24px;
  }

  .panel-scroll::-webkit-scrollbar { width: 3px; }
  .panel-scroll::-webkit-scrollbar-thumb {
    background: rgba(139, 92, 246, 0.15);
    border-radius: 3px;
  }
  .panel-scroll::-webkit-scrollbar-track { background: transparent; }

  /* ═══════════════════════════════════════════════════════════════════
     1. Header
     ═══════════════════════════════════════════════════════════════════ */
  .panel-header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(139, 92, 246, 0.06);
    margin-bottom: 16px;
  }

  .header-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .category-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 1.2px;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    border: none;
    background: rgba(139, 92, 246, 0.04);
    cursor: pointer;
    transition: all 150ms;
  }

  .close-btn:hover {
    background: rgba(139, 92, 246, 0.1);
  }

  .close-btn:active {
    transform: scale(0.92);
  }

  .header-badges {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 6px;
    letter-spacing: 0.3px;
  }

  .source-badge {
    background: rgba(129, 140, 248, 0.1);
    color: #818cf8;
  }

  .source-badge:not(.auto) {
    background: rgba(245, 158, 11, 0.1);
    color: #f59e0b;
  }

  .version-badge {
    background: rgba(196, 161, 255, 0.1);
    color: #c4a1ff;
  }

  .canon-badge {
    background: rgba(218, 165, 32, 0.12);
    color: #fbbf24;
  }

  .canon-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #fbbf24;
    box-shadow: 0 0 4px rgba(218, 165, 32, 0.5);
  }

  /* ═══════════════════════════════════════════════════════════════════
     2. Content Preview
     ═══════════════════════════════════════════════════════════════════ */
  .content-section {
    margin-bottom: 16px;
  }

  .content-card {
    display: flex;
    background: linear-gradient(135deg, #0e0e1e, #141028);
    border: 1px solid rgba(45, 36, 88, 0.6);
    border-radius: 12px;
    overflow: hidden;
  }

  .content-accent-strip {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(
      180deg,
      var(--card-accent),
      color-mix(in srgb, var(--card-accent) 25%, transparent)
    );
  }

  .content-text {
    flex: 1;
    padding: 12px 14px;
    font-size: 13px;
    line-height: 1.6;
    color: #cbc5dd;
    margin: 0;
    word-wrap: break-word;
  }

  /* ═══════════════════════════════════════════════════════════════════
     Section headers (shared)
     ═══════════════════════════════════════════════════════════════════ */
  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 700;
    color: #5a5a7a;
    letter-spacing: 1px;
    text-transform: uppercase;
  }

  /* ═══════════════════════════════════════════════════════════════════
     3. Active Links
     ═══════════════════════════════════════════════════════════════════ */
  .links-section {
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(139, 92, 246, 0.06);
    margin-bottom: 16px;
  }

  .link-count-badge {
    font-size: 10px;
    font-weight: 700;
    color: #c4a1ff;
    background: rgba(139, 92, 246, 0.12);
    padding: 1px 7px;
    border-radius: 8px;
    min-width: 18px;
    text-align: center;
  }

  .link-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .link-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgba(14, 14, 30, 0.5);
    border: 1px solid rgba(139, 92, 246, 0.06);
    transition: all 180ms ease-out;
  }

  .link-card:hover {
    background: rgba(139, 92, 246, 0.04);
    border-color: rgba(139, 92, 246, 0.1);
  }

  .link-card.confirming {
    border-color: rgba(239, 68, 68, 0.2);
    background: rgba(239, 68, 68, 0.04);
  }

  .link-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .link-conv-title {
    font-size: 12px;
    font-weight: 600;
    color: #c8c8e0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .link-meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .link-type-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 5px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .link-type-pill.copy {
    background: rgba(139, 92, 246, 0.15);
    color: #c4a1ff;
  }

  .link-type-pill.sync {
    background: rgba(0, 242, 255, 0.12);
    color: #00f2ff;
  }

  .sync-pulse-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #00f2ff;
    animation: syncPulse 2s ease-in-out infinite;
  }

  @keyframes syncPulse {
    0%, 100% { opacity: 0.4; box-shadow: 0 0 0 0 rgba(0, 242, 255, 0); }
    50%      { opacity: 1;   box-shadow: 0 0 6px 2px rgba(0, 242, 255, 0.3); }
  }

  .link-direction {
    font-size: 13px;
    color: #6b6b8a;
    font-weight: 600;
    line-height: 1;
  }

  .link-sync-mode {
    font-size: 9px;
    color: #5a5a7a;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 600;
  }

  .unlink-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 28px;
    border-radius: 7px;
    border: 1px solid transparent;
    background: transparent;
    color: #5a5a7a;
    cursor: pointer;
    transition: all 180ms;
    flex-shrink: 0;
    padding: 0 4px;
  }

  .unlink-btn:hover {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.15);
  }

  .unlink-btn.confirming {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.12);
    border-color: rgba(239, 68, 68, 0.25);
  }

  .confirm-text {
    font-size: 9px;
    font-weight: 700;
    white-space: nowrap;
    letter-spacing: 0.3px;
  }

  /* ═══════════════════════════════════════════════════════════════════
     4. Share Action
     ═══════════════════════════════════════════════════════════════════ */
  .share-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(139, 92, 246, 0.06);
    margin-bottom: 16px;
  }

  /* ── Conversation Picker ── */
  .conv-picker {
    position: relative;
  }

  .conv-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 9px 12px;
    background: rgba(14, 14, 30, 0.6);
    border: 1px solid rgba(139, 92, 246, 0.1);
    border-radius: 10px;
    cursor: pointer;
    transition: all 200ms;
    font-family: var(--font-body, 'Inter', sans-serif);
    text-align: left;
  }

  .conv-trigger:hover:not(:disabled) {
    border-color: rgba(139, 92, 246, 0.25);
    background: rgba(14, 14, 30, 0.8);
  }

  .conv-trigger:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .conv-trigger-label {
    flex: 1;
    font-size: 12px;
    font-weight: 500;
    color: #8b8ba7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conv-chevron {
    display: flex;
    transition: transform 200ms;
  }

  .conv-chevron.open {
    transform: rotate(180deg);
  }

  .conv-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    max-height: 200px;
    overflow-y: auto;
    background: rgba(12, 12, 28, 0.98);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(139, 92, 246, 0.12);
    border-radius: 10px;
    padding: 4px;
    z-index: 50;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6);
    animation: dropIn 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .conv-dropdown::-webkit-scrollbar { width: 3px; }
  .conv-dropdown::-webkit-scrollbar-thumb {
    background: rgba(139, 92, 246, 0.15);
    border-radius: 3px;
  }

  @keyframes dropIn {
    from { opacity: 0; transform: translateY(-6px) scale(0.97); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .conv-option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 12px;
    font-size: 12px;
    font-weight: 500;
    color: #8b8ba7;
    background: none;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: all 150ms;
    font-family: var(--font-body, 'Inter', sans-serif);
    text-align: left;
  }

  .conv-option:hover {
    background: rgba(139, 92, 246, 0.08);
    color: #e8e0ff;
  }

  .conv-option.selected {
    background: rgba(139, 92, 246, 0.12);
    color: #c4a1ff;
  }

  .conv-option-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conv-option-count {
    font-size: 10px;
    color: #4a4a6a;
    flex-shrink: 0;
    margin-left: 8px;
  }

  /* ── Toggle Group ── */
  .toggle-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .toggle-label {
    font-size: 10px;
    font-weight: 600;
    color: #5a5a7a;
    text-transform: uppercase;
    letter-spacing: 0.8px;
  }

  .toggle-desc {
    font-size: 10px;
    color: #4a4a6a;
    font-style: italic;
  }

  .pill-toggle {
    display: flex;
    gap: 0;
    background: rgba(14, 14, 30, 0.5);
    border: 1px solid rgba(139, 92, 246, 0.08);
    border-radius: 10px;
    padding: 3px;
  }

  .pill-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
    color: #6b6b8a;
    background: transparent;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: all 200ms ease-out;
    font-family: var(--font-body, 'Inter', sans-serif);
    position: relative;
  }

  .pill-btn:hover:not(.active) {
    color: #8b8ba7;
    background: rgba(139, 92, 246, 0.04);
  }

  .pill-btn.active.purple {
    background: rgba(139, 92, 246, 0.2);
    color: #e8e0ff;
    box-shadow: 0 0 12px rgba(139, 92, 246, 0.1);
  }

  .pill-btn.active.cyan {
    background: rgba(0, 242, 255, 0.15);
    color: #e0fffe;
    box-shadow: 0 0 12px rgba(0, 242, 255, 0.08);
  }

  .direction-arrow {
    font-size: 14px;
    line-height: 1;
  }

  @keyframes fadeSlideIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* ── Create Link Button ── */
  .create-link-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 16px;
    border-radius: 10px;
    border: none;
    background: linear-gradient(135deg, #8B5CF6, #7c3aed);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    font-family: var(--font-body, 'Inter', sans-serif);
    cursor: pointer;
    transition: all 200ms;
    box-shadow: 0 4px 16px rgba(139, 92, 246, 0.25);
    margin-top: 4px;
  }

  .create-link-btn:hover:not(:disabled) {
    box-shadow: 0 6px 24px rgba(139, 92, 246, 0.4);
    transform: translateY(-1px);
  }

  .create-link-btn:active:not(:disabled) {
    transform: translateY(0px) scale(0.98);
  }

  .create-link-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
    box-shadow: none;
  }

  /* ═══════════════════════════════════════════════════════════════════
     5. Actions Footer
     ═══════════════════════════════════════════════════════════════════ */
  .actions-footer {
    margin-top: auto;
    padding-top: 12px;
  }

  .delete-memory-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    padding: 10px 14px;
    border-radius: 10px;
    border: 1px solid rgba(239, 68, 68, 0.08);
    background: rgba(239, 68, 68, 0.03);
    color: #6b6b8a;
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-body, 'Inter', sans-serif);
    cursor: pointer;
    transition: all 200ms;
  }

  .delete-memory-btn:hover {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.2);
  }

  .delete-memory-btn.confirming {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.12);
    border-color: rgba(239, 68, 68, 0.3);
    animation: confirmShake 300ms ease-out;
  }

  @keyframes confirmShake {
    0%, 100% { transform: translateX(0); }
    25% { transform: translateX(-3px); }
    75% { transform: translateX(3px); }
  }

  /* ═══════════════════════════════════════════════════════════════════
     Light Theme Overrides
     ═══════════════════════════════════════════════════════════════════ */
  :global([data-theme="light"]) .action-panel {
    background: rgba(255, 255, 255, 0.96);
    border-left-color: rgba(139, 92, 246, 0.1);
    box-shadow:
      -8px 0 40px rgba(0, 0, 0, 0.08),
      -1px 0 0 rgba(139, 92, 246, 0.06);
  }

  :global([data-theme="light"]) .map-backdrop {
    background: rgba(0, 0, 0, 0.15);
  }

  :global([data-theme="light"]) .panel-header {
    border-bottom-color: rgba(139, 92, 246, 0.08);
  }

  :global([data-theme="light"]) .close-btn {
    background: rgba(139, 92, 246, 0.06);
  }

  :global([data-theme="light"]) .close-btn:hover {
    background: rgba(139, 92, 246, 0.12);
  }

  :global([data-theme="light"]) .content-card {
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.9), rgba(245, 242, 250, 0.95));
    border-color: rgba(139, 92, 246, 0.1);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
  }

  :global([data-theme="light"]) .content-text {
    color: #2a2a3e;
  }

  :global([data-theme="light"]) .section-title {
    color: #7c7c9a;
  }

  :global([data-theme="light"]) .link-card {
    background: rgba(245, 242, 250, 0.6);
    border-color: rgba(139, 92, 246, 0.06);
  }

  :global([data-theme="light"]) .link-card:hover {
    background: rgba(139, 92, 246, 0.04);
  }

  :global([data-theme="light"]) .link-conv-title {
    color: #2a2a3e;
  }

  :global([data-theme="light"]) .conv-trigger {
    background: rgba(245, 242, 250, 0.7);
    border-color: rgba(139, 92, 246, 0.1);
  }

  :global([data-theme="light"]) .conv-trigger:hover:not(:disabled) {
    background: rgba(245, 242, 250, 0.9);
    border-color: rgba(139, 92, 246, 0.2);
  }

  :global([data-theme="light"]) .conv-trigger-label {
    color: #4a4a6a;
  }

  :global([data-theme="light"]) .conv-dropdown {
    background: rgba(255, 255, 255, 0.98);
    border-color: rgba(139, 92, 246, 0.1);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.12);
  }

  :global([data-theme="light"]) .conv-option {
    color: #4a4a6a;
  }

  :global([data-theme="light"]) .conv-option:hover {
    background: rgba(139, 92, 246, 0.06);
    color: #2a2a3e;
  }

  :global([data-theme="light"]) .pill-toggle {
    background: rgba(245, 242, 250, 0.5);
    border-color: rgba(139, 92, 246, 0.08);
  }

  :global([data-theme="light"]) .pill-btn {
    color: #7c7c9a;
  }

  :global([data-theme="light"]) .pill-btn:hover:not(.active) {
    color: #4a4a6a;
    background: rgba(139, 92, 246, 0.04);
  }

  :global([data-theme="light"]) .toggle-label {
    color: #7c7c9a;
  }

  :global([data-theme="light"]) .toggle-desc {
    color: #9c9cb4;
  }

  :global([data-theme="light"]) .delete-memory-btn {
    color: #9c9cb4;
    background: rgba(239, 68, 68, 0.02);
    border-color: rgba(239, 68, 68, 0.06);
  }

  :global([data-theme="light"]) .delete-memory-btn:hover {
    color: #dc2626;
    background: rgba(239, 68, 68, 0.06);
  }

  :global([data-theme="light"]) .delete-memory-btn.confirming {
    color: #dc2626;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.2);
  }

  :global([data-theme="light"]) .links-section {
    border-bottom-color: rgba(139, 92, 246, 0.06);
  }

  :global([data-theme="light"]) .share-section {
    border-bottom-color: rgba(139, 92, 246, 0.06);
  }

  :global([data-theme="light"]) .unlink-btn {
    color: #9c9cb4;
  }

  :global([data-theme="light"]) .badge {
    background: rgba(139, 92, 246, 0.08);
  }

  :global([data-theme="light"]) .source-badge {
    background: rgba(129, 140, 248, 0.08);
  }

  :global([data-theme="light"]) .source-badge:not(.auto) {
    background: rgba(245, 158, 11, 0.08);
  }

  :global([data-theme="light"]) .link-direction {
    color: #9c9cb4;
  }

  :global([data-theme="light"]) .link-sync-mode {
    color: #9c9cb4;
  }
</style>
