<script lang="ts">
  import Icon from './Icon.svelte';
  import SceneDisplay from './SceneDisplay.svelte';
  import ContextCarousel from './ContextCarousel.svelte';
  import ContextLorebook from './ContextLorebook.svelte';
  import ContextMemories from './ContextMemories.svelte';
  import ContextGroupCast from './ContextGroupCast.svelte';

  let {
    characterId = null,
    characterName,
    characterTagline,
    avatarUrl = null,
    tags = [],
    additionalCharacters = [],
    conversationId = null,
    onClose,
  }: {
    characterId?: string | null;
    characterName: string;
    characterTagline: string;
    avatarUrl?: string | null;
    tags?: { label: string; color: string }[];
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
    conversationId?: string | null;
    onClose: () => void;
  } = $props();

  let isMultiChar = $derived((additionalCharacters?.length ?? 0) > 0);
</script>

<aside class="context-panel animate-slide-in-right" aria-label="Character context">
  <!-- Header -->
  <div class="ctx-header">
    <span class="ctx-title" id="ctx-character-title">{isMultiChar ? 'CHARACTERS' : 'CHARACTER'}</span>
    <button class="ctx-close" onclick={onClose} aria-label="Close context panel">
      <Icon name="x" size={16} color="var(--fg-muted)" />
    </button>
  </div>

  <ContextCarousel {characterId} {characterName} {characterTagline} {avatarUrl} {tags} {additionalCharacters} />

  <div class="ctx-divider" role="separator"></div>

  <!-- Scene Display -->
  <SceneDisplay />

  <div class="ctx-divider" role="separator"></div>

  <ContextLorebook {characterId} />

  <div class="ctx-divider" role="separator"></div>

  <ContextMemories {characterId} {conversationId} />

  <div class="ctx-divider" role="separator"></div>

  <ContextGroupCast {characterId} {characterName} {conversationId} {additionalCharacters} />
</aside>

<style>
  .context-panel {
    width: var(--context-panel-width); height: 100%;
    background: linear-gradient(175deg, #0c0c1e, #09091a 50%, #07071a);
    border-left: 1px solid rgba(139,92,246,0.08);
    padding: 18px 16px; display: flex; flex-direction: column;
    gap: 16px; overflow-y: auto; flex-shrink: 0;
    animation: ctxSlideIn 350ms cubic-bezier(0.34,1.56,0.64,1) both;
  }
  @keyframes ctxSlideIn {
    from { opacity: 0; transform: translateX(20px); }
    to { opacity: 1; transform: translateX(0); }
  }
  .context-panel::-webkit-scrollbar { width: 3px; }
  .context-panel::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }

  .ctx-header { display: flex; justify-content: space-between; align-items: center; }
  .ctx-title {
    font-size: var(--text-xs); font-weight: 700; color: #5a5a7a;
    font-family: var(--font-mono); letter-spacing: 1.8px;
  }
  .ctx-close {
    background: none; border: none; padding: 6px; border-radius: 8px;
    cursor: pointer; transition: background 150ms;
  }
  .ctx-close:hover { background: rgba(139,92,246,0.08); }

  .ctx-divider {
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.12), transparent);
  }

  @media (max-width: 1024px) { .context-panel { display: none; } }
</style>
