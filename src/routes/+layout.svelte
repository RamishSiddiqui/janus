<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { browser } from '$app/env';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import TitleBar from '#lib/components/TitleBar.svelte';
  import ErrorBoundary from '#lib/components/ErrorBoundary.svelte';
  import ToastContainer from '#lib/components/ToastContainer.svelte';
  import UpdatePrompt from '#lib/components/UpdatePrompt.svelte';
  import { startAutoUpdateChecks, stopAutoUpdateChecks } from '#lib/stores/updates.js';
  import FirstRunWizard from '#lib/components/FirstRunWizard.svelte';
  import WebGate from '#lib/components/WebGate.svelte';
  import {
    SIGNED_OUT_EVENT,
    closeEvents,
    getInitialStatus,
    isWebMode,
    webAuth,
    type WebStatus,
    type WebUser,
  } from '#lib/services/webBridge.js';
  import { shouldShowWizard } from '#lib/utils/onboarding.js';
  import { settings } from '#lib/stores/settings.js';
  import { initMultiCharListener, cleanupMultiCharListener } from '#lib/stores/chat.js';
  import { initTtsPlaybackListener, cleanupTtsPlaybackListener, isSpeaking, stop as stopTtsPlayback } from '#lib/stores/ttsPlayback.js';
  import { initFrontendLogCapture } from '#lib/stores/logs.js';
  import type { NavItem } from '#lib/types/index.js';

  let { children } = $props();

  let sidebarCollapsed = $state(false);

  // Phone layout: the sidebar becomes a drawer opened from the title bar.
  let isMobile = $state(false);
  let mobileNavOpen = $state(false);
  if (browser) {
    const mobileQuery = window.matchMedia('(max-width: 768px)');
    isMobile = mobileQuery.matches;
    mobileQuery.addEventListener('change', (e) => {
      isMobile = e.matches;
      if (!e.matches) mobileNavOpen = false;
    });
  }

  function toggleSidebar() {
    if (isMobile) mobileNavOpen = !mobileNavOpen;
    else sidebarCollapsed = !sidebarCollapsed;
  }
  let showWizard = $state(false);

  // Browser access: until someone signs in, only the sign-in screen renders.
  const webMode = isWebMode();
  const startStatus = getInitialStatus();
  let gateStatus = $state<WebStatus | null>(startStatus);
  let webUser = $state<WebUser | null>(
    startStatus?.user && !startStatus.user.must_change ? startStatus.user : null,
  );

  async function backToGate() {
    closeEvents();
    webUser = null;
    try {
      gateStatus = await webAuth.status();
    } catch {
      // keep the last known status; the gate still works
    }
  }

  async function signOut() {
    try {
      await webAuth.logout();
    } catch {
      // the session may already be gone
    }
    await backToGate();
  }

  onMount(() => {
    if (!webMode) return;
    const onOut = () => void backToGate();
    window.addEventListener(SIGNED_OUT_EVENT, onOut);
    return () => window.removeEventListener(SIGNED_OUT_EVENT, onOut);
  });

  // First-run setup guide: only for someone with no chat provider yet.
  // An existing user is marked done silently so an upgrade never nags them.
  let wizardChecked = false;
  $effect(() => {
    if (!browser || wizardChecked) return;
    if (!('__TAURI_INTERNALS__' in window) || $settings.onboardingDone) return;
    if (webMode && !webUser) return;
    wizardChecked = true;
    void (async () => {
      try {
        const ipc = await import('#lib/services/ipc.js');
        const llm = await ipc.listProviders('llm');
        if (shouldShowWizard(false, llm.length)) showWizard = true;
        else settings.update((s) => ({ ...s, onboardingDone: true }));
      } catch (err) {
        console.error('[onboarding] provider check failed:', err);
      }
    })();
  });

  // As early as possible so nothing logged during the rest of this page's
  // own initialization is missed.
  if (browser) initFrontendLogCapture();

  // The cold-start splash (#janus-splash in app.html) renders as plain HTML
  // before Svelte even boots, so there's no blank-frame flash while the
  // webview spins up. Once this layout has actually mounted — real content
  // is ready behind it — fade it out and drop it from the DOM.
  onMount(() => {
    const splash = document.getElementById('janus-splash');
    if (!splash) return;
    splash.classList.add('janus-splash-hidden');
    setTimeout(() => splash.remove(), 400);
  });

  // Quiet background check for a newer Janus (desktop only; the store skips
  // browsers, dev builds and anyone who turned it off in Settings).
  onMount(() => {
    startAutoUpdateChecks();
    return stopAutoUpdateChecks;
  });

  // Navigation items for the sidebar
  const navItems = [
    { path: '/',          label: 'Chats',     icon: 'message-circle' },
    { path: '/gallery',   label: 'Characters', icon: 'users' },
    { path: '/personas',  label: 'Personas',  icon: 'user' },
    { path: '/memories',  label: 'Memories',  icon: 'brain' },
    { path: '/providers',  label: 'Providers',  icon: 'plug',   group: 'ai-studio' },
    { path: '/models',     label: 'LLM Models',     icon: 'layers', group: 'ai-studio' },
    { path: '/media-models', label: 'Image/Video Models', icon: 'image', group: 'ai-studio' },
    { path: '/embedders',  label: 'Embedding Models',  icon: 'zap',    group: 'ai-studio' },
    { path: '/voice-models', label: 'Voice Models', icon: 'volume-2', group: 'ai-studio' },
    { path: '/trash',     label: 'Trash',     icon: 'trash-2' },
    { path: '/settings',  label: 'Settings',  icon: 'settings' },
  ] as const satisfies readonly NavItem[];

  let currentPath = $derived(page.url.pathname);

  // Resolve effective theme (dark/light) from user pref + system
  let systemPrefersDark = $state(true);
  if (browser) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    systemPrefersDark = mq.matches;
    mq.addEventListener('change', (e) => systemPrefersDark = e.matches);
  }

  let effectiveTheme = $derived(
    $settings.theme === 'system'
      ? (systemPrefersDark ? 'dark' : 'light')
      : $settings.theme
  );

  // Apply theme to document root for CSS variable switching
  $effect(() => {
    if (browser) {
      document.documentElement.setAttribute('data-theme', effectiveTheme);
    }
  });

  // Apply font size setting to CSS variable
  const fontSizeMap: Record<string, string> = { Small: '13px', Medium: '14px', Large: '16px' };
  $effect(() => {
    if (browser) {
      document.documentElement.style.setProperty(
        '--app-font-size', fontSizeMap[$settings.fontSize] ?? '14px'
      );
    }
  });

  // Initialize multi-character response listener
  $effect(() => {
    if (!browser) return;
    initMultiCharListener();
    return () => cleanupMultiCharListener();
  });

  // Initialize the TTS streamed-playback listener (a no-op subscription
  // when TTS is disabled — the listener itself gates on $settings.ttsEnabled
  // per chunk, see ttsPlayback.ts).
  $effect(() => {
    if (!browser) return;
    initTtsPlaybackListener();
    return () => cleanupTtsPlaybackListener();
  });

  // Warm the TTS engine at startup (fire-and-forget, non-blocking) when the
  // feature is enabled — the ONNX session + G2P engine load is a real ~4-5s
  // cost measured in practice, and paying it here means it's already done
  // by the time the user's first message/preview needs it, instead of
  // stacking on top of that first interaction. `tts_preload_engine` itself
  // no-ops quietly if the model isn't downloaded yet.
  $effect(() => {
    if (!browser || !('__TAURI_INTERNALS__' in window) || !$settings.ttsEnabled) return;
    import('#lib/services/ipc.js').then((ipc) => {
      ipc.ttsPreloadEngine().catch((err) => console.error('[tts] Preload failed:', err));
    });
  });

  /** Global keyboard shortcuts */
  function handleKeydown(e: KeyboardEvent) {
    // Ctrl/Cmd + N → New chat
    if ((e.ctrlKey || e.metaKey) && e.key === 'n') {
      e.preventDefault();
      goto('/');
    }
    // Ctrl/Cmd + B → Toggle sidebar
    if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
      e.preventDefault();
      toggleSidebar();
    }
    // Escape → Blur active element (close dropdowns, deselect inputs)
    if (e.key === 'Escape') {
      mobileNavOpen = false;
      const active = document.activeElement as HTMLElement | null;
      active?.blur();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Skip navigation for keyboard/screen reader users -->
<a href="#main-content" class="sr-only">Skip to content</a>

{#if webMode && !webUser && gateStatus}
  <WebGate status={gateStatus} onenter={(u) => (webUser = u)} />
{:else}
{#if showWizard}
  <FirstRunWizard onclose={() => (showWizard = false)} />
{/if}

<div class="app-frame">
  <TitleBar
    onToggleSidebar={toggleSidebar}
    webUser={webUser?.username ?? null}
    onSignOut={signOut}
  />
  <div class="app-shell" class:sidebar-collapsed={sidebarCollapsed}>
    <Sidebar
      {navItems}
      {currentPath}
      collapsed={sidebarCollapsed && !isMobile}
      mobileOpen={mobileNavOpen}
      onMobileClose={() => (mobileNavOpen = false)}
      onNavigate={(path) => goto(path)}
      onToggleCollapse={() => sidebarCollapsed = !sidebarCollapsed}
    />
    {#if isMobile && mobileNavOpen}
      <button class="drawer-backdrop" aria-label="Close menu" onclick={() => (mobileNavOpen = false)}></button>
    {/if}
    <main id="main-content" class="app-content">
      <ErrorBoundary fallbackTitle="This page encountered an error">
        {@render children()}
      </ErrorBoundary>
    </main>
  </div>
</div>
{/if}

<ToastContainer />
{#if !webMode}<UpdatePrompt />{/if}

<style>
  .drawer-backdrop {
    position: fixed;
    inset: 34px 0 0 0;
    z-index: 90;
    border: 0;
    background: rgba(5, 5, 15, 0.6);
    backdrop-filter: blur(2px);
  }
  .app-frame {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* Phones: the visible height, not the height behind the browser's toolbars. */
    height: 100dvh;
    width: 100%;
    overflow: hidden;
  }

  .app-shell {
    display: flex;
    flex: 1;
    min-height: 0;
    width: 100%;
    overflow: hidden;
    background: var(--surface-inverse);
  }

  .app-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
