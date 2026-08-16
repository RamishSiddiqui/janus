<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import { success } from '$lib/stores/toast';

  let streamingEnabled = $state($settings.streamingEnabled);
  let showThinking = $state($settings.showThinking);
  let autoGenerateImages = $state($settings.autoGenerateImages);
  let autoSaveMemories = $state($settings.autoSaveMemories);

  // Persist changes back to store (debounced to avoid infinite loop)
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snapshot = { streamingEnabled, showThinking, autoGenerateImages, autoSaveMemories };
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      settings.update(prev => ({ ...prev, ...snapshot }));
    }, 50);
  });
</script>

<div class="panel-heading animate-fade-in-up stagger-2">
  <span class="panel-heading-title">Chat Behavior</span>
  <span class="panel-heading-desc">How responses stream in and what runs automatically after each message</span>
</div>
<div class="settings-toggle-grid animate-fade-in-up stagger-2">
  <div class="toggle-card">
    <div class="setting-label">
      <span class="setting-name">Streaming Responses</span>
      <span class="setting-desc">{streamingEnabled ? 'Text appears word-by-word as it generates' : 'Full response appears at once when complete'}</span>
    </div>
    <button
      class="toggle-switch"
      class:on={streamingEnabled}
      onclick={() => {
        streamingEnabled = !streamingEnabled;
        success(streamingEnabled ? 'Streaming enabled — responses stream in real-time' : 'Streaming disabled — responses appear when complete');
      }}
      role="switch"
      aria-checked={streamingEnabled}
      aria-label="Toggle streaming responses"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>

  <div class="toggle-card">
    <div class="setting-label">
      <span class="setting-name">Show Model Thinking</span>
      <span class="setting-desc">{showThinking ? 'Reasoning models show a collapsible "Thinking" trace above their reply' : 'Reasoning is hidden entirely — only the in-character reply is shown'}</span>
    </div>
    <button
      class="toggle-switch"
      class:on={showThinking}
      onclick={() => {
        showThinking = !showThinking;
        success(showThinking ? 'Model thinking will be shown (collapsed by default)' : 'Model thinking hidden');
      }}
      role="switch"
      aria-checked={showThinking}
      aria-label="Toggle showing model reasoning/thinking"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>

  <div class="toggle-card">
    <div class="setting-label">
      <span class="setting-name">Auto-Generate Scene Images</span>
      <span class="setting-desc">Generate images from scene context</span>
    </div>
    <button
      class="toggle-switch"
      class:on={autoGenerateImages}
      onclick={() => autoGenerateImages = !autoGenerateImages}
      role="switch"
      aria-checked={autoGenerateImages}
      aria-label="Toggle auto-generate scene images"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>

  <div class="toggle-card">
    <div class="setting-label">
      <span class="setting-name">Auto-Save Memories</span>
      <span class="setting-desc">{autoSaveMemories ? 'Key events are extracted every few messages' : 'Memories are only saved when pinned manually'}</span>
    </div>
    <button
      class="toggle-switch"
      class:on={autoSaveMemories}
      onclick={() => {
        autoSaveMemories = !autoSaveMemories;
        success(autoSaveMemories ? 'Auto-save enabled — key events will be remembered' : 'Auto-save disabled');
      }}
      role="switch"
      aria-checked={autoSaveMemories}
      aria-label="Toggle auto-save memories"
    >
      <span class="toggle-knob"></span>
    </button>
  </div>
</div>
