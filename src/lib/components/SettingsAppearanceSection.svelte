<script lang="ts">
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { success } from '$lib/stores/toast';

  let theme = $state($settings.theme);
  let fontSize = $state($settings.fontSize);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { theme, fontSize };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  const fontSizes = ['Small', 'Medium', 'Large'] as const;
  let showFontDropdown = $state(false);
  let dropdownStyle = $state('');

  function toggleFontDropdown() {
    showFontDropdown = !showFontDropdown;
    if (showFontDropdown) {
      const btn = document.querySelector('.setting-dropdown') as HTMLElement | null;
      if (btn) {
        const rect = btn.getBoundingClientRect();
        dropdownStyle = `top:${rect.bottom + 6}px;right:${window.innerWidth - rect.right}px;width:120px;`;
      }
    }
  }

  // Close dropdown on click outside, scroll, or resize
  $effect(() => {
    if (showFontDropdown) {
      const handler = (e: MouseEvent) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.dropdown-menu') && !target.closest('.setting-dropdown')) {
          showFontDropdown = false;
        }
      };
      const dismissOnScroll = () => { showFontDropdown = false; };
      const timer = setTimeout(() => document.addEventListener('click', handler), 0);
      window.addEventListener('scroll', dismissOnScroll, { capture: true, passive: true });
      window.addEventListener('resize', dismissOnScroll, { passive: true });
      return () => {
        clearTimeout(timer);
        document.removeEventListener('click', handler);
        window.removeEventListener('scroll', dismissOnScroll, { capture: true } as EventListenerOptions);
        window.removeEventListener('resize', dismissOnScroll);
      };
    }
  });

  function selectFontSize(size: string) {
    fontSize = size;
    showFontDropdown = false;
    success(`Font size set to ${size}`);
  }
</script>

<div class="panel-heading animate-fade-in-up stagger-1">
  <span class="panel-heading-title">Appearance</span>
  <span class="panel-heading-desc">How Janus looks on your screen</span>
</div>
<div class="settings-card-grid animate-fade-in-up stagger-1">
  <div class="settings-card">
    <div class="settings-card-icon"><Icon name="palette" size={18} color="var(--accent-primary)" /></div>
    <span class="settings-card-name">Theme</span>
    <span class="settings-card-desc">Choose your color scheme</span>
    <div class="theme-toggle theme-toggle-lg">
      <button
        class="theme-btn"
        class:active={theme === 'dark'}
        onclick={() => theme = 'dark'}
      >Dark</button>
      <button
        class="theme-btn"
        class:active={theme === 'light'}
        onclick={() => theme = 'light'}
      >Light</button>
      <button
        class="theme-btn"
        class:active={theme === 'system'}
        onclick={() => theme = 'system'}
      >System</button>
    </div>
  </div>

  <div class="settings-card">
    <div class="settings-card-icon"><Icon name="file-text" size={18} color="var(--accent-primary)" /></div>
    <span class="settings-card-name">Font Size</span>
    <span class="settings-card-desc">Adjust text size across the app</span>
    <div class="font-dropdown-wrapper">
      <button class="setting-dropdown setting-dropdown-lg" onclick={toggleFontDropdown}>
        <span>{fontSize}</span>
        <Icon name="chevron-down" size={13} color="var(--fg-muted)" />
      </button>
    </div>
  </div>
</div>

{#if showFontDropdown}
  <div class="dropdown-menu" style={dropdownStyle}>
    {#each fontSizes as size}
      <button class="dropdown-item" class:active={fontSize === size} onclick={() => selectFontSize(size)}>{size}</button>
    {/each}
  </div>
{/if}

<style>
  /* ── Appearance: side-by-side setting cards instead of one narrow list ── */
  .settings-card-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px; max-width: 900px;
  }
  .settings-card {
    display: flex; flex-direction: column; gap: 10px;
    padding: 22px; border-radius: 16px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  .settings-card:hover { border-color: color-mix(in srgb, var(--accent) 45%, transparent); box-shadow: 0 4px 24px -6px var(--accent); }
  .settings-card-icon {
    width: 36px; height: 36px; border-radius: 10px; display: flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .settings-card-name { font-size: var(--text-md); font-weight: 700; color: #e8e0ff; }
  .settings-card-desc { font-size: var(--text-sm); color: #5a5a7a; margin-top: -6px; }
  .theme-toggle-lg { margin-top: 4px; }
  .setting-dropdown-lg { width: 100%; justify-content: space-between; margin-top: 4px; }
</style>
