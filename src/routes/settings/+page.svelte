<script lang="ts">
  import Icon from '$lib/components/Icon.svelte';
  import SplitHeading from '$lib/components/SplitHeading.svelte';
  import SettingsAppearanceSection from '$lib/components/SettingsAppearanceSection.svelte';
  import SettingsChatSection from '$lib/components/SettingsChatSection.svelte';
  import SettingsContextSection from '$lib/components/SettingsContextSection.svelte';
  import SettingsPrivacySection from '$lib/components/SettingsPrivacySection.svelte';
  import SettingsImageSection from '$lib/components/SettingsImageSection.svelte';
  import SettingsPromptsSection from '$lib/components/SettingsPromptsSection.svelte';
  import SettingsLoggingSection from '$lib/components/SettingsLoggingSection.svelte';
  import { settings } from '$lib/stores/settings';

  // ── Sidebar navigation ──
  // Settings grew to 8 sections crammed into a two-column masonry layout
  // that kept getting more cluttered as features were added (Image Presets'
  // quality knobs, the reasoning toggle, etc). A single active-section panel
  // with sidebar nav (the VS Code / Linear / macOS System Settings pattern)
  // scales to any number of sections without the page just getting taller.
  type SettingsSection = 'appearance' | 'chat' | 'context' | 'privacy' | 'image' | 'prompts' | 'logging';
  let activeSection = $state<SettingsSection>('appearance');
  const NAV_ITEMS: { id: SettingsSection; label: string; icon: string; accent: string }[] = [
    { id: 'appearance', label: 'Appearance', icon: 'palette', accent: '#9075f2' },
    { id: 'chat', label: 'Chat Behavior', icon: 'message-circle', accent: '#22d3ee' },
    { id: 'context', label: 'Context & Memory', icon: 'network', accent: '#e879f9' },
    { id: 'image', label: 'Image Generation', icon: 'image', accent: '#fbbf24' },
    { id: 'prompts', label: 'Prompts', icon: 'file-text', accent: '#34d399' },
    { id: 'privacy', label: 'Data & Privacy', icon: 'shield', accent: '#fb7185' },
    { id: 'logging', label: 'Logging', icon: 'terminal', accent: '#94a3b8' },
  ];
  // Each section carries its own accent — driven into the panel below as a
  // CSS custom property, so every glass card/button/progress-bar re-tints
  // to match without hand-coding a per-section colour on each one.
  let sectionAccent = $derived(NAV_ITEMS.find(i => i.id === activeSection)?.accent ?? '#9075f2');

  // Each section component owns its own settings slice end-to-end (reads
  // $settings once at mount, persists its own changes back). A full
  // `settings.set(...)` from outside that flow — currently only the Privacy
  // section's Import — leaves every OTHER section's already-mounted local
  // state stale, since they don't reactively watch the store. Bumping this
  // (via the Privacy section's `onImported` callback) forces every section
  // to remount and re-read fresh from the now-updated store, instead of
  // reaching into their locals directly the way the old monolithic page did.
  let importGeneration = $state(0);
</script>

<svelte:head>
  <title>Settings — Janus</title>
</svelte:head>

<div class="settings-page" style="--accent: {sectionAccent}">
  <!-- Header -->
  <header class="settings-header">
    <div class="settings-header-left">
      <h1 class="settings-title"><SplitHeading text="Settings" /></h1>
      <span class="settings-subtitle">Customize your Janus experience</span>
    </div>
    <div class="settings-header-about">
      <span class="about-name">Janus v0.1.0</span>
      <span class="about-dot" aria-hidden="true">·</span>
      <span class="about-desc">{$settings.localStorageOnly ? '🔒 Private' : '⚠️ Privacy Relaxed'}</span>
      <button class="about-link-btn" title="GitHub">
        <Icon name="github" size={14} color="var(--fg-secondary)" />
      </button>
      <button class="about-link-btn" title="Star on GitHub">
        <Icon name="star" size={14} color="var(--fg-secondary)" />
      </button>
    </div>
  </header>

  <!-- Section nav — a floating carousel of chips instead of a second
       sidebar (the app's own nav rail is already the one persistent rail).
       Each chip carries its own accent; the active one steps forward and
       lights up while the rest recede, and that same accent drives the
       glass panel below via --accent. -->
  <div class="settings-carousel" role="tablist" aria-label="Settings sections">
    {#each NAV_ITEMS as item (item.id)}
      <button
        class="carousel-chip"
        class:active={activeSection === item.id}
        style="--chip-accent: {item.accent}"
        onclick={() => activeSection = item.id}
        role="tab"
        aria-selected={activeSection === item.id}
      >
        <Icon name={item.icon} size={13} color={activeSection === item.id ? '#0a0812' : 'var(--fg-muted)'} />
        <span>{item.label}</span>
      </button>
    {/each}
  </div>

  <div class="settings-body">
    {#key activeSection}
    {#key importGeneration}
    <div class="settings-panel">
    <div class="panel-glow" aria-hidden="true"></div>
    {#if activeSection === 'appearance'}
      <SettingsAppearanceSection />
    {/if}

    {#if activeSection === 'chat'}
      <SettingsChatSection />
    {/if}

    {#if activeSection === 'context'}
      <SettingsContextSection />
    {/if}

    {#if activeSection === 'privacy'}
      <SettingsPrivacySection onImported={() => importGeneration++} />
    {/if}

    {#if activeSection === 'image'}
      <SettingsImageSection />
    {/if}

    {#if activeSection === 'prompts'}
      <SettingsPromptsSection />
    {/if}

    {#if activeSection === 'logging'}
      <SettingsLoggingSection />
    {/if}
    </div>
    {/key}
    {/key}
  </div>
</div>

<style>
  .settings-page {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
  }

  /* ── Header ── */
  .settings-header {
    display: flex; align-items: flex-end; justify-content: space-between; gap: 16px;
    padding: 28px 36px 20px; flex-shrink: 0; position: relative;
  }
  .settings-header::after {
    content: ''; position: absolute; bottom: 0; left: 36px; right: 36px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }
  .settings-header-left { display: flex; flex-direction: column; gap: 4px; }
  .settings-title {
    font-size: 30px; font-weight: 600; letter-spacing: -0.6px;
  }
  .settings-subtitle { font-size: var(--text-lg); color: #5a5a7a; letter-spacing: 0.3px; }

  .settings-header-about {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding-bottom: 4px;
  }
  .settings-header-about .about-name { font-size: var(--text-sm); font-weight: 700; color: #8b8ba7; }
  .settings-header-about .about-dot { color: #3a3a52; }
  .settings-header-about .about-desc { font-size: 11px; color: #4a4a6a; font-family: var(--font-mono); letter-spacing: 0.3px; }

  /* ── Section nav: floating carousel, not a second sidebar ── */
  .settings-carousel {
    display: flex; align-items: center; justify-content: center; gap: 8px; flex-wrap: wrap;
    padding: 18px 36px; flex-shrink: 0;
    border-bottom: 1px solid rgba(255,255,255,0.05);
  }
  .carousel-chip {
    display: flex; align-items: center; gap: 7px;
    padding: 9px 16px; border-radius: 999px;
    background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.09);
    color: #a8a3c0; font-size: 12.5px; font-weight: 600;
    font-family: var(--font-body); cursor: pointer;
    transition: all 220ms cubic-bezier(0.16,1,0.3,1);
  }
  .carousel-chip:hover { background: rgba(255,255,255,0.08); border-color: rgba(255,255,255,0.16); color: #e8e5f5; }
  .carousel-chip.active {
    transform: scale(1.06); color: #0a0812; font-weight: 700;
    background: var(--chip-accent);
    border-color: var(--chip-accent);
    box-shadow: 0 8px 26px -8px var(--chip-accent);
  }

  .settings-body { display: flex; flex: 1; overflow: hidden; min-height: 0; }

  .settings-panel {
    position: relative;
    flex: 1; overflow-y: auto; min-width: 0;
    padding: 32px 36px 48px; display: flex; flex-direction: column; gap: 22px;
  }
  .settings-panel::-webkit-scrollbar { width: 4px; }
  .settings-panel::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }

  /* Ambient light wash behind the panel content, tinted to the active
     section's accent — the "light through a prism" effect from the
     approved concept, applied for real instead of a flat backdrop. */
  .panel-glow {
    position: absolute; top: -60px; left: 50%; transform: translateX(-50%);
    width: 520px; height: 360px; border-radius: 50%;
    background: radial-gradient(circle, var(--accent), transparent 70%);
    filter: blur(120px); opacity: 0.16; pointer-events: none; z-index: 0;
    transition: background 400ms ease;
  }
  .settings-panel > :not(.panel-glow) { position: relative; z-index: 1; }

  /* ── Nav footer links (About, moved into the sidebar) ── */
  .about-link-btn {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: all 150ms;
  }
  .about-link-btn:hover { background: rgba(139,92,246,0.06); }

  /* ── Responsive ── */
  @media (max-width: 768px) {
    .settings-header { flex-direction: column; align-items: flex-start; }
    .settings-header-about { padding-bottom: 0; }
    .settings-carousel { padding: 10px 16px; overflow-x: auto; flex-wrap: nowrap; justify-content: flex-start; }
    .carousel-chip span { display: none; }
    .settings-panel { padding: 20px 16px 40px; }
  }

  /* ── Shared "settings UI kit" — every Settings*Section.svelte renders
       markup using these classes, so they're declared :global() here
       (Svelte's per-component style scoping doesn't reach into child-
       rendered markup otherwise). Kept in the orchestrator rather than
       duplicated across all 7 sections since they're true cross-cutting
       primitives, not section-specific content. ── */

  /* Panel heading (section title + description, every section's first element) */
  :global(.panel-heading) { display: flex; flex-direction: column; gap: 4px; }
  :global(.panel-heading-title) { font-size: 20px; font-weight: 800; color: #e8e0ff; letter-spacing: -0.2px; }
  :global(.panel-heading-desc) { font-size: var(--text-sm); color: #6b6b8a; max-width: 640px; line-height: 1.5; }

  /* Sections whose content is a short action/toggle list read better at a
     comfortable measure than stretched edge-to-edge on a wide window. */
  :global(.settings-section-bounded) { max-width: 640px; }

  /* Toggle-row card grid — Chat and Image sections */
  :global(.settings-toggle-grid) {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 14px;
  }
  :global(.toggle-card) {
    display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 18px 20px; border-radius: 14px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  :global(.toggle-card:hover) { border-color: color-mix(in srgb, var(--accent) 45%, transparent); box-shadow: 0 4px 24px -6px var(--accent); }

  /* Section Card */
  :global(.settings-section) {
    padding: 20px; border-radius: 16px;
    background: rgba(255,255,255,0.04); backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.08);
    display: flex; flex-direction: column; gap: 16px;
    transition: border-color 200ms, box-shadow 250ms, background 200ms;
  }
  :global(.settings-section:hover) {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
    box-shadow: 0 4px 24px -6px var(--accent);
  }

  :global(.section-header) { display: flex; align-items: center; gap: 10px; }
  :global(.section-header-left) { display: flex; align-items: center; gap: 10px; flex: 1; }
  :global(.section-title) { font-size: var(--text-lg); font-weight: 700; color: #e8e0ff; }

  /* Setting Row */
  :global(.setting-row) {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 0;
  }
  :global(.setting-label) { display: flex; flex-direction: column; gap: 2px; }
  :global(.setting-name) { font-size: var(--text-md); color: #c8c8e0; font-weight: 500; }
  :global(.setting-desc) { font-size: var(--text-sm); color: #5a5a7a; }

  /* Theme Toggle — Appearance section */
  /* Floating pill thumb inset within a padded pill track (macOS/iOS segmented-
     control pattern) — both fully rounded, so there's no radius mismatch
     between the track and the active segment like a flush 10px-radius track
     with a square-cornered active button produced. */
  :global(.theme-toggle) {
    display: flex; gap: 2px; border-radius: 999px; padding: 3px;
    border: 1px solid rgba(139,92,246,0.1);
    background: rgba(9,9,26,0.6);
  }
  :global(.theme-btn) {
    flex: 1; padding: 6px 14px; background: transparent; border: none; border-radius: 999px;
    color: #5a5a7a; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); cursor: pointer; text-align: center;
    transition: all 200ms ease;
  }
  :global(.theme-btn:hover) { color: #8b8ba7; }
  :global(.theme-btn.active) {
    background: var(--accent);
    color: #0a0812;
    box-shadow: 0 2px 12px -2px var(--accent);
  }

  /* Dropdown — Appearance's font picker, Context's window-size/top-K/similarity selects */
  :global(.font-dropdown-wrapper) { position: relative; }
  :global(.setting-dropdown) {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    width: 120px; height: 34px; padding: 0 12px; border-radius: 10px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    font-size: 12px; font-weight: 600; font-family: var(--font-body);
    color: #e0e0f0; cursor: pointer; transition: border-color 200ms;
  }
  :global(.setting-dropdown:hover) { border-color: rgba(139,92,246,0.25); }
  :global(.dropdown-menu) {
    position: fixed; z-index: 50;
    background: linear-gradient(175deg, #12122a, #0a0a1a);
    border: 1px solid rgba(139,92,246,0.12); border-radius: 12px;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5); padding: 4px;
    display: flex; flex-direction: column;
  }
  :global(.dropdown-item) {
    padding: 7px 12px; border-radius: 8px; border: none; background: transparent;
    color: #8b8ba7; font-size: var(--text-sm); font-weight: 500;
    font-family: var(--font-body); text-align: left; cursor: pointer;
    transition: all 120ms;
  }
  :global(.dropdown-item:hover) { background: rgba(139,92,246,0.06); color: #e0e0f0; }
  :global(.dropdown-item.active) { color: var(--accent); font-weight: 700; }

  /* Clear/danger confirm inline panel — Privacy section */
  :global(.clear-confirm) {
    display: flex; flex-direction: column; gap: 10px; padding: 12px;
    border-radius: 12px; background: rgba(244,63,94,0.04);
    border: 1px solid rgba(244,63,94,0.15);
  }
  :global(.clear-warn) { font-size: var(--text-sm); color: #F43F5E; line-height: 1.5; }

  /* Toggle Switch — used by every section */
  :global(.toggle-switch) {
    width: 44px; height: 24px; border-radius: 99px;
    background: #2a2a4a; border: none; padding: 3px;
    display: flex; align-items: center; cursor: pointer;
    transition: background 250ms ease; flex-shrink: 0;
  }
  :global(.toggle-switch.on) {
    background: var(--accent);
    justify-content: flex-end;
    box-shadow: 0 0 12px -2px var(--accent);
  }
  :global(.toggle-knob) {
    width: 18px; height: 18px; border-radius: 50%; background: #fff;
    transition: transform 250ms cubic-bezier(0.34,1.56,0.64,1);
    box-shadow: 0 1px 4px rgba(0,0,0,0.3);
  }

  /* Buttons — used by every section */
  :global(.button-row) { display: flex; gap: 10px; }
  :global(.backup-status) {
    display: block; margin-top: 8px;
    font-size: var(--text-xs); color: var(--fg-muted);
    font-family: var(--font-mono);
  }
  :global(.settings-btn) {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 9px 16px; border-radius: 10px; font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); border: none; cursor: pointer;
    transition: all 180ms ease;
  }
  :global(.settings-btn.outline) {
    flex: 1; background: transparent;
    border: 1px solid rgba(139,92,246,0.12); color: #8b8ba7;
  }
  :global(.settings-btn.outline:hover) { background: rgba(139,92,246,0.06); border-color: rgba(139,92,246,0.2); }
  :global(.settings-btn.danger) {
    background: rgba(244,63,94,0.06); border: 1px solid rgba(244,63,94,0.15);
    color: #F43F5E; width: 100%;
  }
  :global(.settings-btn.danger:hover) { background: rgba(244,63,94,0.12); }
  :global(.settings-btn.primary) {
    background: var(--accent); color: #0a0812;
    box-shadow: 0 2px 12px -2px var(--accent); flex: 1;
  }
  :global(.settings-btn.primary:hover) { transform: translateY(-1px); box-shadow: 0 4px 20px -4px var(--accent); }
  :global(.settings-btn.primary:disabled) { opacity: 0.5; pointer-events: none; }
  :global(.settings-btn.sm) { padding: 6px 12px; font-size: var(--text-xs); flex: none; }

  /* System Prompt textarea + reset link — Prompts and Logging sections */
  :global(.system-prompt-input) {
    width: 100%; min-height: 140px; padding: 14px 16px; border-radius: 12px;
    background: rgba(14,14,30,0.6); border: 1px solid rgba(139,92,246,0.08);
    color: #c8c8e0; font-size: 12px; font-family: var(--font-body);
    line-height: 1.7; resize: vertical; outline: none;
    transition: border-color 200ms;
  }
  :global(.system-prompt-input:focus) { border-color: rgba(139,92,246,0.3); }
  :global(.prompt-hint) { font-size: 10px; color: #4a4a6a; font-family: var(--font-mono); }
  :global(.reset-btn) {
    background: none; border: none; cursor: pointer;
    color: var(--accent); font-size: var(--text-sm); font-weight: 600;
    font-family: var(--font-body); transition: opacity 150ms;
  }
  :global(.reset-btn:hover) { opacity: 0.7; }
  :global(.phi-description) {
    font-size: var(--text-sm); color: #5a5a7a; line-height: 1.6;
  }

  /* Staggered entrance — every section's panel-heading/grid uses these */
  :global(.animate-fade-in-up) { animation: fadeInUp 400ms ease both; }
  :global(.stagger-1) { animation-delay: 40ms; }
  :global(.stagger-2) { animation-delay: 100ms; }
  :global(.stagger-2b) { animation-delay: 140ms; }
  :global(.stagger-2c) { animation-delay: 160ms; }
  :global(.stagger-3) { animation-delay: 180ms; }
  :global(.stagger-3b) { animation-delay: 210ms; }
  :global(.stagger-4) { animation-delay: 240ms; }
  :global(.stagger-4b) { animation-delay: 280ms; }
  @keyframes fadeInUp {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }
  /* Context section's Memory config reveal */
  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-12px); }
    to   { opacity: 1; transform: translateY(0); }
  }
</style>
