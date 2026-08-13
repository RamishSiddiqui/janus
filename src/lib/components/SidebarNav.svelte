<script lang="ts">
  import Icon from './Icon.svelte';
  import { browser } from '$app/environment';
  import type { NavItem } from '$lib/types';

  let {
    navItems, currentPath, collapsed = false, onNavigate,
  }: {
    navItems: readonly NavItem[]; currentPath: string; collapsed?: boolean;
    onNavigate: (path: string) => void;
  } = $props();

  // AI Studio accordion — persisted in localStorage
  let aiStudioOpen = $state(
    browser ? (localStorage.getItem('sidebar-ai-studio-open') !== 'false') : true
  );
  function toggleAiStudio() {
    aiStudioOpen = !aiStudioOpen;
    if (browser) localStorage.setItem('sidebar-ai-studio-open', String(aiStudioOpen));
  }
</script>

<nav class="sb-nav" aria-label="Main navigation">
  <!-- Flat (ungrouped) items first -->
  {#each navItems.filter(i => i.path !== '/settings' && i.path !== '/trash' && !i.group) as item (item.path)}
    {@const isActive = currentPath === item.path}
    <button class="sb-nav-item" class:active={isActive} class:collapsed onclick={() => onNavigate(item.path)}
      title={collapsed ? item.label : undefined} aria-current={isActive ? 'page' : undefined}>
      {#if isActive}<span class="nav-glow-bar"></span>{/if}
      <span class="nav-icon"><Icon name={item.icon} size={16} color={isActive ? '#c4a1ff' : '#6b6b8a'} /></span>
      {#if !collapsed}<span class="nav-text">{item.label}</span>{/if}
    </button>
  {/each}

  <!-- AI Studio accordion group -->
  {#if !collapsed}
    {@const studioItems = navItems.filter(i => i.group === 'ai-studio')}
    {@const studioActive = studioItems.some(i => currentPath.startsWith(i.path))}
    {@const studioOpen = aiStudioOpen || studioActive}
    <div class="nav-group" class:group-active={studioActive}>
      <button class="nav-group-header" onclick={toggleAiStudio}
        aria-expanded={studioOpen} aria-label="AI Studio section">
        <span class="nav-group-icon">
          <Icon name="cpu" size={15} color={studioActive ? '#c4a1ff' : '#6b6b8a'} />
        </span>
        <span class="nav-group-label" class:active={studioActive}>AI Studio</span>
        <span class="nav-group-chevron" class:open={studioOpen}>
          <Icon name="chevron-right" size={13} color={studioActive ? '#c4a1ff' : '#5a5a7a'} />
        </span>
      </button>
      {#if studioOpen}
        <div class="nav-sub-list">
          <div class="nav-sub-rail"></div>
          {#each studioItems as item (item.path)}
            {@const isActive = currentPath === item.path || currentPath.startsWith(item.path + '/')}
            <button class="nav-sub-item" class:active={isActive}
              onclick={() => onNavigate(item.path)}
              aria-current={isActive ? 'page' : undefined}>
              {#if isActive}<span class="nav-sub-accent"></span>{/if}
              <span class="nav-sub-icon">
                <Icon name={item.icon} size={14} color={isActive ? '#c4a1ff' : '#5a5a7a'} />
              </span>
              <span class="nav-sub-text">{item.label}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <!-- Collapsed: show sub-items as icon-only -->
    {#each navItems.filter(i => i.group === 'ai-studio') as item (item.path)}
      {@const isActive = currentPath === item.path}
      <button class="sb-nav-item" class:active={isActive} class:collapsed onclick={() => onNavigate(item.path)}
        title={item.label} aria-current={isActive ? 'page' : undefined}>
        {#if isActive}<span class="nav-glow-bar"></span>{/if}
        <span class="nav-icon"><Icon name={item.icon} size={16} color={isActive ? '#c4a1ff' : '#6b6b8a'} /></span>
      </button>
    {/each}
  {/if}
</nav>

<style>
  .sb-nav {
    display: flex; flex-direction: column; gap: 2px; flex-shrink: 0;
    position: relative; z-index: 1;
    overflow: hidden;
  }
  .sb-nav-item {
    display: flex; align-items: center; gap: 11px;
    padding: 10px 14px; border-radius: 10px;
    border: 1px solid transparent; background: transparent;
    color: #8b8ba7; font-size: var(--text-md); font-weight: 500;
    font-family: var(--font-body); width: 100%; text-align: left;
    cursor: pointer; position: relative; overflow: hidden;
    transition: all 180ms var(--ease-out);
  }
  .sb-nav-item:hover {
    background: rgba(139,92,246,0.07);
    border-color: rgba(139,92,246,0.06);
    color: #c8c8e0;
  }
  .sb-nav-item.active {
    background: linear-gradient(90deg, rgba(139,92,246,0.12) 0%, rgba(139,92,246,0.04) 100%);
    border-color: rgba(139,92,246,0.1);
    color: #e8e0ff;
    font-weight: 600;
  }
  .nav-glow-bar {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 3px; height: 22px; border-radius: 0 6px 6px 0;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 14px rgba(139,92,246,0.7), 0 0 4px rgba(191,64,255,0.9);
    animation: barPulse 2.5s ease-in-out infinite;
  }
  @keyframes barPulse { 0%,100% { box-shadow: 0 0 10px rgba(139,92,246,0.5); } 50% { box-shadow: 0 0 18px rgba(139,92,246,0.8), 0 0 6px rgba(191,64,255,0.6); } }

  .nav-icon { display: flex; align-items: center; width: 20px; height: 20px; flex-shrink: 0; }
  .nav-text { white-space: nowrap; }
  .sb-nav-item.collapsed { justify-content: center; padding: 10px; }

  /* ── AI Studio Accordion Group ── */
  .nav-group {
    border-radius: 10px;
    border: 1px solid transparent;
    overflow: hidden;
    transition: border-color 200ms, background 200ms;
  }
  .nav-group.group-active {
    background: rgba(139,92,246,0.04);
    border-color: rgba(139,92,246,0.08);
  }
  .nav-group-header {
    display: flex; align-items: center; gap: 10px;
    padding: 9px 12px; width: 100%;
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-body); text-align: left;
    transition: background 160ms;
    border-radius: 10px;
  }
  .nav-group-header:hover { background: rgba(139,92,246,0.06); }
  .nav-group-icon { display: flex; align-items: center; width: 20px; height: 20px; flex-shrink: 0; }
  .nav-group-label {
    flex: 1; font-size: var(--text-md); font-weight: 500; color: #6b6b8a;
    white-space: nowrap; transition: color 160ms;
  }
  .nav-group-label.active { color: #c4a1ff; font-weight: 600; }
  .nav-group-chevron {
    display: flex; flex-shrink: 0;
    transition: transform 220ms cubic-bezier(0.34,1.56,0.64,1);
  }
  .nav-group-chevron.open { transform: rotate(90deg); }

  .nav-sub-list {
    position: relative;
    padding: 2px 0 6px 14px;
    display: flex; flex-direction: column; gap: 1px;
    animation: subListIn 200ms ease both;
  }
  @keyframes subListIn {
    from { opacity: 0; transform: translateY(-6px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .nav-sub-rail {
    position: absolute; left: 22px; top: 4px; bottom: 8px;
    width: 1.5px;
    background: linear-gradient(180deg, rgba(139,92,246,0.2) 0%, rgba(139,92,246,0.04) 100%);
    border-radius: 2px;
  }
  .nav-sub-item {
    display: flex; align-items: center; gap: 9px;
    padding: 7px 10px 7px 20px;
    border-radius: 8px; width: 100%;
    background: transparent; border: 1px solid transparent;
    font-family: var(--font-body); text-align: left; cursor: pointer;
    position: relative; transition: all 150ms;
  }
  .nav-sub-item:hover {
    background: rgba(139,92,246,0.07);
    border-color: rgba(139,92,246,0.06);
  }
  .nav-sub-item.active {
    background: linear-gradient(90deg, rgba(139,92,246,0.14) 0%, rgba(139,92,246,0.04) 100%);
    border-color: rgba(139,92,246,0.12);
  }
  .nav-sub-accent {
    position: absolute; left: 0; top: 50%; transform: translateY(-50%);
    width: 2.5px; height: 16px; border-radius: 0 4px 4px 0;
    background: linear-gradient(180deg, #8B5CF6, #bf40ff);
    box-shadow: 0 0 10px rgba(139,92,246,0.7);
    animation: barPulse 2.5s ease-in-out infinite;
  }
  .nav-sub-icon { display: flex; align-items: center; width: 18px; height: 18px; flex-shrink: 0; }
  .nav-sub-text {
    font-size: 13px; font-weight: 500; color: #6b6b8a;
    white-space: nowrap; transition: color 150ms;
  }
  .nav-sub-item:hover .nav-sub-text { color: #c8c8e0; }
  .nav-sub-item.active .nav-sub-text { color: #e8e0ff; font-weight: 600; }
</style>
