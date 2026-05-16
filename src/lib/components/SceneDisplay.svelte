<script lang="ts">
  import { browser } from '$app/environment';
  import Icon from './Icon.svelte';
  import { activeConversationId } from '$lib/stores/chat';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;

  let activeTab: 'image' | 'video' = $state('image');
  let isLoading = $state(false);
  let sceneCaption = $state('No scene generated yet');
  let sceneImageUrl: string | null = $state(null);
  let currentSceneId: string | null = $state(null);
  let promptText = $state('');
  let isEditingPrompt = $state(false);

  // Load existing scenes when conversation changes
  $effect(() => {
    const convId = $activeConversationId;
    if (convId && isTauri) {
      loadLatestScene(convId);
    } else {
      sceneImageUrl = null;
      sceneCaption = 'No scene generated yet';
      currentSceneId = null;
    }
  });

  async function loadLatestScene(convId: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const scenes = await ipc.listScenes(convId);
      if (scenes.length > 0) {
        const latest = scenes[0]; // Already sorted DESC by created_at
        currentSceneId = latest.id;
        sceneCaption = latest.caption || latest.prompt;
        promptText = latest.prompt;

        // Resolve the file URL
        const { convertFileSrc } = await import('@tauri-apps/api/core');
        const absPath = await ipc.getScenePath(latest.file_path);
        sceneImageUrl = convertFileSrc(absPath);
      } else {
        sceneImageUrl = null;
        sceneCaption = 'No scene generated yet';
        currentSceneId = null;
      }
    } catch (err) {
      console.error('Failed to load scenes:', err);
    }
  }

  async function handleGenerate() {
    const convId = $activeConversationId;
    if (!convId || isLoading) return;

    const prompt = promptText.trim() || 'A detailed scene from the current conversation';
    isLoading = true;
    isEditingPrompt = false;

    if (!isTauri) {
      // Dev mode: simulate generation
      setTimeout(() => {
        sceneCaption = `${prompt} — generated (mock)`;
        isLoading = false;
      }, 2000);
      return;
    }

    try {
      const ipc = await import('$lib/services/ipc');
      const scene = await ipc.generateScene(convId, prompt, {
        width: 512,
        height: 512,
      });

      currentSceneId = scene.id;
      sceneCaption = scene.caption || scene.prompt;
      promptText = scene.prompt;

      // Load the generated image
      const { convertFileSrc } = await import('@tauri-apps/api/core');
      const absPath = await ipc.getScenePath(scene.file_path);
      sceneImageUrl = convertFileSrc(absPath);
    } catch (err) {
      console.error('Failed to generate scene:', err);
      sceneCaption = 'Generation failed — check your image provider settings';
    }
    isLoading = false;
  }

  async function handleRegenerate() {
    await handleGenerate();
  }

  async function handleSave() {
    if (!sceneImageUrl) return;
    // Open the image in a new window / save dialog
    if (isTauri && currentSceneId) {
      try {
        const { save } = await import('@tauri-apps/plugin-dialog');
        const ipc = await import('$lib/services/ipc');
        const scenes = await ipc.listScenes($activeConversationId);
        const scene = scenes.find(s => s.id === currentSceneId);
        if (scene) {
          const absPath = await ipc.getScenePath(scene.file_path);
          // Copy to user-selected path
          const dest = await save({
            defaultPath: `scene-${currentSceneId}.png`,
            filters: [{ name: 'PNG Image', extensions: ['png'] }],
          });
          if (dest) {
            const { copyFile } = await import('@tauri-apps/plugin-fs');
            await copyFile(absPath, dest);
          }
        }
      } catch (err) {
        console.error('Failed to save scene:', err);
      }
    }
  }

  function handleEditPrompt() {
    isEditingPrompt = !isEditingPrompt;
  }
</script>

<div class="scene-display">
  <!-- Header with toggle -->
  <div class="scene-header">
    <span class="scene-title">SCENE</span>
    <div class="scene-toggle" role="tablist" aria-label="Scene media type">
      <button 
        class="toggle-btn" 
        class:active={activeTab === 'image'}
        onclick={() => activeTab = 'image'}
        role="tab"
        aria-selected={activeTab === 'image'}
        aria-controls="scene-panel"
      >
        <Icon name="image" size={12} color={activeTab === 'image' ? '#FFFFFF' : 'var(--fg-muted)'} />
        <span>Image</span>
      </button>
      <button 
        class="toggle-btn" 
        class:active={activeTab === 'video'}
        onclick={() => activeTab = 'video'}
        role="tab"
        aria-selected={activeTab === 'video'}
        aria-controls="scene-panel"
      >
        <Icon name="video" size={12} color={activeTab === 'video' ? '#FFFFFF' : 'var(--fg-muted)'} />
        <span>Video</span>
      </button>
    </div>
  </div>

  <!-- Scene Frame -->
  <div class="scene-frame" class:loading={isLoading} id="scene-panel" role="tabpanel">
    {#if isLoading}
      <div class="scene-loading animate-shimmer">
        <Icon name="image" size={24} color="var(--fg-muted)" />
        <span class="loading-text">Generating...</span>
      </div>
    {:else if activeTab === 'image'}
      <div class="scene-image">
        {#if sceneImageUrl}
          <img src={sceneImageUrl} alt={sceneCaption} class="generated-image" />
        {:else}
          <div class="scene-placeholder" onclick={handleGenerate} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleGenerate()}>
            <div class="scene-overlay">
              <Icon name="sparkles" size={20} color="rgba(255,255,255,0.6)" />
              <span class="gen-hint">Click to generate</span>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="scene-video">
        <div class="video-placeholder">
          <Icon name="video" size={32} color="var(--fg-muted)" />
          <span class="video-text">Video generation ready</span>
          <button class="generate-video-btn">
            <Icon name="sparkles" size={14} color="#FFFFFF" />
            Generate Scene Video
          </button>
        </div>
      </div>
    {/if}
  </div>

  <!-- Prompt Editor -->
  {#if isEditingPrompt}
    <div class="prompt-editor">
      <textarea
        class="prompt-input"
        bind:value={promptText}
        placeholder="Describe the scene to generate..."
        rows="2"
      ></textarea>
      <button class="prompt-go-btn" onclick={handleGenerate} disabled={isLoading}>
        <Icon name="sparkles" size={12} color="#FFFFFF" />
        Generate
      </button>
    </div>
  {/if}

  <!-- Caption -->
  <div class="scene-caption">
    <span class="caption-text">{sceneCaption}</span>
  </div>

  <!-- Actions -->
  <div class="scene-actions">
    <button class="scene-action-btn" onclick={handleRegenerate} aria-label="Regenerate scene" disabled={isLoading}>
      <Icon name="refresh-cw" size={10} color="var(--fg-muted)" />
      <span>Regenerate</span>
    </button>
    <button class="scene-action-btn" onclick={handleSave} aria-label="Save scene" disabled={!sceneImageUrl}>
      <Icon name="download" size={10} color="var(--fg-muted)" />
      <span>Save</span>
    </button>
    <button class="scene-action-btn" onclick={handleEditPrompt} aria-label="Edit scene prompt" class:active={isEditingPrompt}>
      <Icon name="pencil" size={10} color={isEditingPrompt ? 'var(--accent-primary)' : 'var(--fg-muted)'} />
      <span>Edit Prompt</span>
    </button>
  </div>
</div>

<style>
  .scene-display {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .scene-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .scene-title {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--fg-muted);
    font-family: var(--font-mono);
    letter-spacing: 1px;
  }

  /* Toggle */
  .scene-toggle {
    display: flex;
    border-radius: var(--rounded-md);
    border: 1px solid var(--border-subtle);
    overflow: hidden;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    background: transparent;
    border: none;
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-family: var(--font-body);
    transition: all var(--duration-fast) var(--ease-out);
  }

  .toggle-btn.active {
    background: var(--accent-primary);
    color: #FFFFFF;
    font-weight: 600;
  }

  .toggle-btn:first-child {
    border-radius: 7px 0 0 7px;
  }

  .toggle-btn:last-child {
    border-radius: 0 7px 7px 0;
  }

  /* Scene Frame */
  .scene-frame {
    width: 100%;
    height: 180px;
    border-radius: var(--rounded-md);
    background: var(--surface-card);
    border: 1px solid var(--border-subtle);
    overflow: hidden;
    position: relative;
  }

  .scene-image, .scene-video {
    width: 100%;
    height: 100%;
  }

  .generated-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .scene-placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(
      135deg,
      #1a0a2e 0%,
      #2d1b69 30%,
      #8B5CF620 60%,
      #1a1a2e 100%
    );
    position: relative;
    cursor: pointer;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .scene-placeholder:hover {
    opacity: 0.85;
  }

  .scene-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .gen-hint {
    font-size: var(--text-xs);
    color: rgba(255, 255, 255, 0.5);
    font-family: var(--font-mono);
  }

  /* Loading */
  .scene-loading {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .loading-text {
    font-size: var(--text-sm);
    color: var(--fg-muted);
    font-family: var(--font-mono);
  }

  /* Video placeholder */
  .video-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: var(--surface-card);
  }

  .video-text {
    font-size: var(--text-sm);
    color: var(--fg-muted);
  }

  .generate-video-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--rounded-md);
    background: var(--accent-tertiary);
    border: none;
    color: #000;
    font-size: var(--text-sm);
    font-weight: 600;
    font-family: var(--font-body);
    margin-top: 4px;
    transition: all var(--duration-fast) var(--ease-out);
  }

  .generate-video-btn:hover {
    opacity: 0.9;
    transform: scale(1.02);
  }

  /* Prompt Editor */
  .prompt-editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .prompt-input {
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--rounded-sm);
    border: 1px solid var(--border-subtle);
    background: var(--surface-input);
    color: var(--fg-primary);
    font-size: var(--text-sm);
    font-family: var(--font-body);
    resize: vertical;
    min-height: 40px;
    outline: none;
    transition: border-color var(--duration-fast) var(--ease-out);
  }

  .prompt-input:focus {
    border-color: var(--accent-primary);
  }

  .prompt-input::placeholder {
    color: var(--fg-muted);
  }

  .prompt-go-btn {
    align-self: flex-end;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    border-radius: var(--rounded-sm);
    background: var(--accent-primary);
    border: none;
    color: #FFFFFF;
    font-size: var(--text-xs);
    font-weight: 600;
    font-family: var(--font-body);
    transition: all var(--duration-fast) var(--ease-out);
  }

  .prompt-go-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .prompt-go-btn:disabled {
    opacity: 0.5;
  }

  /* Caption */
  .scene-caption {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .caption-text {
    font-size: var(--text-xs);
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Actions */
  .scene-actions {
    display: flex;
    gap: 6px;
  }

  .scene-action-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-radius: var(--rounded-sm);
    border: 1px solid var(--border-subtle);
    background: transparent;
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-family: var(--font-body);
    transition: all var(--duration-fast) var(--ease-out);
  }

  .scene-action-btn:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--fg-secondary);
  }

  .scene-action-btn:disabled {
    opacity: 0.4;
  }

  .scene-action-btn.active {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }
</style>
