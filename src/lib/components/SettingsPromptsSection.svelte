<script lang="ts">
  import Icon from './Icon.svelte';
  import { settings } from '$lib/stores/settings';
  import { success } from '$lib/stores/toast';

  let systemPrompt = $state($settings.systemPrompt);
  let postHistoryInstructions = $state($settings.postHistoryInstructions);
  let profileRefreshPrompt = $state($settings.profileRefreshPrompt);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { systemPrompt, postHistoryInstructions, profileRefreshPrompt };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });

  // Matches the original behavior exactly: this resets the ENTIRE settings
  // store to defaults (not just prompts) — a pre-existing quirk, not
  // something introduced by this split. Other tabs naturally pick up the
  // reset the next time they're navigated to, since each section re-reads
  // from $settings on mount.
  function resetSystemPrompt() {
    settings.reset();
    systemPrompt = $settings.systemPrompt;
    postHistoryInstructions = $settings.postHistoryInstructions;
    profileRefreshPrompt = $settings.profileRefreshPrompt;
    success('Prompts reset to defaults');
  }
</script>

<div class="panel-heading animate-fade-in-up stagger-4">
  <span class="panel-heading-title">Prompts</span>
  <span class="panel-heading-desc">The system-level instructions injected into every generation</span>
</div>
<section class="settings-section animate-fade-in-up stagger-4">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="file-text" size={16} color="var(--accent-primary)" />
      <span class="section-title">Default System Prompt</span>
    </div>
    <button class="reset-btn" onclick={resetSystemPrompt}>Reset</button>
  </div>

  <textarea
    class="system-prompt-input"
    bind:value={systemPrompt}
    rows="6"
    aria-label="Default system prompt"
  ></textarea>

  <span class="prompt-hint">Use {`{{char}}`} for character name • {`{{user}}`} for player name</span>
</section>

<!-- Post-History Instructions (PHI) -->
<section class="settings-section animate-fade-in-up stagger-4b">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="compass" size={16} color="var(--accent-primary)" />
      <span class="section-title">Narrative Direction</span>
    </div>
    <button class="reset-btn" onclick={() => { settings.reset(); postHistoryInstructions = $settings.postHistoryInstructions; success('Narrative direction reset'); }}>Reset</button>
  </div>

  <span class="phi-description">Injected after conversation history to shape how the AI structures responses — narrative hooks, scene transitions, and pacing.</span>

  <textarea
    class="system-prompt-input"
    bind:value={postHistoryInstructions}
    rows="6"
    aria-label="Post-history instructions"
  ></textarea>

  <span class="prompt-hint">Controls story momentum • scene transitions • prevents dead-end conversations</span>
</section>

<!-- Character Profile Refresh -->
<section class="settings-section animate-fade-in-up stagger-4b">
  <div class="section-header">
    <div class="section-header-left">
      <Icon name="refresh-cw" size={16} color="var(--accent-primary)" />
      <span class="section-title">Character Profile Refresh</span>
    </div>
    <button class="reset-btn" onclick={resetSystemPrompt}>Reset</button>
  </div>

  <span class="phi-description">Used by "Refresh from Story" — updates an auto-detected character's description, personality, and scenario to match how they've actually appeared, instead of leaving them stuck on the placeholder written when they were first spotted.</span>

  <textarea
    class="system-prompt-input"
    bind:value={profileRefreshPrompt}
    rows="6"
    aria-label="Character profile refresh prompt"
  ></textarea>

  <span class="prompt-hint">Must keep asking for JSON with description/personality/scenario only — editing that part away will break refreshes</span>
</section>
