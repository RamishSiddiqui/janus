<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    characterName, modelName, avatarUrl = null, showContextPanel = true,
    additionalCharacters = [],
    onTogglePanel, onGenerateScene,
  }: {
    characterName: string; modelName: string; avatarUrl?: string | null;
    showContextPanel?: boolean; onTogglePanel: () => void;
    onGenerateScene?: () => void;
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
  } = $props();

  let isMultiChar = $derived(additionalCharacters.length > 0);
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

    {#if isMultiChar}
      <div class="ch-collab">
        <div class="ch-collab-divider"></div>
        <div class="ch-collab-group">
          {#each additionalCharacters.slice(0, 3) as char}
            <div class="ch-collab-ava" style="background:{char.avatarColor}" title={char.name}>
              {#if char.avatarUrl}<img src={char.avatarUrl} alt={char.name} class="ch-collab-ava-img" />{/if}
            </div>
          {/each}
          <span class="ch-collab-label">
            {additionalCharacters.map(c => c.name).join(', ')}
          </span>
        </div>
      </div>
    {/if}
  </div>
  <div class="ch-right" role="toolbar" aria-label="Chat tools">
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

  /* ── Collaborator Pill ── */
  .ch-collab {
    display: flex; align-items: center; gap: 12px;
  }
  .ch-collab-divider {
    width: 1px; height: 28px;
    background: linear-gradient(180deg, transparent, rgba(0,242,255,0.2), transparent);
  }
  .ch-collab-group {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 12px 5px 6px;
    border-radius: 20px;
    background: rgba(0,242,255,0.06);
    border: 1px solid rgba(0,242,255,0.12);
    transition: all 200ms var(--ease-out);
  }
  .ch-collab-group:hover {
    background: rgba(0,242,255,0.1);
    border-color: rgba(0,242,255,0.25);
  }
  .ch-collab-ava {
    width: 24px; height: 24px; border-radius: 50%;
    overflow: hidden; flex-shrink: 0;
    border: 1.5px solid rgba(0,242,255,0.2);
    transition: transform 200ms var(--ease-spring);
  }
  .ch-collab-ava + .ch-collab-ava { margin-left: -8px; }
  .ch-collab-group:hover .ch-collab-ava { transform: scale(1.08); }
  .ch-collab-ava-img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ch-collab-label {
    font-size: 11px; font-weight: 600; color: #00d4e0;
    white-space: nowrap; letter-spacing: -0.1px;
    font-family: var(--font-body);
  }

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
