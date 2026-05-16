<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Message } from '$lib/types';
  import { formatRoleplayContent } from '$lib/utils/format';
  import { browser } from '$app/environment';
  import { messages, activeConversationId, isStreaming, switchBranch, regenerateMessage as storeRegenerate } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import { get } from 'svelte/store';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { message }: { message: Message } = $props();

  let showActions = $state(false);
  let isRegenerating = $state(false);
  let isEditing = $state(false);
  let editContent = $state('');
  let copied = $state(false);

  // Branch navigation
  let hasBranches = $derived((message.siblingIds?.length ?? 0) > 1);
  let branchIndex = $derived(message.siblingIndex ?? 0);
  let branchTotal = $derived(message.siblingIds?.length ?? 1);
  let canPrev = $derived(hasBranches && branchIndex > 0);
  let canNext = $derived(hasBranches && branchIndex < branchTotal - 1);

  function navigateBranch(direction: -1 | 1) {
    if (!message.siblingIds) return;
    const newIndex = branchIndex + direction;
    if (newIndex < 0 || newIndex >= branchTotal) return;
    switchBranch(message.siblingIds[newIndex]);
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
      <div class="msg-bubble ai-bubble">
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
          <div class="msg-text">
            {@html formatRoleplayContent(message.content)}
            {#if message.isStreaming}
              <span class="streaming-cursor cursor-blink" aria-label="Generating">▍</span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Action Bar -->
      <div class="msg-toolbar" class:visible={showActions || hasBranches} role="toolbar" aria-label="Message actions">
        <!-- Branch Navigator -->
        {#if hasBranches}
          <div class="branch-nav" aria-label="Branch navigation">
            <button 
              class="branch-arrow"
              disabled={!canPrev}
              onclick={() => navigateBranch(-1)}
              aria-label="Previous branch"
            >
              <Icon name="chevron-left" size={14} color={canPrev ? 'var(--fg-secondary)' : 'var(--fg-muted)'} />
            </button>
            <span class="branch-counter" title="Alternate {branchIndex + 1} of {branchTotal}">
              {branchIndex + 1}<span class="branch-sep">/</span>{branchTotal}
            </span>
            <button 
              class="branch-arrow"
              disabled={!canNext}
              onclick={() => navigateBranch(1)}
              aria-label="Next branch"
            >
              <Icon name="chevron-right" size={14} color={canNext ? 'var(--fg-secondary)' : 'var(--fg-muted)'} />
            </button>
          </div>
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
      <div class="msg-bubble user-bubble">
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
          <div class="msg-text">
            {@html formatRoleplayContent(message.content)}
          </div>
        {/if}
      </div>

      <!-- User Action Bar -->
      <div class="msg-toolbar user-toolbar" class:visible={showActions || hasBranches} role="toolbar" aria-label="Message actions">
        {#if hasBranches}
          <div class="branch-nav" aria-label="Branch navigation">
            <button 
              class="branch-arrow"
              disabled={!canPrev}
              onclick={() => navigateBranch(-1)}
              aria-label="Previous branch"
            >
              <Icon name="chevron-left" size={14} color={canPrev ? 'var(--fg-secondary)' : 'var(--fg-muted)'} />
            </button>
            <span class="branch-counter" title="Edit {branchIndex + 1} of {branchTotal}">
              {branchIndex + 1}<span class="branch-sep">/</span>{branchTotal}
            </span>
            <button 
              class="branch-arrow"
              disabled={!canNext}
              onclick={() => navigateBranch(1)}
              aria-label="Next branch"
            >
              <Icon name="chevron-right" size={14} color={canNext ? 'var(--fg-secondary)' : 'var(--fg-muted)'} />
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
          <button class="action-btn danger-hover" title="Delete" aria-label="Delete message" onclick={handleDelete}>
            <Icon name="trash-2" size={13} color="var(--fg-muted)" />
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .message { display: flex; gap: 12px; width: 100%; }
  .ai-message { align-items: flex-start; }
  .user-message { justify-content: flex-end; }

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

  .msg-body, .msg-body-user { display: flex; flex-direction: column; gap: 4px; max-width: 620px; }
  .msg-body-user { align-items: flex-end; }

  .msg-bubble { padding: 14px 18px; line-height: 1.65; }

  .ai-bubble {
    background: rgba(14,14,30,0.7);
    border: 1px solid rgba(139,92,246,0.08);
    border-radius: 4px 16px 16px 16px;
    backdrop-filter: blur(4px);
  }
  .user-bubble {
    background: linear-gradient(135deg, #7c3aed, #8B5CF6);
    border-radius: 16px 16px 4px 16px;
    max-width: 520px;
    box-shadow: 0 4px 20px rgba(139,92,246,0.2);
  }

  .msg-text { font-size: var(--text-base); color: #e0e0f0; word-wrap: break-word; }
  .msg-text :global(.rp-action) { color: #8b8ba7; font-style: italic; }
  .streaming-cursor { color: #c4a1ff; font-weight: 700; }

  .msg-toolbar {
    display: flex; align-items: center; gap: 2px; height: 28px;
    opacity: 0; transform: translateY(2px);
    transition: opacity 180ms ease, transform 180ms ease;
  }
  .msg-toolbar.visible { opacity: 1; transform: translateY(0); }
  .user-toolbar { justify-content: flex-end; }

  .toolbar-divider { width: 1px; height: 14px; background: rgba(139,92,246,0.12); margin: 0 4px; flex-shrink: 0; }

  .branch-nav {
    display: flex; align-items: center; gap: 2px;
    background: rgba(14,14,30,0.6); border-radius: 99px;
    padding: 2px 4px; border: 1px solid rgba(139,92,246,0.1);
  }
  .branch-arrow {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; border: none; background: transparent;
    border-radius: 50%; padding: 0; cursor: pointer;
    transition: background 150ms, transform 100ms;
  }
  .branch-arrow:not(:disabled):hover { background: rgba(139,92,246,0.1); }
  .branch-arrow:not(:disabled):active { transform: scale(0.9); }
  .branch-arrow:disabled { opacity: 0.3; cursor: default; }

  .branch-counter {
    font-size: var(--text-sm); font-weight: 600; color: #8b8ba7;
    font-family: var(--font-mono); min-width: 28px;
    text-align: center; user-select: none;
  }
  .branch-sep { color: #4a4a6a; margin: 0 0.5px; font-weight: 400; }

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
  .action-btn.spin :global(.icon) { animation: spin 600ms ease-in-out; }
  @keyframes spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }

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
