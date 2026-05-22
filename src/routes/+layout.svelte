<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import ErrorBoundary from '$lib/components/ErrorBoundary.svelte';
  import ToastContainer from '$lib/components/ToastContainer.svelte';
  import { settings } from '$lib/stores/settings';
  import type { NavItem } from '$lib/types';

  let { children } = $props();
  
  let sidebarCollapsed = $state(false);

  // Navigation items for the sidebar
  const navItems = [
    { path: '/',          label: 'Chats',     icon: 'message-circle' },
    { path: '/gallery',   label: 'Characters', icon: 'users' },
    { path: '/memories',  label: 'Memories',  icon: 'brain' },
    { path: '/providers',  label: 'Providers',  icon: 'plug',   group: 'ai-studio' },
    { path: '/models',     label: 'LLM Models',     icon: 'layers', group: 'ai-studio' },
    { path: '/embedders',  label: 'Embedding Models',  icon: 'zap',    group: 'ai-studio' },
    { path: '/settings',  label: 'Settings',  icon: 'settings' },
  ] as const satisfies readonly NavItem[];

  let currentPath = $derived($page.url.pathname);

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
      sidebarCollapsed = !sidebarCollapsed;
    }
    // Escape → Blur active element (close dropdowns, deselect inputs)
    if (e.key === 'Escape') {
      const active = document.activeElement as HTMLElement | null;
      active?.blur();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Skip navigation for keyboard/screen reader users -->
<a href="#main-content" class="sr-only">Skip to content</a>

<div class="app-shell" class:sidebar-collapsed={sidebarCollapsed}>
  <Sidebar 
    {navItems} 
    {currentPath} 
    collapsed={sidebarCollapsed}
    onNavigate={(path) => goto(path)}
    onToggleCollapse={() => sidebarCollapsed = !sidebarCollapsed}
  />
  <main id="main-content" class="app-content">
    <ErrorBoundary fallbackTitle="This page encountered an error">
      {@render children()}
    </ErrorBoundary>
  </main>
</div>

<ToastContainer />

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
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
