<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    characterName, modelName, avatarUrl = null, showContextPanel = true,
    onTogglePanel, onGenerateScene,
  }: {
    characterName: string; modelName: string; avatarUrl?: string | null;
    showContextPanel?: boolean; onTogglePanel: () => void;
    onGenerateScene?: () => void;
  } = $props();
</script>

<header class="ch">
  <div class="ch-left">
    <div class="ch-ava" aria-hidden="true">
      {#if avatarUrl}<img src={avatarUrl} alt={characterName} class="ch-ava-img" />{/if}
      <div class="ch-ava-ring"></div>
    </div>
    <div class="ch-info">
      <span class="ch-name">{characterName}</span>
      <div class="ch-status">
        <span class="ch-dot"></span>
        <span class="ch-model">Using {modelName}</span>
      </div>
    </div>
  </div>
  <div class="ch-right" role="toolbar" aria-label="Chat tools">
    <button class="ch-btn" title="Generate Scene" aria-label="Generate scene image"
      onclick={onGenerateScene}>
      <Icon name="image" size={15} color="#6b6b8a" />
    </button>
    <button class="ch-btn" title="Branch" aria-label="Branch conversation">
      <Icon name="git-branch" size={15} color="#6b6b8a" />
    </button>
    <button class="ch-btn" class:active={showContextPanel} title="Context Panel"
      aria-label="Toggle context panel" aria-pressed={showContextPanel} onclick={onTogglePanel}>
      <Icon name="settings" size={15} color={showContextPanel ? '#c4a1ff' : '#6b6b8a'} />
    </button>
  </div>
</header>

<style>
  .ch {
    display: flex; align-items: center; justify-content: space-between;
    height: 60px; padding: 0 24px; flex-shrink: 0;
    background: linear-gradient(180deg, rgba(12,12,30,0.95), rgba(9,9,26,0.9));
    border-bottom: 1px solid rgba(139,92,246,0.08);
    backdrop-filter: blur(12px);
    position: relative;
  }
  .ch::after {
    content: ''; position: absolute; bottom: 0; left: 24px; right: 24px; height: 1px;
    background: linear-gradient(90deg, transparent, rgba(139,92,246,0.15), transparent);
  }

  .ch-left { display: flex; align-items: center; gap: 14px; }

  .ch-ava {
    width: 38px; height: 38px; min-width: 38px; min-height: 38px;
    border-radius: 50%; aspect-ratio: 1;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    flex-shrink: 0; overflow: hidden; position: relative;
  }
  .ch-ava-img { width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 50%; }
  .ch-ava-ring {
    position: absolute; inset: -3px; border-radius: 50%;
    border: 1.5px solid rgba(139,92,246,0.2); pointer-events: none;
  }

  .ch-info { display: flex; flex-direction: column; gap: 2px; }
  .ch-name { font-size: var(--text-lg); font-weight: 650; color: #e8e0ff; letter-spacing: -0.2px; }

  .ch-status { display: flex; align-items: center; gap: 5px; }
  .ch-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: #10B981; box-shadow: 0 0 6px rgba(16,185,129,0.4);
    animation: dotPulse 2.5s ease-in-out infinite;
  }
  @keyframes dotPulse { 0%,100% { opacity: 0.7; } 50% { opacity: 1; } }
  .ch-model { font-size: var(--text-sm); color: #5a5a7a; font-family: var(--font-mono); }

  .ch-right { display: flex; align-items: center; gap: 6px; }
  .ch-btn {
    width: 34px; height: 34px; border-radius: 10px;
    border: 1px solid rgba(139,92,246,0.08); background: transparent;
    display: flex; align-items: center; justify-content: center; cursor: pointer;
    transition: all 180ms var(--ease-out);
  }
  .ch-btn:hover {
    background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.15);
    transform: translateY(-1px);
  }
  .ch-btn.active {
    background: rgba(139,92,246,0.12); border-color: rgba(139,92,246,0.25);
    box-shadow: 0 0 12px rgba(139,92,246,0.15);
  }
</style>
