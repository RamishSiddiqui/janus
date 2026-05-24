<script lang="ts">
  import { onMount, tick } from "svelte";
  import { browser } from "$app/environment";
  import ChatHeader from "$lib/components/ChatHeader.svelte";
  import SceneStateBar from "$lib/components/SceneStateBar.svelte";
  import type { SceneState } from "$lib/services/ipc";
  import Icon from "$lib/components/Icon.svelte";
  import ChatInput from "$lib/components/ChatInput.svelte";
  import ChatMessage from "$lib/components/ChatMessage.svelte";
  import ContextPanel from "$lib/components/ContextPanel.svelte";
  import { parseCharacterData } from "$lib/utils/character";
  import {
    messages,
    conversations,
    activeConversationId,
    activeConversation,
    activeCharacterId,
    isStreaming,
    lastStreamError,
    hasMoreMessages,
    isLoadingMoreMessages,
    loadConversations,
    loadMessages,
    loadMoreMessages,
    sendMessage,
    retryLastMessage,
    branchConversation,
  } from "$lib/stores/chat";

  const isTauri = browser && "__TAURI_INTERNALS__" in window;

  let inputText = $state("");
  let showContextPanel = $state(false);
  let messagesEl: HTMLDivElement | undefined = $state();
  let loadMoreSentinelEl: HTMLDivElement | undefined = $state();

  // Branching mode — set when user clicks "Branch from here" on a message
  let branchFromId: string | null = $state(null);
  let branchFromContent: string = $state(""); // preview text of the branch-point message

  // Scene state for the current conversation
  let currentSceneState: SceneState | null = $state(null);

  // ── Smart Auto-Scroll ──
  // Respects user intent: if they scroll up to read, don't yank them down.
  // When they scroll back near the bottom, re-engage sticky auto-scroll.
  // Shows a floating "↓" pill when scrolled away during streaming.
  const STICKY_THRESHOLD = 120; // px from bottom to re-engage sticky mode
  let isSticky = $state(true); // start at bottom
  let showScrollPill = $state(false); // floating "scroll to bottom" indicator
  let scrollRafId: number | null = null;

  /** Is the user currently near the bottom of the messages? */
  function isNearBottom(): boolean {
    if (!messagesEl) return true;
    const { scrollTop, scrollHeight, clientHeight } = messagesEl;
    return scrollHeight - scrollTop - clientHeight < STICKY_THRESHOLD;
  }

  function onMessagesScroll() {
    const nearBottom = isNearBottom();
    if (nearBottom) {
      isSticky = true;
      showScrollPill = false;
    } else {
      isSticky = false;
      // Show pill whenever scrolled away — not just during streaming
      showScrollPill = true;
    }
  }

  /** Scroll to bottom programmatically (from pill click or new message). */
  function scrollToBottom(behavior: ScrollBehavior = "smooth") {
    if (!messagesEl) return;
    isSticky = true;
    showScrollPill = false;
    messagesEl.scrollTo({ top: messagesEl.scrollHeight, behavior });
  }

  // Auto-scroll effect — only when sticky
  $effect(() => {
    const len = $messages.length;
    const streaming = $isStreaming;
    // Track content growth during streaming
    const _contentLen = streaming ? $messages[len - 1]?.content?.length : 0;

    // Show/hide pill based on streaming + sticky state
    if (isSticky) {
      showScrollPill = false;
    } else if (len > 0) {
      showScrollPill = true;
    }

    if (!isSticky) return; // User is reading history — don't scroll

    // Debounce via rAF
    if (scrollRafId !== null) cancelAnimationFrame(scrollRafId);
    scrollRafId = requestAnimationFrame(() => {
      scrollRafId = null;
      if (!messagesEl || !isSticky) return;
      messagesEl.scrollTo({
        top: messagesEl.scrollHeight,
        behavior: streaming ? "instant" : "smooth",
      });
    });
  });

  // ── Reverse Infinite Scroll (Load Older Messages) ──
  // An IntersectionObserver watches a sentinel element at the top of the messages area.
  // When it becomes visible (user scrolled near the top), we load the next batch of older
  // messages and preserve the scroll position so the viewport doesn't jump.
  let loadMoreObserver: IntersectionObserver | null = null;

  /** Load older messages and preserve scroll position. */
  async function handleLoadMore() {
    if (!messagesEl || $isLoadingMoreMessages || !$hasMoreMessages) return;

    // Capture scroll position BEFORE prepend
    const prevScrollHeight = messagesEl.scrollHeight;
    const prevScrollTop = messagesEl.scrollTop;

    const count = await loadMoreMessages();
    if (count === 0) return;

    // After DOM update, restore scroll position so viewport doesn't jump
    await tick();
    requestAnimationFrame(() => {
      if (!messagesEl) return;
      const newScrollHeight = messagesEl.scrollHeight;
      const delta = newScrollHeight - prevScrollHeight;
      messagesEl.scrollTop = prevScrollTop + delta;
    });
  }

  // Set up IntersectionObserver when sentinel element mounts
  $effect(() => {
    if (!loadMoreSentinelEl) return;

    // Disconnect previous observer if any
    loadMoreObserver?.disconnect();

    loadMoreObserver = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && $hasMoreMessages && !$isLoadingMoreMessages) {
          handleLoadMore();
        }
      },
      { root: messagesEl, rootMargin: '200px 0px 0px 0px', threshold: 0 }
    );
    loadMoreObserver.observe(loadMoreSentinelEl);

    return () => {
      loadMoreObserver?.disconnect();
      loadMoreObserver = null;
    };
  });

  // ── Initial Scroll: Start at Bottom ──
  // When a conversation opens, scroll to bottom INSTANTLY (no animation)
  // so the user never sees the scroll-from-top jank.
  let lastConversationId = '';
  $effect(() => {
    const convId = $activeConversationId;
    const len = $messages.length;
    if (!convId || convId === lastConversationId || len === 0) return;
    lastConversationId = convId;

    // Use tick() to wait for DOM render, then instant scroll
    tick().then(() => {
      if (!messagesEl) return;
      isSticky = true;
      showScrollPill = false;
      messagesEl.scrollTo({ top: messagesEl.scrollHeight, behavior: 'instant' });
    });
  });

  // Character data loaded from backend
  let characterDescription = $state("");
  let characterTags: { label: string; color: string }[] = $state([]);
  let avatarUrl: string | null = $state(null);

  // Token count approximation
  let tokenCount = $derived(
    $messages.reduce((acc, m) => acc + m.content.length / 4, 0).toFixed(0),
  );

  // Character info from active conversation (with fallbacks)
  let characterName = $derived(
    $activeConversation?.characterName ?? "Select a character",
  );
  let characterId = $derived($activeCharacterId);
  let additionalCharacters = $derived(
    $activeConversation?.additionalCharacters ?? [],
  );

  // Build a character_id → avatarUrl map for multi-char avatar resolution
  let characterAvatarMap = $derived.by(() => {
    const map = new Map<string, string | null>();
    // Primary character
    if (characterId && avatarUrl) {
      map.set(characterId, avatarUrl);
    }
    // Additional characters
    for (const ac of additionalCharacters) {
      if (ac.id && ac.avatarUrl) {
        map.set(ac.id, ac.avatarUrl);
      }
    }
    return map;
  });

  // Resolve per-message avatar: use character-specific avatar if available
  function resolveMessageAvatar(message: { character_id?: string | null; role?: string }): string | null {
    if (message.role !== 'assistant') return null;
    if (message.character_id && characterAvatarMap.has(message.character_id)) {
      return characterAvatarMap.get(message.character_id) ?? null;
    }
    return avatarUrl; // fallback to primary character avatar
  }
  let modelName = $state("No provider configured");
  let selectedModel = $state("");
  let availableModels: string[] = $state([]);
  let activeProviderId = $state("");

  // Branch lineage — if the active conversation was forked from another
  let parentConversationId = $derived(
    $activeConversation?.parentConversationId ?? null,
  );
  let parentConversationTitle = $derived(
    parentConversationId
      ? ($conversations.find((c) => c.id === parentConversationId)
          ?.characterName ?? "Previous conversation")
      : null,
  );

  function navigateToParent(parentId: string) {
    if (!parentId) return;
    isStreaming.set(false); // discard any active stream for the current conversation
    activeConversationId.set(parentId);
    loadMessages(parentId);
  }

  // Load conversations + active model on mount
  onMount(async () => {
    loadConversations();
    if (!isTauri) {
      modelName = "Llama 4 Maverick via OpenRouter";
      return;
    }
    try {
      const ipc = await import("$lib/services/ipc");
      const providers = await ipc.listProviders("llm");
      const active = providers.find((p) => p.is_default) ?? providers[0];
      if (active) {
        activeProviderId = active.id;
        const config = active.config as Record<string, string>;
        let defaultModel = config.model || "";
        if (!defaultModel || defaultModel === "unknown") {
          // Fall back to first enabled model for this provider
          try {
            const enabled = await ipc.listEnabledModels(active.id);
            defaultModel = enabled.filter(e => e.model_type !== 'embedding')[0]?.model_id ?? "";
          } catch {
            /* ignore */
          }
        }
        modelName = defaultModel
          ? `${defaultModel} via ${active.name}`
          : `No model set — AI Studio → Models`;
        selectedModel = defaultModel;
      }
    } catch {
      /* fallback already set */
    }

    // Listen for scene_state_changed events from backend
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('scene_state_changed', () => {
        const convId = $activeConversationId;
        if (convId) loadSceneState(convId);
      });
    });
  });

  // Load scene state whenever the active conversation changes
  $effect(() => {
    const convId = $activeConversationId;
    if (convId && isTauri) {
      loadSceneState(convId);
    } else {
      currentSceneState = null;
    }
  });

  async function loadSceneState(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      currentSceneState = await ipc.getSceneState(convId);
    } catch (err) {
      console.error('Failed to load scene state:', err);
      currentSceneState = null;
    }
  }

  async function refreshModels() {
    if (!isTauri || !activeProviderId) return;
    try {
      const ipc = await import("$lib/services/ipc");
      const enabled = await ipc.listEnabledModels(activeProviderId);
      availableModels = enabled.filter(e => e.model_type !== 'embedding').map((e) => e.model_id);
    } catch {
      availableModels = [];
    }
  }

  // Reactively load messages when active conversation changes
  $effect(() => {
    const id = $activeConversationId;
    if (id) {
      loadMessages(id);
    }
  });

  // Load character data when active character changes
  $effect(() => {
    const charId = $activeCharacterId;
    if (charId && isTauri) {
      loadCharacterData(charId);
    } else {
      // Reset to defaults
      characterDescription =
        "Half-elf with untamed elemental magic at the College of Magic";
      characterTags = [];
      avatarUrl = null;
    }
  });

  async function loadCharacterData(charId: string) {
    try {
      const ipc = await import("$lib/services/ipc");
      const char = await ipc.getCharacter(charId);
      const data = parseCharacterData(char.data);

      characterDescription = data.description || "No description available";

      // Extract tags from character data
      const tagColors = ["#8B5CF6", "#BF40FF", "#00F2FF", "#F59E0B", "#10B981"];
      if (data.tags?.length) {
        characterTags = data.tags.map((t: string, i: number) => ({
          label: t,
          color: tagColors[i % tagColors.length],
        }));
      } else {
        characterTags = [];
      }

      // Resolve avatar URL
      if (char.avatar_path) {
        try {
          const { readFile, BaseDirectory } = await import(
            "@tauri-apps/plugin-fs"
          );
          const bytes = await readFile(char.avatar_path, {
            baseDir: BaseDirectory.AppData,
          });
          const ext =
            char.avatar_path.split(".").pop()?.toLowerCase() || "jpeg";
          const mime =
            ext === "png"
              ? "image/png"
              : ext === "webp"
                ? "image/webp"
                : "image/jpeg";
          const blob = new Blob([bytes], { type: mime });
          // Revoke old blob URL to prevent memory leak
          if (avatarUrl) URL.revokeObjectURL(avatarUrl);
          avatarUrl = URL.createObjectURL(blob);
        } catch {
          avatarUrl = null;
        }
      } else {
        if (avatarUrl) URL.revokeObjectURL(avatarUrl);
        avatarUrl = null;
      }
    } catch (err) {
      console.error("Failed to load character data:", err);
      characterDescription = "Character data unavailable";
      characterTags = [];
      avatarUrl = null;
    }
  }

  async function handleSend() {
    if (!inputText.trim() || $isStreaming) return;
    const text = inputText.trim();
    inputText = "";

    if (branchFromId) {
      // Branching mode: rewind active pointer then send
      const branchId = branchFromId;
      branchFromId = null;
      branchFromContent = "";
      await branchConversation(
        $activeConversationId,
        branchId,
        text,
        selectedModel || undefined,
      );
    } else {
      await sendMessage(
        $activeConversationId,
        text,
        selectedModel || undefined,
      );
    }
  }

  function handleBranch(messageId: string) {
    // Find the message content for the preview label
    const msg = $messages.find((m) => m.id === messageId);
    branchFromContent = msg?.content ?? "";
    branchFromId = messageId;
    // Scroll input into view and focus it
    tick().then(() => {
      (
        document.querySelector(
          ".chat-input-field",
        ) as HTMLTextAreaElement | null
      )?.focus();
    });
  }

  function cancelBranch() {
    branchFromId = null;
    branchFromContent = "";
  }

  // Derived: index of the branch point in the current messages list
  let branchIndex = $derived(
    branchFromId ? $messages.findIndex((m) => m.id === branchFromId) : -1,
  );
</script>

<svelte:head>
  <title>Chat — Mythic</title>
</svelte:head>

<div class="chat-view">
  {#if !$activeConversationId}
    <!-- Landing Screen — no conversation selected -->
    <div class="landing-screen">
      <!-- Ambient floating orbs -->
      <div class="landing-orbs">
        <div class="orb orb-1"></div>
        <div class="orb orb-2"></div>
        <div class="orb orb-3"></div>
      </div>

      <!-- Central content -->
      <div class="landing-content animate-fade-in-scale">
        <!-- Animated quill/scroll icon -->
        <div class="landing-icon-group">
          <div class="landing-glow"></div>
          <svg
            class="landing-icon"
            viewBox="0 0 64 64"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <!-- Scroll -->
            <rect
              x="14"
              y="12"
              width="36"
              height="44"
              rx="4"
              stroke="url(#scrollGrad)"
              stroke-width="2"
              fill="none"
              opacity="0.6"
            />
            <line
              x1="22"
              y1="24"
              x2="42"
              y2="24"
              stroke="var(--fg-muted)"
              stroke-width="1.5"
              stroke-linecap="round"
              opacity="0.4"
            />
            <line
              x1="22"
              y1="30"
              x2="38"
              y2="30"
              stroke="var(--fg-muted)"
              stroke-width="1.5"
              stroke-linecap="round"
              opacity="0.3"
            />
            <line
              x1="22"
              y1="36"
              x2="34"
              y2="36"
              stroke="var(--fg-muted)"
              stroke-width="1.5"
              stroke-linecap="round"
              opacity="0.2"
            />
            <!-- Quill pen -->
            <path
              d="M44 8 C44 8 50 14 48 22 C46 30 40 32 40 32 L42 28 C42 28 46 24 46 16 C46 12 44 8 44 8Z"
              fill="url(#quillGrad)"
              opacity="0.9"
            />
            <line
              x1="40"
              y1="32"
              x2="36"
              y2="44"
              stroke="url(#quillGrad)"
              stroke-width="1.5"
              stroke-linecap="round"
            />
            <defs>
              <linearGradient id="scrollGrad" x1="14" y1="12" x2="50" y2="56">
                <stop offset="0%" stop-color="#8B5CF6" />
                <stop offset="100%" stop-color="#BF40FF" />
              </linearGradient>
              <linearGradient id="quillGrad" x1="36" y1="8" x2="48" y2="44">
                <stop offset="0%" stop-color="#00F2FF" />
                <stop offset="100%" stop-color="#8B5CF6" />
              </linearGradient>
            </defs>
          </svg>
        </div>

        <h2 class="landing-title">Begin Your Story</h2>
        <p class="landing-subtitle">
          Choose a character from the gallery to start a new conversation, or
          pick up where you left off from recent chats.
        </p>

        <div class="landing-actions">
          <a href="/gallery" class="landing-btn primary">
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="3" y="3" width="7" height="7" /><rect
                x="14"
                y="3"
                width="7"
                height="7"
              /><rect x="3" y="14" width="7" height="7" /><rect
                x="14"
                y="14"
                width="7"
                height="7"
              />
            </svg>
            Browse Characters
          </a>
        </div>

        <!-- Floating chat bubble decorations -->
        <div class="landing-bubbles">
          <div class="bubble bubble-1">
            <span class="bubble-text">Tell me about yourself...</span>
          </div>
          <div class="bubble bubble-2">
            <span class="bubble-text">✦ Writing a response...</span>
          </div>
          <div class="bubble bubble-3">
            <span class="bubble-text">What happens next?</span>
          </div>
        </div>
      </div>
    </div>
  {:else}
    <!-- Active Chat UI -->
    <div class="chat-area">
      <ChatHeader
        {characterName}
        {modelName}
        {avatarUrl}
        {showContextPanel}
        {additionalCharacters}
        {parentConversationId}
        {parentConversationTitle}
        onNavigateToParent={navigateToParent}
        onTogglePanel={() => (showContextPanel = !showContextPanel)}
        onGenerateScene={() => {
          showContextPanel = true;
        }}
      />

      {#if $activeConversationId}
        <SceneStateBar sceneState={currentSceneState} />
      {/if}

      <!-- Messages -->
      <div
        class="messages-area"
        bind:this={messagesEl}
        onscroll={onMessagesScroll}
        role="log"
        aria-label="Chat messages"
        aria-live="polite"
      >
        <!-- Reverse-scroll sentinel: triggers loading older messages -->
        <div bind:this={loadMoreSentinelEl} class="load-more-sentinel" aria-hidden="true"></div>

        <!-- Skeleton loader: visible while loading older messages -->
        {#if $isLoadingMoreMessages}
          <div class="load-more-skeleton" aria-label="Loading older messages">
            <div class="skeleton-shimmer-bar"></div>
            <div class="skeleton-messages">
              <div class="skeleton-bubble skeleton-bubble--short"></div>
              <div class="skeleton-bubble skeleton-bubble--long skeleton-bubble--right"></div>
              <div class="skeleton-bubble skeleton-bubble--medium"></div>
            </div>
          </div>
        {:else if $hasMoreMessages}
          <div class="load-more-hint">
            <span class="load-more-dot"></span>
            <span>Scroll up for earlier messages</span>
          </div>
        {/if}

        {#each $messages as message, i (message.id)}
          <div
            class="animate-fade-in-scale stagger-{Math.min(i + 1, 6)}"
            class:branch-dim={branchFromId !== null && i > branchIndex}
          >
            <ChatMessage
              {message}
              onBranch={handleBranch}
              avatarUrl={resolveMessageAvatar(message)}
              {characterName}
            />
          </div>
        {/each}

        {#if $messages.length === 0}
          <div class="empty-chat">
            <span class="empty-icon">✦</span>
            <span class="empty-title">Start a conversation</span>
            <span class="empty-desc"
              >Type a message below to begin your story with {characterName}.</span
            >
          </div>
        {/if}

        {#if $isStreaming && ($messages.length === 0 || !$messages[$messages.length - 1]?.isStreaming)}
          <div
            class="typing-indicator"
            aria-label="AI is generating a response"
          >
            {#if additionalCharacters.length > 0}
              <!-- Multi-char: stacked avatar cluster -->
              <div class="typing-avatar-stack">
                {#if avatarUrl}
                  <div class="typing-avatar-stacked" style="z-index: 3">
                    <img src={avatarUrl} alt={characterName} class="typing-avatar-img" />
                  </div>
                {:else}
                  <div class="typing-avatar-stacked typing-avatar-fallback" style="z-index: 3">
                    <div class="typing-glow"></div>
                  </div>
                {/if}
                {#each additionalCharacters.slice(0, 2) as ac, i}
                  {#if ac.avatarUrl}
                    <div class="typing-avatar-stacked" style="z-index: {2 - i}; margin-left: -10px;">
                      <img src={ac.avatarUrl} alt={ac.name} class="typing-avatar-img" />
                    </div>
                  {:else}
                    <div class="typing-avatar-stacked typing-avatar-fallback" style="z-index: {2 - i}; margin-left: -10px; background: {ac.avatarColor}">
                      <div class="typing-glow"></div>
                    </div>
                  {/if}
                {/each}
              </div>
            {:else if avatarUrl}
              <!-- Single-char: show character avatar -->
              <div class="typing-avatar">
                <img src={avatarUrl} alt={characterName} class="typing-avatar-img" />
                <div class="typing-glow"></div>
              </div>
            {:else}
              <!-- Fallback: gradient orb -->
              <div class="typing-avatar">
                <div class="typing-glow"></div>
              </div>
            {/if}
            <div class="typing-dots">
              <span class="dot dot-1"></span>
              <span class="dot dot-2"></span>
              <span class="dot dot-3"></span>
            </div>
          </div>
        {/if}

        {#if $lastStreamError && !$isStreaming}
          <div class="retry-banner">
            <span class="retry-icon">⚠</span>
            <span class="retry-text"
              >Response failed. Check your provider connection.</span
            >
            <button class="retry-btn" onclick={retryLastMessage}>
              <Icon name="refresh-cw" size={13} color="#fff" />
              <span>Retry</span>
            </button>
          </div>
        {/if}

        <!-- Floating scroll-to-bottom pill — inside messages-area for correct positioning -->
        {#if showScrollPill}
          <button
            class="scroll-pill"
            onclick={() => scrollToBottom("smooth")}
            aria-label="Scroll to latest message"
            title="Jump to latest"
          >
            <span class="scroll-pill-glow"></span>
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M3 5.5L7 9.5L11 5.5"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            <span class="scroll-pill-label"
              >{$isStreaming ? "New message" : "Jump to bottom"}</span
            >
          </button>
        {/if}
      </div>

      <!-- Branch mode banner -->
      {#if branchFromId}
        <div class="branch-banner" role="status" aria-live="polite">
          <div class="branch-banner-left">
            <span class="branch-pulse"></span>
            <svg
              class="branch-icon"
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="6" y1="3" x2="6" y2="15" />
              <circle cx="18" cy="6" r="3" />
              <circle cx="6" cy="18" r="3" />
              <path d="M18 9a9 9 0 0 1-9 9" />
            </svg>
            <span class="branch-label">Branching from:</span>
            <span class="branch-preview"
              >"{branchFromContent.slice(0, 60)}{branchFromContent.length > 60
                ? "…"
                : ""}"</span
            >
          </div>
          <button
            class="branch-cancel"
            onclick={cancelBranch}
            aria-label="Cancel branch"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M2 2l8 8M10 2l-8 8"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
              />
            </svg>
          </button>
        </div>
      {/if}

      <ChatInput
        bind:value={inputText}
        {modelName}
        {tokenCount}
        onSend={handleSend}
        disabled={$isStreaming}
        bind:selectedModel
        {availableModels}
        onRefreshModels={refreshModels}
        isBranching={branchFromId !== null}
      />
    </div>

    <!-- Context Panel -->
    {#if showContextPanel && $activeConversationId}
      <ContextPanel
        {characterId}
        {characterName}
        characterTagline={characterDescription}
        {avatarUrl}
        tags={characterTags}
        {additionalCharacters}
        conversationId={$activeConversationId}
        onClose={() => (showContextPanel = false)}
      />
    {/if}
  {/if}
</div>

<style>
  .chat-view {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /* ===== Landing Screen ===== */
  .landing-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
    background: radial-gradient(
      ellipse at 50% 40%,
      rgba(139, 92, 246, 0.06) 0%,
      transparent 70%
    );
  }

  .landing-orbs {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .orb {
    position: absolute;
    border-radius: 50%;
    filter: blur(80px);
    opacity: 0.15;
    animation: orbFloat 20s ease-in-out infinite;
  }

  .orb-1 {
    width: 300px;
    height: 300px;
    background: var(--accent-primary);
    top: 10%;
    left: 20%;
    animation-delay: 0s;
  }

  .orb-2 {
    width: 200px;
    height: 200px;
    background: var(--accent-secondary);
    bottom: 20%;
    right: 15%;
    animation-delay: -7s;
  }

  .orb-3 {
    width: 250px;
    height: 250px;
    background: var(--accent-tertiary);
    top: 50%;
    left: 60%;
    animation-delay: -14s;
    opacity: 0.08;
  }

  @keyframes orbFloat {
    0%,
    100% {
      transform: translate(0, 0) scale(1);
    }
    25% {
      transform: translate(30px, -20px) scale(1.05);
    }
    50% {
      transform: translate(-20px, 15px) scale(0.95);
    }
    75% {
      transform: translate(15px, 25px) scale(1.02);
    }
  }

  .landing-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    z-index: 1;
    max-width: 480px;
    padding: 32px;
  }

  .landing-icon-group {
    position: relative;
    width: 96px;
    height: 96px;
    margin-bottom: 28px;
  }

  .landing-glow {
    position: absolute;
    inset: -16px;
    border-radius: 50%;
    background: radial-gradient(
      circle,
      rgba(139, 92, 246, 0.3),
      transparent 70%
    );
    animation: glowPulse 3s ease-in-out infinite;
  }

  .landing-icon {
    width: 96px;
    height: 96px;
    position: relative;
    z-index: 1;
    animation: iconFloat 6s ease-in-out infinite;
  }

  @keyframes iconFloat {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-8px);
    }
  }

  .landing-title {
    font-size: var(--text-3xl);
    font-weight: 700;
    color: var(--fg-primary);
    margin-bottom: 12px;
    letter-spacing: -0.5px;
    background: linear-gradient(
      135deg,
      var(--fg-primary),
      var(--accent-primary)
    );
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .landing-subtitle {
    font-size: var(--text-base);
    color: var(--fg-muted);
    line-height: 1.7;
    margin-bottom: 32px;
    max-width: 360px;
  }

  .landing-actions {
    display: flex;
    gap: 12px;
  }

  .landing-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-radius: var(--rounded-lg);
    font-size: var(--text-base);
    font-weight: 600;
    text-decoration: none;
    transition: all var(--duration-normal) var(--ease-out);
  }

  .landing-btn.primary {
    background: linear-gradient(
      135deg,
      var(--accent-primary),
      var(--accent-secondary)
    );
    color: white;
    box-shadow: 0 4px 20px rgba(139, 92, 246, 0.3);
  }

  .landing-btn.primary:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 28px rgba(139, 92, 246, 0.45);
  }

  /* Floating chat bubbles decoration */
  .landing-bubbles {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .bubble {
    position: absolute;
    padding: 8px 14px;
    border-radius: 12px;
    opacity: 0;
    animation: bubbleFloat 12s ease-in-out infinite;
  }

  .bubble-text {
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .bubble-1 {
    background: var(--user-bubble);
    color: rgba(255, 255, 255, 0.7);
    top: 15%;
    right: -60px;
    border-radius: 12px 12px 2px 12px;
    animation-delay: 1s;
  }

  .bubble-2 {
    background: var(--ai-bubble);
    border: 1px solid var(--border-subtle);
    color: var(--fg-muted);
    bottom: 25%;
    left: -40px;
    border-radius: 2px 12px 12px 12px;
    animation-delay: 4s;
  }

  .bubble-3 {
    background: var(--user-bubble);
    color: rgba(255, 255, 255, 0.7);
    bottom: 40%;
    right: -80px;
    border-radius: 12px 12px 2px 12px;
    animation-delay: 7s;
  }

  @keyframes bubbleFloat {
    0% {
      opacity: 0;
      transform: translateY(20px) scale(0.9);
    }
    8% {
      opacity: 0.5;
      transform: translateY(0) scale(1);
    }
    25% {
      opacity: 0.5;
      transform: translateY(-10px) scale(1);
    }
    33% {
      opacity: 0;
      transform: translateY(-20px) scale(0.95);
    }
    100% {
      opacity: 0;
    }
  }

  /* ===== Chat UI (active conversation) ===== */
  .chat-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative; /* Anchor for scroll-pill absolute positioning */
  }

  .messages-area {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    /* Smooth scrollbar for the container */
    scroll-behavior: auto;
  }

  /* ───────────────────────────────────────────────
     REVERSE-SCROLL SENTINEL + SKELETON LOADER
  ─────────────────────────────────────────────── */
  .load-more-sentinel {
    width: 100%;
    height: 1px;
    flex-shrink: 0;
  }

  .load-more-skeleton {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 0 8px;
    animation: skeletonFadeIn 200ms ease-out both;
  }

  @keyframes skeletonFadeIn {
    from { opacity: 0; transform: translateY(-8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* Animated shimmer bar at top */
  .skeleton-shimmer-bar {
    width: 100%;
    height: 2px;
    border-radius: 1px;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(139, 92, 246, 0.3) 25%,
      rgba(0, 242, 255, 0.2) 50%,
      rgba(139, 92, 246, 0.3) 75%,
      transparent 100%
    );
    background-size: 200% 100%;
    animation: shimmerSlide 1.5s ease-in-out infinite;
  }

  @keyframes shimmerSlide {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  .skeleton-messages {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 8px;
  }

  /* Skeleton message bubbles */
  .skeleton-bubble {
    height: 28px;
    border-radius: 14px;
    background: rgba(139, 92, 246, 0.06);
    border: 1px solid rgba(139, 92, 246, 0.08);
    position: relative;
    overflow: hidden;
    animation: skeletonPulse 1.8s ease-in-out infinite;
  }

  .skeleton-bubble::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(139, 92, 246, 0.08) 50%,
      transparent 100%
    );
    background-size: 200% 100%;
    animation: shimmerSlide 1.8s ease-in-out infinite;
  }

  .skeleton-bubble--short { width: 40%; }
  .skeleton-bubble--medium { width: 60%; }
  .skeleton-bubble--long { width: 75%; }
  .skeleton-bubble--right { align-self: flex-end; }

  /* Stagger the pulse animation */
  .skeleton-bubble:nth-child(2) {
    animation-delay: 200ms;
  }
  .skeleton-bubble:nth-child(2)::after {
    animation-delay: 200ms;
  }
  .skeleton-bubble:nth-child(3) {
    animation-delay: 400ms;
  }
  .skeleton-bubble:nth-child(3)::after {
    animation-delay: 400ms;
  }

  @keyframes skeletonPulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }

  /* "Scroll up for earlier messages" hint */
  .load-more-hint {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 0 4px;
    font-size: 11px;
    font-family: var(--font-body);
    color: rgba(139, 92, 246, 0.35);
    letter-spacing: 0.02em;
    user-select: none;
  }

  .load-more-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: rgba(139, 92, 246, 0.3);
    animation: dotPulse 2s ease-in-out infinite;
  }

  @keyframes dotPulse {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 0.8; transform: scale(1.2); }
  }

  /* ───────────────────────────────────────────────
     SCROLL-TO-BOTTOM PILL — Floating glassmorphic indicator
  ─────────────────────────────────────────────── */
  .scroll-pill {
    position: sticky;
    bottom: 2px;
    align-self: center;
    z-index: 20;
    margin-top: -2px; /* Pull up slightly so it doesn't add visual gap */

    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px 8px 12px;
    border: none;
    border-radius: 999px;
    cursor: pointer;

    /* Glassmorphism */
    background: rgba(11, 11, 28, 0.85);
    border: 1px solid rgba(139, 92, 246, 0.25);
    backdrop-filter: blur(20px) saturate(160%);
    box-shadow:
      0 0 0 1px rgba(139, 92, 246, 0.08),
      0 4px 24px rgba(0, 0, 0, 0.5),
      0 1px 0 rgba(255, 255, 255, 0.04) inset;

    color: rgba(196, 161, 255, 0.9);
    font-family: var(--font-body);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.03em;

    /* Entrance animation */
    animation: pillSlideIn 320ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
    transition: all 200ms ease;
  }

  @keyframes pillSlideIn {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.9);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .scroll-pill:hover {
    background: rgba(17, 17, 38, 0.92);
    border-color: rgba(139, 92, 246, 0.45);
    box-shadow:
      0 0 0 1px rgba(139, 92, 246, 0.15),
      0 6px 32px rgba(0, 0, 0, 0.6),
      0 0 20px rgba(139, 92, 246, 0.12),
      0 1px 0 rgba(255, 255, 255, 0.06) inset;
    color: #c4a1ff;
    transform: translateY(-2px);
  }

  .scroll-pill:active {
    transform: scale(0.95);
  }

  /* Pulsing glow dot */
  .scroll-pill-glow {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: linear-gradient(135deg, #7c3aed, #00f2ff);
    box-shadow: 0 0 8px rgba(124, 58, 237, 0.6);
    animation: pillGlowPulse 1.8s ease-in-out infinite;
    flex-shrink: 0;
  }

  @keyframes pillGlowPulse {
    0%,
    100% {
      box-shadow: 0 0 4px rgba(124, 58, 237, 0.4);
      opacity: 0.7;
    }
    50% {
      box-shadow:
        0 0 12px rgba(124, 58, 237, 0.8),
        0 0 20px rgba(0, 242, 255, 0.2);
      opacity: 1;
    }
  }

  .scroll-pill-label {
    white-space: nowrap;
  }

  .scroll-pill svg {
    flex-shrink: 0;
    animation: pillChevronBounce 1.5s ease-in-out infinite;
  }

  @keyframes pillChevronBounce {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(2px);
    }
  }

  .empty-chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    opacity: 0.5;
  }

  .empty-icon {
    font-size: 48px;
    color: var(--accent-primary);
    margin-bottom: 8px;
  }

  .empty-title {
    font-size: var(--text-xl);
    font-weight: 600;
    color: var(--fg-secondary);
  }

  .empty-desc {
    font-size: var(--text-md);
    color: var(--fg-muted);
    max-width: 300px;
    text-align: center;
  }

  @media (max-width: 768px) {
    .messages-area {
      padding: 16px 12px;
    }
  }

  /* Typing Indicator */
  .typing-indicator {
    display: flex;
    align-items: center;
    gap: 12px;
    animation: fadeSlideUp 300ms var(--ease-out) forwards;
  }

  .typing-avatar {
    width: 32px;
    height: 32px;
    border-radius: var(--rounded-full);
    background: linear-gradient(
      135deg,
      var(--accent-primary),
      var(--accent-secondary)
    );
    flex-shrink: 0;
    position: relative;
    animation: gentlePulse 2s ease-in-out infinite;
  }

  .typing-glow {
    position: absolute;
    inset: -3px;
    border-radius: var(--rounded-full);
    background: linear-gradient(
      135deg,
      var(--accent-primary),
      var(--accent-secondary)
    );
    opacity: 0.2;
    filter: blur(6px);
    animation: glowPulse 2s ease-in-out infinite;
  }

  /* Avatar image inside typing indicator */
  .typing-avatar-img {
    width: 100%; height: 100%;
    object-fit: cover;
    border-radius: 50%;
    display: block;
  }

  /* Stacked avatar cluster for multi-char */
  .typing-avatar-stack {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    animation: gentlePulse 2s ease-in-out infinite;
  }
  .typing-avatar-stacked {
    width: 28px; height: 28px;
    border-radius: 50%;
    overflow: hidden;
    position: relative;
    border: 2px solid rgba(6, 6, 18, 0.9);
    box-shadow: 0 0 8px rgba(139, 92, 246, 0.2);
    flex-shrink: 0;
  }
  .typing-avatar-fallback {
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
  }

  .typing-dots {
    display: flex;
    gap: 4px;
    padding: 10px 16px;
    background: var(--ai-bubble);
    border: 1px solid var(--border-subtle);
    border-radius: 2px 12px 12px 12px;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--rounded-full);
    background: var(--fg-muted);
    animation: dotBounce 1.4s ease-in-out infinite;
  }

  .dot-1 {
    animation-delay: 0ms;
  }
  .dot-2 {
    animation-delay: 160ms;
  }
  .dot-3 {
    animation-delay: 320ms;
  }

  @keyframes dotBounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    30% {
      transform: translateY(-6px);
      opacity: 1;
    }
  }

  @keyframes gentlePulse {
    0%,
    100% {
      transform: scale(1);
    }
    50% {
      transform: scale(1.05);
    }
  }

  @keyframes glowPulse {
    0%,
    100% {
      opacity: 0.15;
    }
    50% {
      opacity: 0.35;
    }
  }

  @keyframes fadeSlideUp {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  /* Retry Banner */
  .retry-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    margin: 8px 0;
    background: rgba(244, 63, 94, 0.08);
    border: 1px solid rgba(244, 63, 94, 0.15);
    border-radius: 12px;
    backdrop-filter: blur(8px);
    animation: fadeSlideUp 200ms var(--ease-out);
  }

  .retry-icon {
    font-size: var(--text-lg);
    flex-shrink: 0;
  }

  .retry-text {
    flex: 1;
    font-size: var(--text-sm);
    color: #e0a0a8;
    font-family: var(--font-body);
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 14px;
    border-radius: 8px;
    background: linear-gradient(135deg, #8b5cf6, #bf40ff);
    border: none;
    color: #fff;
    font-size: var(--text-sm);
    font-weight: 600;
    font-family: var(--font-body);
    cursor: pointer;
    flex-shrink: 0;
    transition: all 150ms;
    box-shadow: 0 2px 10px rgba(139, 92, 246, 0.25);
  }

  .retry-btn:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 16px rgba(139, 92, 246, 0.4);
  }

  .retry-btn:active {
    transform: scale(0.95);
  }

  /* ═══════════════════════════════════════
     Branching Mode
  ═══════════════════════════════════════ */

  /* Messages after the branch point fade & desaturate */
  .branch-dim {
    opacity: 0.22;
    filter: grayscale(0.6);
    pointer-events: none;
    transition:
      opacity 300ms ease,
      filter 300ms ease;
  }

  /* Frosted-glass banner that appears above ChatInput */
  .branch-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 0 0 2px;
    padding: 9px 14px 9px 12px;
    background: rgba(0, 242, 255, 0.04);
    border: 1px solid rgba(0, 242, 255, 0.14);
    border-radius: 12px;
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow:
      0 0 0 1px rgba(0, 242, 255, 0.04) inset,
      0 4px 24px rgba(0, 0, 0, 0.15);
    animation: branchBannerIn 220ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes branchBannerIn {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .branch-banner-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  /* Pulsing cyan dot */
  .branch-pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #00f2ff;
    flex-shrink: 0;
    box-shadow: 0 0 8px rgba(0, 242, 255, 0.7);
    animation: branchPulse 1.6s ease-in-out infinite;
  }
  @keyframes branchPulse {
    0%,
    100% {
      box-shadow: 0 0 6px rgba(0, 242, 255, 0.5);
      opacity: 0.8;
    }
    50% {
      box-shadow: 0 0 14px rgba(0, 242, 255, 0.9);
      opacity: 1;
    }
  }

  .branch-icon {
    color: rgba(0, 242, 255, 0.7);
    flex-shrink: 0;
  }

  .branch-label {
    font-size: 11px;
    font-weight: 600;
    color: rgba(0, 242, 255, 0.7);
    white-space: nowrap;
    letter-spacing: 0.2px;
    flex-shrink: 0;
  }

  .branch-preview {
    font-size: 11px;
    color: rgba(0, 242, 255, 0.45);
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    font-family: var(--font-body);
  }

  .branch-cancel {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    border: 1px solid rgba(0, 242, 255, 0.1);
    background: transparent;
    color: rgba(0, 242, 255, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: all 150ms ease;
  }
  .branch-cancel:hover {
    background: rgba(0, 242, 255, 0.08);
    border-color: rgba(0, 242, 255, 0.25);
    color: rgba(0, 242, 255, 0.8);
    transform: scale(1.1);
  }
  .branch-cancel:active {
    transform: scale(0.92);
  }
</style>
