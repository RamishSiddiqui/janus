<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import JanusMark from './JanusMark.svelte';
  import EmotionHUD from './EmotionHUD.svelte';
  import ThinkingBlock from './ThinkingBlock.svelte';
  import type { Message } from '$lib/types';
  import { formatRoleplayContent } from '$lib/utils/format';
  import { browser } from '$app/environment';
  import { messages, activeConversationId, activeCharacterId, isStreaming, switchBranch, switchToConversation, regenerateMessage as storeRegenerate, characterEmotionStates, retryLastMessage } from '$lib/stores/chat';
  import { settings } from '$lib/stores/settings';
  import { success, error as toastError } from '$lib/stores/toast';
  import { get } from 'svelte/store';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let { message, onBranch, avatarUrl = null, characterName = '', model }: {
    message: Message;
    onBranch?: (id: string) => void;
    avatarUrl?: string | null;
    characterName?: string;
    /** Currently selected model in the chat input — threaded through to Regenerate so it doesn't silently fall back to the provider's stored default. */
    model?: string;
  } = $props();

  // ── Multi-Character Awareness ──
  const CHAR_ACCENT_COLORS = [
    '#8B5CF6', // violet (default/primary)
    '#00F2FF', // cyan
    '#F59E0B', // amber
    '#10B981', // emerald
    '#F43F5E', // rose
    '#3B82F6', // blue
    '#EC4899', // pink
    '#6366F1', // indigo
  ];

  function charColor(name: string): string {
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return CHAR_ACCENT_COLORS[Math.abs(hash) % CHAR_ACCENT_COLORS.length];
  }

  // Multi-character responses only get split into separate per-character
  // messages (with their own `character_name`/`character_avatar_url`) AFTER
  // the full response finishes streaming — see chat.rs's `Done` handler.
  // While a response is still streaming in, or in the rare case a raw
  // marker survives finalization unstripped, the bubble would otherwise
  // show the primary character's name/avatar with a literal "[Name]: "
  // prefix sitting in the text. Detecting a leading marker client-side and
  // treating it the same as a resolved `character_name` fixes both: the
  // header updates to the actual speaker as soon as their marker has fully
  // streamed in, and the marker itself never renders as raw text.
  function stripLeadingMarker(text: string): { name: string | null; rest: string } {
    const m = text.match(/^\s*\[([A-Z][\w' .-]{1,40})\]:\s*/);
    return m ? { name: m[1], rest: text.slice(m[0].length) } : { name: null, rest: text };
  }
  let liveMarker = $derived(
    message.character_name ? { name: null as string | null, rest: message.content } : stripLeadingMarker(message.content)
  );
  let isMultiChar = $derived(!!message.character_name || !!liveMarker.name);
  let displayName = $derived(message.character_name || liveMarker.name || characterName);
  // `avatarUrl` is the conversation's primary character's avatar — only a
  // valid fallback for a plain single-character message (no character_name
  // at all). A multi-char message with no avatar of its own (e.g. a
  // freshly-registered transient speaker with no portrait yet, or a name
  // detected live via `liveMarker` before the backend split has happened)
  // must NOT silently borrow the primary's face; falling through to no
  // image lets the plain colored-ring placeholder render instead (see
  // markup below).
  let displayAvatar = $derived(isMultiChar ? message.character_avatar_url : (message.character_avatar_url || avatarUrl));
  let accentColor = $derived(isMultiChar ? charColor(displayName) : '');

  let showActions = $state(false);
  let isRegenerating = $state(false);
  let isEditing = $state(false);
  let editContent = $state('');
  let copied = $state(false);
  let isSwitching = $state(false);

  // ── Attached images (user messages only) ──
  // Loaded as blob: URLs the same way avatars/scenes are (see blobUrl.ts) —
  // the app's CSP blocks asset:// but allows blob:. Re-derives whenever the
  // message's attachment list changes (e.g. the optimistic → real message
  // ID swap right after sending, or a fresh history load).
  let attachmentUrls: string[] = $state([]);
  $effect(() => {
    const attachments = message.attachments;
    let cancelled = false;
    if (!attachments || attachments.length === 0 || !isTauri) {
      attachmentUrls = [];
      return;
    }
    import('$lib/utils/blobUrl').then(async ({ loadFileAsBlobUrl }) => {
      const urls = await Promise.all(
        attachments.map(a => loadFileAsBlobUrl(a.relativePath, a.mimeType).catch(() => null))
      );
      if (!cancelled) attachmentUrls = urls.filter((u): u is string => u !== null);
    });
    return () => {
      cancelled = true;
      for (const url of attachmentUrls) URL.revokeObjectURL(url);
    };
  });

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

    const text = liveMarker.rest;
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

  // Reactively derived from the map store — picks the emotion state for THIS message's character.
  // Falls back to the primary character state for messages without a character_id (single-char mode).
  let emotionState = $derived(
    (() => {
      const map = $characterEmotionStates;
      const msgCharId = message.character_id;
      if (msgCharId) return map.get(msgCharId) ?? null;
      // Single-char mode: use primary character
      const primaryId = $activeCharacterId;
      return primaryId ? (map.get(primaryId) ?? null) : null;
    })()
  );

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
      await storeRegenerate(convId, message.id, model);
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

  // Auto-grows to fit content instead of exposing the browser's native
  // drag-to-resize handle — same technique as the main compose box
  // (ChatInput.svelte's autoResize), just with a taller cap since edited
  // messages are often full narrative paragraphs rather than a quick reply.
  let editAreaEl: HTMLTextAreaElement | undefined = $state();
  const EDIT_AREA_MAX_HEIGHT = 400;
  function autoResizeEditArea(e?: Event) {
    const el = (e?.target as HTMLTextAreaElement) ?? editAreaEl;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, EDIT_AREA_MAX_HEIGHT) + 'px';
  }

  async function startEdit() {
    editContent = message.content;
    isEditing = true;
    await tick();
    autoResizeEditArea();
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
    <div class="msg-avatar ai-avatar" aria-hidden="true" style={isMultiChar ? `--char-accent: ${accentColor}` : ''}>
      {#if displayAvatar}
        <img src={displayAvatar} alt={displayName} class="ai-avatar-img" />
      {:else}
        <div class="ai-avatar-fallback"><JanusMark size={18} /></div>
      {/if}
      <div class="avatar-glow"></div>
    </div>
    <div class="msg-body">
      {#if isMultiChar}
        <span class="char-name-badge" style="--char-accent: {accentColor}">{displayName}</span>
      {/if}
      <div class="msg-bubble ai-bubble" class:switching={isSwitching} class:multi-char={isMultiChar} style={isMultiChar ? `--char-accent: ${accentColor}` : ''}>
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
          <textarea class="edit-area" bind:this={editAreaEl} bind:value={editContent} oninput={autoResizeEditArea} rows="4" aria-label="Edit message content"></textarea>
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
          {#if $settings.showThinking && (message.reasoning || message.isThinking)}
            <ThinkingBlock
              reasoning={message.reasoning ?? ''}
              isThinking={message.isThinking}
              startedAt={message.thinkingStartedAt}
            />
          {/if}
          <div class="msg-text" class:dim={isSwitching}>
            {#if message.isError}
              <div class="error-state">
                <div class="error-indicator">
                  <Icon name="alert-circle" size={14} color="#F43F5E" />
                  <span class="error-label">Generation failed</span>
                </div>
              </div>
            {:else if message.isStreaming}
              <!-- During streaming: use direct DOM updates via $effect (no Svelte diffing) -->
              <div bind:this={streamTextEl} class="stream-live"></div>
              <span class="streaming-cursor cursor-blink" aria-label="Generating">▍</span>
            {:else}
              <!-- After streaming: use normal Svelte {@html} for proper formatting -->
              {@html formatRoleplayContent(liveMarker.rest)}
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

        <!-- Emotion HUD — shows this character's state from the per-character map -->
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
        {#if attachmentUrls.length > 0}
          <div class="msg-attachments">
            {#each attachmentUrls as url}
              <img src={url} alt="Attached" class="msg-attachment-thumb" />
            {/each}
          </div>
        {/if}
        {#if isEditing}
          <textarea class="edit-area user-edit" bind:this={editAreaEl} bind:value={editContent} oninput={autoResizeEditArea} rows="3" aria-label="Edit message content"></textarea>
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
  .user-error { align-items: flex-end; }

  /* ───────────────────────────────────────────────
     AVATAR
  ─────────────────────────────────────────────── */
  .msg-avatar {
    width: 36px; height: 36px; border-radius: 50%;
    flex-shrink: 0; position: relative; margin-top: 2px;
  }
  .ai-avatar {
    background: radial-gradient(circle at 35% 30%, #1e1a38, #0e0c1c);
    box-shadow: 0 0 0 1px rgba(139,92,246,0.2), 0 0 18px rgba(139,92,246,0.15);
    overflow: hidden;
  }
  .ai-avatar-fallback {
    width: 100%; height: 100%;
    display: flex; align-items: center; justify-content: center;
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

  /* Roleplay action text — italic, muted */
  .msg-text :global(.rp-action) {
    color: rgba(139, 139, 175, 0.72);
    font-style: italic;
    font-size: 13.5px;
    letter-spacing: 0.02em;
  }
  /* Block-level action paragraphs (set by formatter for full-line actions) */
  .msg-text :global(.rp-action-block) {
    display: block;
    margin: 6px 0;
  }
  .user-text {
    color: rgba(255, 255, 255, 0.96);
    font-weight: 420;
    letter-spacing: 0.015em;
  }
  .msg-attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
    position: relative;
  }
  .msg-attachment-thumb {
    width: 96px;
    height: 96px;
    object-fit: cover;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    display: block;
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
    width: 100%; min-height: 64px; max-height: 400px;
    padding: 13px 16px;
    border-radius: 12px;
    border: 1px solid rgba(139,92,246,0.3);
    background: rgba(10,10,26,0.8);
    color: rgba(224,220,248,0.94);
    font-size: 14px; font-family: var(--font-body);
    line-height: 1.65; resize: none; overflow-y: auto; outline: none;
    box-shadow: 0 0 0 4px rgba(139,92,246,0.06), inset 0 1px 0 rgba(255,255,255,0.03);
    /* No height transition — the JS-driven resize already runs per
       keystroke; animating height on top of that just adds lag between
       typing and the box actually growing (same reasoning as the existing
       compose box, which doesn't animate it either). */
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
    /* Sits on the user bubble's solid purple gradient (always purple,
       regardless of theme) — the previous muted gray-purple text read as
       near-invisible against it. */
    background: transparent;
    border: 1px solid rgba(255,255,255,0.25);
    color: rgba(255,255,255,0.8);
  }
  .edit-btn.cancel:hover { background: rgba(255,255,255,0.12); }

  /* ───────────────────────────────────────────────
     RESPONSIVE
  ─────────────────────────────────────────────── */
  @media (max-width: 768px) {
    .msg-body, .msg-body-user { max-width: 88vw; }
    .user-bubble { max-width: 82vw; }
  }

  /* ───────────────────────────────────────────────
     MULTI-CHARACTER — Name Badge & Accent Colors
  ─────────────────────────────────────────────── */
  .char-name-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 10px 2px 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--char-accent, #8B5CF6);
    background: color-mix(in srgb, var(--char-accent, #8B5CF6) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--char-accent, #8B5CF6) 15%, transparent);
    border-radius: 999px;
    margin-bottom: 4px;
    animation: badgeIn 280ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
    text-transform: uppercase;
    user-select: none;
    width: fit-content;
  }
  @keyframes badgeIn {
    from { opacity: 0; transform: translateY(-4px) scale(0.9); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* Multi-char bubble — character-colored left accent */
  .ai-bubble.multi-char::before {
    background: linear-gradient(
      180deg,
      transparent,
      var(--char-accent, rgba(139,92,246,0.5)),
      transparent
    );
  }

  /* Multi-char avatar — character-colored glow */
  .ai-avatar[style*="--char-accent"] .avatar-glow {
    background: radial-gradient(
      circle,
      color-mix(in srgb, var(--char-accent, #8B5CF6) 25%, transparent) 0%,
      transparent 68%
    );
  }

  /* Light theme overrides */
  :global([data-theme="light"]) .char-name-badge {
    background: color-mix(in srgb, var(--char-accent, #8B5CF6) 6%, white);
    border-color: color-mix(in srgb, var(--char-accent, #8B5CF6) 12%, transparent);
  }

  /* Reduced motion support */
  @media (prefers-reduced-motion: reduce) {
    .char-name-badge { animation: none; }
  }
</style>