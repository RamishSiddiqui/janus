<script lang="ts">
  import Icon from './Icon.svelte';
  import EmotionHUD from './EmotionHUD.svelte';
  import type { Message } from '$lib/types';
  import { formatRoleplayContent } from '$lib/utils/format';
  import { browser } from '$app/environment';
  import { messages, activeConversationId, activeCharacterId, isStreaming, switchBranch, regenerateMessage as storeRegenerate, characterEmotionState } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import { get } from 'svelte/store';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { message, onBranch }: { message: Message; onBranch?: (id: string) => void } = $props();

  let showActions = $state(false);
  let isRegenerating = $state(false);
  let isEditing = $state(false);
  let editContent = $state('');
  let copied = $state(false);
  let isSwitching = $state(false);

  // Reactively derived from the store — updates live after each stream completes
  let emotionState = $derived($characterEmotionState);

  // Branch navigation
  let hasBranches = $derived((message.siblingIds?.length ?? 0) > 1);
  let branchIndex = $derived(message.siblingIndex ?? 0);
  let branchTotal = $derived(message.siblingIds?.length ?? 1);
  let canPrev = $derived(hasBranches && branchIndex > 0);
  let canNext = $derived(hasBranches && branchIndex < branchTotal - 1);

  async function navigateBranch(direction: -1 | 1) {
    if (!message.siblingIds || isSwitching) return;
    const newIndex = branchIndex + direction;
    if (newIndex < 0 || newIndex >= branchTotal) return;
    isSwitching = true;
    await switchBranch(message.siblingIds[newIndex]);
    // isSwitching resets naturally when messages reload
    setTimeout(() => isSwitching = false, 600);
  }

  async function handleRegenerate() {
    if (!isTauri || isRegenerating || get(isStreaming)) return;
    isRegenerating = true;
    const convId = get(activeConversationId);
    if (convId) {
      await storeRegenerate(convId, message.id);
    }
    isRegenerating = false;
  }

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => copied = false, 1500);
    } catch (err) {
      toastError('Failed to copy message');
    }
  }

  function startEdit() {
    editContent = message.content;
    isEditing = true;
  }

  async function saveEdit() {
    if (!isTauri) { isEditing = false; return; }
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.updateMessage(message.id, editContent);
      messages.update(msgs => msgs.map(m => m.id === message.id ? { ...m, content: editContent } : m));
      success('Message updated');
    } catch (err) {
      toastError('Failed to update message');
    }
    isEditing = false;
  }

  function cancelEdit() {
    isEditing = false;
  }

  async function handleDelete() {
    if (!isTauri) return;
    try {
      const ipc = await import('$lib/services/ipc');
      await ipc.deleteMessage(message.id);
      messages.update(msgs => msgs.filter(m => m.id !== message.id));
      success('Message deleted');
    } catch (err) {
      toastError('Failed to delete message');
    }
  }
</script>

{#if message.role === 'assistant'}
  <div 
    class="message ai-message"
    role="article"
    aria-label="AI message"
    onmouseenter={() => showActions = true}
    onmouseleave={() => showActions = false}
    onfocusin={() => showActions = true}
    onfocusout={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) showActions = false; }}
  >
    <div class="msg-avatar ai-avatar" aria-hidden="true">
      <div class="avatar-glow"></div>
    </div>
    <div class="msg-body">
      <div class="msg-bubble ai-bubble" class:switching={isSwitching}>
        {#if isSwitching}
          <div class="timeline-shift-overlay" aria-hidden="true">
            <span class="shift-text">Shifting timeline…</span>
            <div class="shift-particles">
              {#each [0,1,2,3,4] as i}
                <span class="shift-particle" style="--i:{i}"></span>
              {/each}
            </div>
          </div>
        {/if}
        {#if isEditing}
          <textarea class="edit-area" bind:value={editContent} rows="4" aria-label="Edit message content"></textarea>
          <div class="edit-actions">
            <button class="edit-btn save" onclick={saveEdit}>
              <Icon name="check" size={12} color="#FFFFFF" />
              <span>Save</span>
            </button>
            <button class="edit-btn cancel" onclick={cancelEdit}>
              <span>Cancel</span>
            </button>
          </div>
        {:else}
          <div class="msg-text" class:dim={isSwitching}>
            {@html formatRoleplayContent(message.content)}
            {#if message.isStreaming}
              <span class="streaming-cursor cursor-blink" aria-label="Generating">▍</span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Action Bar -->
      <div class="msg-toolbar" class:visible={showActions || hasBranches} role="toolbar" aria-label="Message actions">
        <!-- Quantum Timeline Branch Navigator -->
        {#if hasBranches}
          <div class="timeline-nav" aria-label="Timeline navigation" title="Alternate AI responses — navigate parallel timelines">
            <button 
              class="tl-arrow tl-prev"
              disabled={!canPrev || isSwitching}
              onclick={() => navigateBranch(-1)}
              aria-label="Previous timeline"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <path d="M6.5 1.5L3 5l3.5 3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>

            <!-- Dot Track -->
            <div class="tl-track" aria-hidden="true">
              {#each Array(branchTotal) as _, i}
                <button
                  class="tl-dot"
                  class:active={i === branchIndex}
                  class:visited={i < branchIndex}
                  disabled={isSwitching}
                  onclick={() => {
                    if (message.siblingIds && !isSwitching) {
                      isSwitching = true;
                      switchBranch(message.siblingIds[i]).finally(() => setTimeout(() => isSwitching = false, 600));
                    }
                  }}
                  aria-label="Timeline {i + 1}"
                  title="Timeline {i + 1} of {branchTotal}"
                ></button>
              {/each}
            </div>

            <span class="tl-counter" title="{branchIndex + 1} of {branchTotal} timelines">
              <span class="tl-cur">{branchIndex + 1}</span>
              <span class="tl-slash">/</span>
              <span class="tl-total">{branchTotal}</span>
            </span>

            <button 
              class="tl-arrow tl-next"
              disabled={!canNext || isSwitching}
              onclick={() => navigateBranch(1)}
              aria-label="Next timeline"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <path d="M3.5 1.5L7 5l-3.5 3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
          <span class="toolbar-divider"></span>
        {/if}

        <!-- Emotion HUD -->
        {#if emotionState && message.role === 'assistant'}
          <EmotionHUD state={emotionState} />
          <span class="toolbar-divider"></span>
        {/if}

        <!-- Action Buttons -->
        <div class="action-group" class:visible={showActions}>
          <button 
            class="action-btn" 
            class:spin={isRegenerating}
            title="Regenerate" 
            aria-label="Regenerate response"
            onclick={handleRegenerate}
          >
            <Icon name="refresh-cw" size={13} color="var(--fg-muted)" />
          </button>
          <button class="action-btn" title={copied ? 'Copied!' : 'Copy'} aria-label="Copy message" onclick={handleCopy}>
            <Icon name={copied ? 'check' : 'copy'} size={13} color={copied ? 'var(--success)' : 'var(--fg-muted)'} />
          </button>
          <button class="action-btn" title="Edit" aria-label="Edit message" onclick={startEdit}>
            <Icon name="pencil" size={13} color="var(--fg-muted)" />
          </button>
          <button class="action-btn branch-btn" title="Branch from here" aria-label="Branch conversation from this message" onclick={() => onBranch?.(message.id)}>
            <Icon name="git-branch" size={13} color="var(--fg-muted)" />
          </button>
          <button class="action-btn danger-hover" title="Delete" aria-label="Delete message" onclick={handleDelete}>
            <Icon name="trash-2" size={13} color="var(--fg-muted)" />
          </button>
        </div>
      </div>
    </div>
  </div>
{:else}
  <div 
    class="message user-message"
    role="article"
    aria-label="Your message"
    onmouseenter={() => showActions = true}
    onmouseleave={() => showActions = false}
    onfocusin={() => showActions = true}
    onfocusout={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) showActions = false; }}
  >
    <div class="msg-body-user">
      <div class="msg-bubble user-bubble" class:switching={isSwitching}>
        {#if isEditing}
          <textarea class="edit-area user-edit" bind:value={editContent} rows="3" aria-label="Edit message content"></textarea>
          <div class="edit-actions">
            <button class="edit-btn save" onclick={saveEdit}>
              <Icon name="check" size={12} color="#FFFFFF" />
              <span>Save</span>
            </button>
            <button class="edit-btn cancel" onclick={cancelEdit}>
              <span>Cancel</span>
            </button>
          </div>
        {:else}
          <div class="msg-text user-text">
            {@html formatRoleplayContent(message.content)}
          </div>
        {/if}
      </div>

      <!-- User Action Bar -->
      <div class="msg-toolbar user-toolbar" class:visible={showActions || hasBranches} role="toolbar" aria-label="Message actions">
        {#if hasBranches}
          <div class="timeline-nav" aria-label="Timeline navigation">
            <button 
              class="tl-arrow tl-prev"
              disabled={!canPrev || isSwitching}
              onclick={() => navigateBranch(-1)}
              aria-label="Previous timeline"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <path d="M6.5 1.5L3 5l3.5 3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>

            <div class="tl-track" aria-hidden="true">
              {#each Array(branchTotal) as _, i}
                <button
                  class="tl-dot"
                  class:active={i === branchIndex}
                  class:visited={i < branchIndex}
                  disabled={isSwitching}
                  onclick={() => {
                    if (message.siblingIds && !isSwitching) {
                      isSwitching = true;
                      switchBranch(message.siblingIds[i]).finally(() => setTimeout(() => isSwitching = false, 600));
                    }
                  }}
                  aria-label="Timeline {i + 1}"
                ></button>
              {/each}
            </div>

            <span class="tl-counter">
              <span class="tl-cur">{branchIndex + 1}</span>
              <span class="tl-slash">/</span>
              <span class="tl-total">{branchTotal}</span>
            </span>

            <button 
              class="tl-arrow tl-next"
              disabled={!canNext || isSwitching}
              onclick={() => navigateBranch(1)}
              aria-label="Next timeline"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <path d="M3.5 1.5L7 5l-3.5 3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
          <span class="toolbar-divider"></span>
        {/if}

        <div class="action-group" class:visible={showActions}>
          <button class="action-btn" title={copied ? 'Copied!' : 'Copy'} aria-label="Copy message" onclick={handleCopy}>
            <Icon name={copied ? 'check' : 'copy'} size={13} color={copied ? 'var(--success)' : 'var(--fg-muted)'} />
          </button>
          <button class="action-btn" title="Edit" aria-label="Edit message" onclick={startEdit}>
            <Icon name="pencil" size={13} color="var(--fg-muted)" />
          </button>
          <button class="action-btn branch-btn" title="Branch from here" aria-label="Branch conversation from this message" onclick={() => onBranch?.(message.id)}>
            <Icon name="git-branch" size={13} color="var(--fg-muted)" />
          </button>
          <button class="action-btn danger-hover" title="Delete" aria-label="Delete message" onclick={handleDelete}>
            <Icon name="trash-2" size={13} color="var(--fg-muted)" />
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Layout ── */
  .message { display: flex; gap: 12px; width: 100%; }
  .ai-message { align-items: flex-start; }
  .user-message { justify-content: flex-end; }

  /* ── Avatars ── */
  .msg-avatar {
    width: 34px; height: 34px; border-radius: 11px;
    flex-shrink: 0; position: relative;
  }
  .ai-avatar {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 12px rgba(139,92,246,0.2);
  }
  .avatar-glow {
    position: absolute; inset: -3px; border-radius: 13px;
    background: radial-gradient(circle, rgba(139,92,246,0.2) 0%, transparent 70%);
    pointer-events: none; z-index: -1;
  }

  /* ── Bubbles ── */
  .msg-body, .msg-body-user { display: flex; flex-direction: column; gap: 4px; max-width: 620px; }
  .msg-body-user { align-items: flex-end; }

  .msg-bubble { padding: 14px 18px; line-height: 1.65; position: relative; overflow: hidden; }

  .ai-bubble {
    background: rgba(14,14,30,0.7);
    border: 1px solid rgba(139,92,246,0.08);
    border-radius: 4px 16px 16px 16px;
    backdrop-filter: blur(4px);
    transition: border-color 300ms ease;
  }
  .ai-bubble.switching {
    border-color: rgba(139,92,246,0.3);
    box-shadow: 0 0 20px rgba(139,92,246,0.08);
  }
  .user-bubble {
    background: linear-gradient(135deg, #7c3aed, #8B5CF6);
    border-radius: 16px 16px 4px 16px;
    max-width: 520px;
    box-shadow: 0 4px 20px rgba(139,92,246,0.2);
  }

  /* ── Timeline Shift Overlay ── */
  .timeline-shift-overlay {
    position: absolute; inset: 0; z-index: 10;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 8px;
    background: rgba(10,10,26,0.88);
    backdrop-filter: blur(6px);
    animation: shiftFadeIn 220ms cubic-bezier(0.16,1,0.3,1) both;
  }
  @keyframes shiftFadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .shift-text {
    font-size: 11px; font-weight: 600; color: #8b6fc5;
    letter-spacing: 0.8px; text-transform: uppercase;
    animation: shiftPulse 700ms ease-in-out infinite alternate;
  }
  @keyframes shiftPulse {
    from { opacity: 0.5; }
    to { opacity: 1; color: #c4a1ff; }
  }

  .shift-particles { display: flex; gap: 5px; align-items: center; }
  .shift-particle {
    width: 4px; height: 4px; border-radius: 50%;
    background: #8B5CF6;
    animation: particleOrbit 600ms ease-in-out calc(var(--i) * 80ms) infinite alternate;
  }
  @keyframes particleOrbit {
    from { transform: translateY(0) scale(0.6); opacity: 0.3; background: #8B5CF6; }
    to   { transform: translateY(-6px) scale(1); opacity: 1; background: #00f2ff; }
  }

  /* ── Message Text ── */
  .msg-text { font-size: var(--text-base); color: #e0e0f0; word-wrap: break-word; transition: opacity 300ms ease; }
  .msg-text.dim { opacity: 0.25; }
  .msg-text :global(.rp-action) { color: #8b8ba7; font-style: italic; }
  .user-text { color: rgba(255,255,255,0.95); }
  .streaming-cursor { color: #c4a1ff; font-weight: 700; }

  /* ── Toolbar ── */
  .msg-toolbar {
    display: flex; align-items: center; gap: 2px; height: 28px;
    opacity: 0; transform: translateY(2px);
    transition: opacity 180ms ease, transform 180ms ease;
  }
  .msg-toolbar.visible { opacity: 1; transform: translateY(0); }
  .user-toolbar { justify-content: flex-end; }
  .toolbar-divider { width: 1px; height: 14px; background: rgba(139,92,246,0.12); margin: 0 4px; flex-shrink: 0; }

  /* ── Quantum Timeline Navigator ── */
  .timeline-nav {
    display: flex; align-items: center; gap: 4px;
    background: rgba(8,8,20,0.85);
    border: 1px solid rgba(139,92,246,0.18);
    border-radius: 99px;
    padding: 3px 5px;
    backdrop-filter: blur(8px);
    box-shadow: 0 0 0 1px rgba(139,92,246,0.06),
                inset 0 1px 0 rgba(255,255,255,0.03);
    animation: navAppear 250ms cubic-bezier(0.16,1,0.3,1) both;
  }
  @keyframes navAppear {
    from { opacity: 0; transform: translateY(4px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .tl-arrow {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; border: none; background: transparent;
    border-radius: 50%; padding: 0; cursor: pointer; color: #5a5a7a;
    transition: color 150ms, background 150ms, transform 80ms;
    flex-shrink: 0;
  }
  .tl-arrow:not(:disabled):hover {
    color: #c4a1ff;
    background: rgba(139,92,246,0.12);
    transform: scale(1.1);
  }
  .tl-arrow:not(:disabled):active { transform: scale(0.88); }
  .tl-arrow:disabled { opacity: 0.25; cursor: default; }

  /* ── Dot Track ── */
  .tl-track {
    display: flex; align-items: center; gap: 4px;
    padding: 0 2px;
  }

  .tl-dot {
    width: 6px; height: 6px; border-radius: 50%;
    border: none; padding: 0; cursor: pointer;
    background: rgba(139,92,246,0.18);
    transition: transform 200ms cubic-bezier(0.34,1.56,0.64,1),
                background 200ms ease,
                box-shadow 200ms ease,
                width 300ms cubic-bezier(0.34,1.56,0.64,1);
    flex-shrink: 0;
    position: relative;
  }
  .tl-dot:hover {
    background: rgba(139,92,246,0.45);
    transform: scale(1.35);
  }
  .tl-dot:disabled { cursor: default; }
  .tl-dot.visited {
    background: rgba(139,92,246,0.35);
  }
  .tl-dot.active {
    width: 16px;
    border-radius: 3px;
    background: linear-gradient(90deg, #8B5CF6, #00f2ff);
    box-shadow: 0 0 8px rgba(139,92,246,0.6),
                0 0 16px rgba(0,242,255,0.15);
    transform: scale(1);
    animation: activeDotPulse 2s ease-in-out infinite;
  }
  @keyframes activeDotPulse {
    0%, 100% { box-shadow: 0 0 8px rgba(139,92,246,0.5), 0 0 0px rgba(0,242,255,0.1); }
    50%       { box-shadow: 0 0 12px rgba(139,92,246,0.8), 0 0 20px rgba(0,242,255,0.25); }
  }

  /* ── Counter ── */
  .tl-counter {
    display: flex; align-items: baseline; gap: 1px;
    font-family: var(--font-mono); user-select: none;
    font-size: 10px; font-weight: 700; padding: 0 2px;
  }
  .tl-cur { color: #c4a1ff; }
  .tl-slash { color: #3a3a5a; font-weight: 400; margin: 0 1px; }
  .tl-total { color: #4a4a6a; }

  /* ── Action Group ── */
  .action-group { display: flex; gap: 1px; align-items: center; opacity: 0; transition: opacity 150ms; }
  .action-group.visible { opacity: 1; }

  .action-btn {
    background: none; border: none; padding: 5px;
    border-radius: 8px; display: flex; align-items: center;
    justify-content: center; cursor: pointer;
    transition: background 120ms, transform 100ms;
  }
  .action-btn:hover { background: rgba(139,92,246,0.08); }
  .action-btn:active { transform: scale(0.92); }
  .action-btn.danger-hover:hover { background: rgba(244,63,94,0.1); }
  /* Branch button — cyan accent to signal "new timeline" */
  .action-btn.branch-btn:hover {
    background: rgba(0,242,255,0.08);
    box-shadow: 0 0 0 1px rgba(0,242,255,0.12);
  }
  .action-btn.branch-btn:hover :global(svg) {
    color: #00f2ff;
    filter: drop-shadow(0 0 4px rgba(0,242,255,0.5));
  }
  .action-btn.spin :global(.icon) { animation: spin 600ms ease-in-out; }
  @keyframes spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }

  /* ── Edit Mode ── */
  .edit-area {
    width: 100%; min-height: 60px; padding: 12px 14px;
    border-radius: 10px; border: 1px solid rgba(139,92,246,0.35);
    background: rgba(14,14,30,0.8); color: #e0e0f0;
    font-size: 14px; font-family: var(--font-body);
    line-height: 1.6; resize: vertical; outline: none;
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06);
  }
  .user-edit { text-align: right; }

  .edit-actions { display: flex; gap: 6px; margin-top: 6px; }
  .edit-btn {
    display: flex; align-items: center; gap: 4px;
    padding: 6px 14px; border-radius: 8px;
    font-size: 11px; font-family: var(--font-body);
    font-weight: 600; border: none; cursor: pointer;
    transition: all 150ms;
  }
  .edit-btn.save {
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    color: #fff; box-shadow: 0 2px 10px rgba(139,92,246,0.25);
  }
  .edit-btn.save:hover { box-shadow: 0 4px 16px rgba(139,92,246,0.4); transform: translateY(-1px); }
  .edit-btn.cancel {
    background: transparent; border: 1px solid rgba(139,92,246,0.12);
    color: #6b6b8a;
  }
  .edit-btn.cancel:hover { background: rgba(139,92,246,0.06); }

  @media (max-width: 768px) {
    .msg-body, .msg-body-user { max-width: 85vw; }
    .user-bubble { max-width: 80vw; }
  }
</style>