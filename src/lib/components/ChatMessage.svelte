<script lang="ts">
  import Icon from './Icon.svelte';
  import EmotionHUD from './EmotionHUD.svelte';
  import type { Message } from '$lib/types';
  import { formatRoleplayContent } from '$lib/utils/format';
  import { browser } from '$app/environment';
  import { messages, activeConversationId, activeCharacterId, isStreaming, switchBranch, switchToConversation, regenerateMessage as storeRegenerate, characterEmotionState, retryLastMessage } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import { get } from 'svelte/store';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { message, onBranch, avatarUrl = null, characterName = '' }: { 
    message: Message; 
    onBranch?: (id: string) => void;
    avatarUrl?: string | null;
    characterName?: string;
  } = $props();

  let showActions = $state(false);
  let isRegenerating = $state(false);
  let isEditing = $state(false);
  let editContent = $state('');
  let copied = $state(false);
  let isSwitching = $state(false);

  // ── Typewriter Reveal ──
  // LLM tokens arrive as whole words — we reveal them character-by-character
  // for a smooth typing feel. Uses an adaptive easing algorithm:
  // speeds up when falling behind, slows down when caught up.
  let streamTextEl: HTMLDivElement | undefined = $state();
  let revealedLen = 0;        // how many chars currently visible
  let typewriterRafId: number | null = null;
  let wasStreaming = false;

  // Minimum chars to reveal per frame — tuned for 60fps feel
  const MIN_CHARS_PER_FRAME = 1;
  // Easing factor: fraction of remaining gap to close each frame
  // 0.35 = smooth but responsive; higher = faster catch-up
  const EASE_FACTOR = 0.35;

  function typewriterTick() {
    typewriterRafId = null;
    if (!streamTextEl) return;

    const text = message.content;
    const targetLen = text.length;
    if (revealedLen >= targetLen) {
      // Caught up — wait for more content
      if (message.isStreaming) {
        typewriterRafId = requestAnimationFrame(typewriterTick);
      }
      return;
    }

    // Adaptive speed: close a fraction of the gap, with a minimum speed
    const gap = targetLen - revealedLen;
    const step = Math.max(MIN_CHARS_PER_FRAME, Math.ceil(gap * EASE_FACTOR));
    let newLen = Math.min(revealedLen + step, targetLen);

    // ── Safe slice: never cut inside an *asterisk* formatting pair ──
    // Find all *...* pairs and ensure we don't slice inside one.
    // This handles both *italic* and **bold** patterns.
    const pairRegex = /\*{1,2}[^*]+\*{1,2}/g;
    let match: RegExpExecArray | null;
    while ((match = pairRegex.exec(text)) !== null) {
      const pairStart = match.index;
      const pairEnd = pairStart + match[0].length;
      if (newLen > pairStart && newLen < pairEnd) {
        // We're slicing inside this pair
        if (pairEnd - newLen < 50) {
          // Close enough — extend to include the full pair
          newLen = pairEnd;
        } else {
          // Too far — retreat to before this pair starts
          newLen = pairStart;
        }
        break; // Only need to fix the first conflict
      }
    }

    revealedLen = Math.min(newLen, targetLen);

    // Render only the revealed portion
    const visibleText = text.slice(0, revealedLen);
    streamTextEl.innerHTML = formatRoleplayContent(visibleText);

    // Keep ticking if there's more to reveal or stream is still active
    if (revealedLen < targetLen || message.isStreaming) {
      typewriterRafId = requestAnimationFrame(typewriterTick);
    }
  }

  // React to streaming state changes
  $effect(() => {
    const isNowStreaming = !!message.isStreaming;
    const contentLen = message.content.length;

    if (isNowStreaming && streamTextEl) {
      // Start or continue the typewriter loop
      if (!wasStreaming) {
        // Fresh stream start — reset reveal position
        revealedLen = 0;
        wasStreaming = true;
      }
      // Ensure the rAF loop is running
      if (typewriterRafId === null) {
        typewriterRafId = requestAnimationFrame(typewriterTick);
      }
    } else if (wasStreaming && !isNowStreaming) {
      // Stream just ended — cancel any pending frame and show full content
      wasStreaming = false;
      if (typewriterRafId !== null) {
        cancelAnimationFrame(typewriterRafId);
        typewriterRafId = null;
      }
      revealedLen = contentLen;
      // The {:else} branch of the template will now render via {@html}
    }
  });

  // Reactively derived from the store — updates live after each stream completes
  let emotionState = $derived($characterEmotionState);

  // Branch navigation — supports both in-conversation siblings and cross-conversation branches
  let hasConvBranches = $derived((message.siblingConversationIds?.length ?? 0) > 1);
  let hasBranches = $derived(hasConvBranches || (message.siblingIds?.length ?? 0) > 1);
  let branchIndex = $derived(
    hasConvBranches ? (message.siblingConversationIndex ?? 0) : (message.siblingIndex ?? 0)
  );
  let branchTotal = $derived(
    hasConvBranches ? (message.siblingConversationIds?.length ?? 1) : (message.siblingIds?.length ?? 1)
  );
  let canPrev = $derived(hasBranches && branchIndex > 0);
  let canNext = $derived(hasBranches && branchIndex < branchTotal - 1);

  async function navigateBranch(direction: -1 | 1) {
    if (isSwitching) return;
    const newIndex = branchIndex + direction;
    if (newIndex < 0 || newIndex >= branchTotal) return;
    isSwitching = true;

    if (hasConvBranches && message.siblingConversationIds) {
      // Cross-conversation navigation — switch entire conversation
      await switchToConversation(message.siblingConversationIds[newIndex]);
    } else if (message.siblingIds) {
      // In-conversation navigation — switch active message within same conversation
      await switchBranch(message.siblingIds[newIndex]);
    }

    setTimeout(() => isSwitching = false, 400);
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
      {#if avatarUrl}
        <img src={avatarUrl} alt={characterName} class="ai-avatar-img" />
      {/if}
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
            {#if message.isError}
              <div class="error-state">
                <div class="error-indicator">
                  <Icon name="alert-circle" size={14} color="#F43F5E" />
                  <span class="error-label">Generation failed</span>
                </div>
                <button class="retry-btn" onclick={retryLastMessage}>
                  <Icon name="refresh-cw" size={12} color="#FFFFFF" />
                  <span>Retry</span>
                </button>
              </div>
            {:else if message.isStreaming}
              <!-- During streaming: use direct DOM updates via $effect (no Svelte diffing) -->
              <div bind:this={streamTextEl} class="stream-live"></div>
              <span class="streaming-cursor cursor-blink" aria-label="Generating">▍</span>
            {:else}
              <!-- After streaming: use normal Svelte {@html} for proper formatting -->
              {@html formatRoleplayContent(message.content)}
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
                    if (isSwitching) return;
                    isSwitching = true;
                    if (hasConvBranches && message.siblingConversationIds) {
                      switchToConversation(message.siblingConversationIds[i]).finally(() => setTimeout(() => isSwitching = false, 400));
                    } else if (message.siblingIds) {
                      switchBranch(message.siblingIds[i]).finally(() => setTimeout(() => isSwitching = false, 600));
                    } else {
                      isSwitching = false;
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
            {#if message.isError}
              <div class="error-state user-error">
                <span class="error-content">{message.content}</span>
                <div class="error-indicator">
                  <Icon name="alert-circle" size={14} color="#F43F5E" />
                  <span class="error-label">Failed to send</span>
                </div>
                <button class="retry-btn" onclick={retryLastMessage}>
                  <Icon name="refresh-cw" size={12} color="#FFFFFF" />
                  <span>Retry</span>
                </button>
              </div>
            {:else}
              {@html formatRoleplayContent(message.content)}
            {/if}
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
                    if (isSwitching) return;
                    isSwitching = true;
                    if (hasConvBranches && message.siblingConversationIds) {
                      switchToConversation(message.siblingConversationIds[i]).finally(() => setTimeout(() => isSwitching = false, 400));
                    } else if (message.siblingIds) {
                      switchBranch(message.siblingIds[i]).finally(() => setTimeout(() => isSwitching = false, 600));
                    } else {
                      isSwitching = false;
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
  /* ───────────────────────────────────────────────
     LAYOUT
  ─────────────────────────────────────────────── */
  .message {
    display: flex;
    gap: 14px;
    width: 100%;
    animation: msgIn 320ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes msgIn {
    from { opacity: 0; transform: translateY(10px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .ai-message  { align-items: flex-start; }
  .user-message { justify-content: flex-end; }

  /* ───────────────────────────────────────────────
     ERROR STATE
  ─────────────────────────────────────────────── */
  .error-state {
    display: flex; flex-direction: column; gap: 10px;
    padding: 4px 0;
  }
  .error-indicator {
    display: flex; align-items: center; gap: 6px;
  }
  .error-label {
    font-size: 13px; font-weight: 600; color: #F43F5E;
    letter-spacing: 0.2px;
  }
  .error-content {
    font-size: 13px; color: #c8c8e0; line-height: 1.5;
    opacity: 0.7;
  }
  .retry-btn {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 14px; border-radius: 8px;
    background: linear-gradient(135deg, rgba(139,92,246,0.2), rgba(191,64,255,0.15));
    border: 1px solid rgba(139,92,246,0.25);
    color: #e0e0f0; font-size: 12px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 200ms ease; width: fit-content;
  }
  .retry-btn:hover {
    background: linear-gradient(135deg, rgba(139,92,246,0.35), rgba(191,64,255,0.25));
    border-color: rgba(139,92,246,0.4);
    box-shadow: 0 2px 12px rgba(139,92,246,0.2);
    transform: translateY(-1px);
  }
  .retry-btn:active { transform: translateY(0); }
  .user-error { align-items: flex-end; }

  /* ───────────────────────────────────────────────
     AVATAR
  ─────────────────────────────────────────────── */
  .msg-avatar {
    width: 36px; height: 36px; border-radius: 50%;
    flex-shrink: 0; position: relative; margin-top: 2px;
  }
  .ai-avatar {
    background: conic-gradient(from 200deg, #7c3aed, #bf40ff, #00f2ff, #7c3aed);
    box-shadow: 0 0 0 1px rgba(139,92,246,0.2), 0 0 18px rgba(139,92,246,0.15);
    overflow: hidden;
  }
  .ai-avatar-img {
    width: 100%; height: 100%;
    object-fit: cover; display: block;
    border-radius: 50%;
  }
  .avatar-glow {
    position: absolute; inset: -4px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.18) 0%, transparent 68%);
    animation: avatarPulse 3.5s ease-in-out infinite;
    pointer-events: none;
  }
  @keyframes avatarPulse {
    0%, 100% { opacity: 0.6; transform: scale(1); }
    50%       { opacity: 1;   transform: scale(1.08); }
  }

  /* ───────────────────────────────────────────────
     BODY CONTAINERS
  ─────────────────────────────────────────────── */
  .msg-body     { display: flex; flex-direction: column; gap: 6px; max-width: 640px; }
  .msg-body-user { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; max-width: 560px; }

  /* ───────────────────────────────────────────────
     AI BUBBLE — Liquid Obsidian Glass
  ─────────────────────────────────────────────── */
  .msg-bubble { position: relative; overflow: hidden; }

  .ai-bubble {
    padding: 18px 22px;
    background: rgba(11, 11, 28, 0.72);
    border-radius: 3px 20px 20px 20px;
    border: 1px solid rgba(255, 255, 255, 0.04);
    box-shadow:
      0 1px 0 rgba(255,255,255,0.04) inset,
      0 -1px 0 rgba(0,0,0,0.2) inset,
      0 8px 32px rgba(0,0,0,0.28),
      0 2px 8px rgba(0,0,0,0.2);
    backdrop-filter: blur(16px) saturate(160%);
    line-height: 1.72;
    transition: box-shadow 400ms ease, border-color 400ms ease;
  }
  /* Left luminous accent line */
  .ai-bubble::before {
    content: '';
    position: absolute; left: 0; top: 16px; bottom: 16px; width: 2px;
    background: linear-gradient(180deg, transparent, rgba(139,92,246,0.5), transparent);
    border-radius: 0 1px 1px 0;
    transition: opacity 400ms ease;
    opacity: 0.6;
  }
  .ai-bubble:hover::before { opacity: 1; }
  /* Top sheen */
  .ai-bubble::after {
    content: '';
    position: absolute; inset: 0;
    background: linear-gradient(160deg, rgba(255,255,255,0.025) 0%, transparent 40%);
    pointer-events: none; border-radius: inherit;
  }

  .ai-bubble.switching {
    border-color: rgba(139,92,246,0.18);
    box-shadow: 0 0 0 1px rgba(139,92,246,0.1),
                0 0 32px rgba(139,92,246,0.08),
                0 8px 32px rgba(0,0,0,0.28);
  }

  /* ───────────────────────────────────────────────
     USER BUBBLE — Plasma Violet
  ─────────────────────────────────────────────── */
  .user-bubble {
    padding: 14px 20px;
    background: linear-gradient(145deg, #6d28d9 0%, #7c3aed 40%, #8b5cf6 100%);
    border-radius: 20px 20px 4px 20px;
    max-width: 480px;
    box-shadow:
      0 0 0 1px rgba(139,92,246,0.35),
      0 4px 24px rgba(109,40,217,0.3),
      0 12px 40px rgba(109,40,217,0.15),
      inset 0 1px 0 rgba(255,255,255,0.14);
    line-height: 1.65;
    position: relative; overflow: hidden;
    transition: box-shadow 300ms ease;
  }
  /* Specular highlight layer */
  .user-bubble::before {
    content: '';
    position: absolute; inset: 0;
    background: radial-gradient(ellipse 70% 40% at 50% 0%, rgba(255,255,255,0.12), transparent 60%);
    pointer-events: none; border-radius: inherit;
  }
  .user-bubble:hover {
    box-shadow:
      0 0 0 1px rgba(139,92,246,0.5),
      0 6px 32px rgba(109,40,217,0.4),
      0 16px 48px rgba(109,40,217,0.2),
      inset 0 1px 0 rgba(255,255,255,0.18);
  }
  .user-bubble.switching {
    box-shadow:
      0 0 0 1px rgba(0,242,255,0.25),
      0 0 40px rgba(0,242,255,0.1),
      0 8px 32px rgba(109,40,217,0.3);
  }

  /* ───────────────────────────────────────────────
     TIMELINE SHIFT OVERLAY
  ─────────────────────────────────────────────── */
  .timeline-shift-overlay {
    position: absolute; inset: 0; z-index: 10;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 10px;
    background: rgba(6, 6, 18, 0.82);
    backdrop-filter: blur(10px);
    border-radius: inherit;
    animation: shiftFadeIn 200ms ease both;
  }
  @keyframes shiftFadeIn { from { opacity: 0; } to { opacity: 1; } }

  .shift-text {
    font-size: 10px; font-weight: 600; color: rgba(196,161,255,0.7);
    letter-spacing: 1.5px; text-transform: uppercase; font-family: var(--font-mono);
    animation: shiftPulse 900ms ease-in-out infinite alternate;
  }
  @keyframes shiftPulse {
    from { opacity: 0.4; }
    to   { opacity: 0.9; color: #c4a1ff; }
  }
  .shift-particles { display: flex; gap: 6px; align-items: center; }
  .shift-particle {
    width: 3px; height: 3px; border-radius: 50%;
    background: #8B5CF6;
    animation: particleFloat 700ms ease-in-out calc(var(--i) * 90ms) infinite alternate;
  }
  @keyframes particleFloat {
    from { transform: translateY(0) scale(0.5); opacity: 0.2; background: #8B5CF6; }
    to   { transform: translateY(-7px) scale(1.1); opacity: 1; background: #00f2ff; }
  }

  /* ───────────────────────────────────────────────
     MESSAGE TEXT
  ─────────────────────────────────────────────── */
  .msg-text {
    font-size: 14.5px;
    color: rgba(224, 220, 248, 0.92);
    word-wrap: break-word;
    letter-spacing: 0.01em;
    transition: opacity 300ms ease;
  }
  .msg-text.dim { opacity: 0.2; filter: blur(1px); }

  /* Roleplay action text — italic, muted, spaced */
  .msg-text :global(.rp-action) {
    color: rgba(139, 139, 175, 0.72);
    font-style: italic;
    display: block;
    margin: 6px 0;
    font-size: 13.5px;
    letter-spacing: 0.02em;
  }
  .user-text {
    color: rgba(255, 255, 255, 0.96);
    font-weight: 420;
    letter-spacing: 0.015em;
  }
  .streaming-cursor {
    color: #c4a1ff;
    font-weight: 700;
    animation: cursorBlink 900ms step-end infinite;
    /* Prevent cursor from causing layout shifts */
    display: inline-block;
    width: 0.5em;
    vertical-align: baseline;
  }
  @keyframes cursorBlink { 0%,100% { opacity: 1; } 50% { opacity: 0; } }

  /* Streaming live container — inherits all text styles, avoids layout shifts */
  .stream-live {
    display: inline;
    /* Inherit roleplay formatting from parent */
  }
  .stream-live :global(.rp-action) {
    color: rgba(139, 139, 175, 0.72);
    font-style: italic;
    /* INLINE during streaming — prevents line breaks mid-paragraph.
       The final render (after streaming) uses the normal display:block from .msg-text. */
    display: inline;
    margin: 0;
    font-size: 13.5px;
    letter-spacing: 0.02em;
  }

  /* ───────────────────────────────────────────────
     TOOLBAR
  ─────────────────────────────────────────────── */
  .msg-toolbar {
    display: flex; align-items: center; gap: 3px;
    height: 30px;
    opacity: 0; transform: translateY(4px);
    transition: opacity 200ms ease, transform 200ms cubic-bezier(0.16, 1, 0.3, 1);
    pointer-events: none;
  }
  .msg-toolbar.visible {
    opacity: 1; transform: translateY(0);
    pointer-events: auto;
  }
  .user-toolbar { justify-content: flex-end; }
  .toolbar-divider {
    width: 1px; height: 12px;
    background: rgba(139,92,246,0.1);
    margin: 0 3px; flex-shrink: 0;
  }

  /* ───────────────────────────────────────────────
     THREAD NAVIGATOR (Redesigned)
     "Liquid Thread" — the focal UI element
  ─────────────────────────────────────────────── */
  .timeline-nav {
    display: flex; align-items: center; gap: 5px;
    padding: 4px 6px;
    background: rgba(6, 6, 18, 0.88);
    border: 1px solid rgba(139,92,246,0.2);
    border-radius: 999px;
    backdrop-filter: blur(20px);
    box-shadow:
      0 0 0 1px rgba(139,92,246,0.06),
      0 2px 16px rgba(0,0,0,0.4),
      inset 0 1px 0 rgba(255,255,255,0.04);
    animation: threadIn 280ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes threadIn {
    from { opacity: 0; transform: translateY(6px) scale(0.92); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* Arrow buttons */
  .tl-arrow {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    background: transparent; border: none; padding: 0;
    border-radius: 50%; cursor: pointer;
    color: rgba(139,92,246,0.4);
    transition: color 150ms ease, background 150ms ease, transform 100ms ease;
    flex-shrink: 0;
  }
  .tl-arrow:not(:disabled):hover {
    color: #c4a1ff;
    background: rgba(139,92,246,0.12);
    transform: scale(1.15);
  }
  .tl-arrow:not(:disabled):active { transform: scale(0.85); }
  .tl-arrow:disabled { opacity: 0.2; cursor: default; }

  /* Dot Track */
  .tl-track { display: flex; align-items: center; gap: 4px; padding: 0 1px; }

  .tl-dot {
    width: 5px; height: 5px; border-radius: 50%;
    border: none; padding: 0; cursor: pointer;
    background: rgba(139,92,246,0.2);
    transition:
      width 280ms cubic-bezier(0.34, 1.56, 0.64, 1),
      background 200ms ease,
      transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1),
      box-shadow 200ms ease;
    flex-shrink: 0;
  }
  .tl-dot:hover:not(:disabled) {
    background: rgba(139,92,246,0.5);
    transform: scale(1.4);
  }
  .tl-dot:disabled { cursor: default; }
  .tl-dot.visited  { background: rgba(139,92,246,0.3); }
  .tl-dot.active {
    width: 18px;
    border-radius: 3px;
    background: linear-gradient(90deg, #7c3aed, #00d4e0);
    box-shadow:
      0 0 6px rgba(124,58,237,0.7),
      0 0 14px rgba(0,212,224,0.2);
    animation: threadDotLive 2.5s ease-in-out infinite;
  }
  @keyframes threadDotLive {
    0%,100% { box-shadow: 0 0 6px rgba(124,58,237,0.6), 0 0 0px rgba(0,212,224,0.1); }
    50%      { box-shadow: 0 0 10px rgba(124,58,237,0.9), 0 0 20px rgba(0,212,224,0.3); }
  }

  /* Counter */
  .tl-counter {
    display: flex; align-items: baseline; gap: 0;
    font-family: var(--font-mono);
    font-size: 9.5px; font-weight: 700;
    padding: 0 3px; user-select: none;
    letter-spacing: 0.03em;
  }
  .tl-cur   { color: #b89eff; }
  .tl-slash { color: rgba(139,92,246,0.2); font-weight: 400; margin: 0 1px; }
  .tl-total { color: rgba(139,92,246,0.35); }

  /* ───────────────────────────────────────────────
     ACTION BUTTONS
  ─────────────────────────────────────────────── */
  .action-group { display: flex; gap: 1px; align-items: center; opacity: 0; transition: opacity 150ms ease; }
  .action-group.visible { opacity: 1; }

  .action-btn {
    background: none; border: none;
    padding: 6px; border-radius: 9px;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer;
    transition: background 130ms ease, transform 100ms ease, box-shadow 130ms ease;
  }
  .action-btn:hover {
    background: rgba(139,92,246,0.09);
    transform: translateY(-1px);
  }
  .action-btn:active { transform: scale(0.9); }
  .action-btn.danger-hover:hover { background: rgba(244,63,94,0.1); }
  .action-btn.branch-btn:hover {
    background: rgba(0,242,255,0.07);
    box-shadow: 0 0 0 1px rgba(0,242,255,0.1);
  }
  .action-btn.branch-btn:hover :global(svg) {
    color: #00f2ff;
    filter: drop-shadow(0 0 5px rgba(0,242,255,0.55));
  }
  .action-btn.spin :global(.icon) { animation: spin 500ms ease-in-out; }
  @keyframes spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }

  /* ───────────────────────────────────────────────
     EDIT MODE
  ─────────────────────────────────────────────── */
  .edit-area {
    width: 100%; min-height: 64px;
    padding: 13px 16px;
    border-radius: 12px;
    border: 1px solid rgba(139,92,246,0.3);
    background: rgba(10,10,26,0.8);
    color: rgba(224,220,248,0.94);
    font-size: 14px; font-family: var(--font-body);
    line-height: 1.65; resize: vertical; outline: none;
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06), inset 0 1px 0 rgba(255,255,255,0.03);
    transition: border-color 200ms, box-shadow 200ms;
  }
  .edit-area:focus {
    border-color: rgba(139,92,246,0.5);
    box-shadow: 0 0 0 4px rgba(139,92,246,0.1), inset 0 1px 0 rgba(255,255,255,0.03);
  }
  .user-edit { text-align: right; }

  .edit-actions { display: flex; gap: 6px; margin-top: 8px; }
  .edit-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 7px 16px; border-radius: 10px;
    font-size: 11.5px; font-family: var(--font-body);
    font-weight: 600; border: none; cursor: pointer;
    transition: all 150ms ease;
    letter-spacing: 0.02em;
  }
  .edit-btn.save {
    background: linear-gradient(135deg, #7c3aed, #8B5CF6);
    color: #fff;
    box-shadow: 0 2px 12px rgba(124,58,237,0.3);
  }
  .edit-btn.save:hover {
    box-shadow: 0 4px 20px rgba(124,58,237,0.45);
    transform: translateY(-1px);
  }
  .edit-btn.cancel {
    background: transparent;
    border: 1px solid rgba(139,92,246,0.14);
    color: rgba(107,107,138,0.85);
  }
  .edit-btn.cancel:hover { background: rgba(139,92,246,0.06); }

  /* ───────────────────────────────────────────────
     RESPONSIVE
  ─────────────────────────────────────────────── */
  @media (max-width: 768px) {
    .msg-body, .msg-body-user { max-width: 88vw; }
    .user-bubble { max-width: 82vw; }
  }
</style>