<script lang="ts">
  import Icon from './Icon.svelte';
  import SceneDisplay from './SceneDisplay.svelte';
  import ContextCarousel from './ContextCarousel.svelte';
  import ContextMemories from './ContextMemories.svelte';

  let {
    characterId = null,
    characterName,
    characterTagline,
    avatarUrl = null,
    avatarPath = null,
    tags = [],
    additionalCharacters = [],
    conversationId = null,
    onClose,
  }: {
    characterId?: string | null;
    characterName: string;
    characterTagline: string;
    avatarUrl?: string | null;
    /** Raw relative avatar path (not the blob: URL) — needed as an img2img
     *  reference, which requires a real file path server-side. */
    avatarPath?: string | null;
    tags?: { label: string; color: string }[];
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
    conversationId?: string | null;
    onClose: () => void;
  } = $props();

  let isMultiChar = $derived((additionalCharacters?.length ?? 0) > 0);

  // ── Drag-to-resize ──────────────────────────────────────────────────
  const MIN_WIDTH = 280;
  const MAX_WIDTH = 640;
  const WIDTH_STORAGE_KEY = 'mythic:contextPanelWidth';

  function loadStoredWidth(): number {
    if (typeof localStorage === 'undefined') return 320;
    const stored = Number(localStorage.getItem(WIDTH_STORAGE_KEY));
    return stored >= MIN_WIDTH && stored <= MAX_WIDTH ? stored : 320;
  }

  let panelWidth = $state(loadStoredWidth());
  let isResizing = $state(false);
  let dragStartX = 0;
  let dragStartWidth = 0;

  function handleResizeStart(e: PointerEvent) {
    isResizing = true;
    dragStartX = e.clientX;
    dragStartWidth = panelWidth;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handleResizeMove(e: PointerEvent) {
    if (!isResizing) return;
    // The panel's right edge is pinned to the viewport edge, so dragging the
    // left-edge handle further left (negative clientX delta) should widen it.
    const next = dragStartWidth + (dragStartX - e.clientX);
    panelWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next));
  }

  function handleResizeEnd() {
    if (!isResizing) return;
    isResizing = false;
    localStorage.setItem(WIDTH_STORAGE_KEY, String(panelWidth));
  }

  function handleResizeKeydown(e: KeyboardEvent) {
    const step = 20;
    if (e.key === 'ArrowLeft') panelWidth = Math.min(MAX_WIDTH, panelWidth + step);
    else if (e.key === 'ArrowRight') panelWidth = Math.max(MIN_WIDTH, panelWidth - step);
    else return;
    e.preventDefault();
    localStorage.setItem(WIDTH_STORAGE_KEY, String(panelWidth));
  }
</script>

<aside
  class="context-panel animate-slide-in-right"
  class:resizing={isResizing}
  style="width: {panelWidth}px"
  aria-label="Character context"
>
  <!-- Drag handle -->
  <div
    class="ctx-resize-handle"
    onpointerdown={handleResizeStart}
    onpointermove={handleResizeMove}
    onpointerup={handleResizeEnd}
    onpointercancel={handleResizeEnd}
    onkeydown={handleResizeKeydown}
    role="slider"
    aria-orientation="vertical"
    aria-label="Resize context panel"
    aria-valuenow={panelWidth}
    aria-valuemin={MIN_WIDTH}
    aria-valuemax={MAX_WIDTH}
    tabindex="0"
  ></div>

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
  <SceneDisplay
    {characterId}
    {characterName}
    characterDescription={characterTagline}
    {avatarPath}
    {additionalCharacters}
  />

  <div class="ctx-divider" role="separator"></div>

  <ContextMemories {characterId} {conversationId} />
</aside>

<style>
  .context-panel {
    height: 100%; position: relative;
    background: linear-gradient(175deg, #0c0c1e, #09091a 50%, #07071a);
    border-left: 1px solid rgba(139,92,246,0.08);
    padding: 18px 16px; display: flex; flex-direction: column;
    gap: 16px; overflow-y: auto; flex-shrink: 0;
    animation: ctxSlideIn 350ms cubic-bezier(0.34,1.56,0.64,1) both;
    /* Exposes this panel's own width (not the viewport's) as `cqi` to every
       descendant, so text/buttons/spacing can scale with the drag handle
       instead of staying pinned to fixed px sizes as the panel widens. */
    container-type: inline-size;
    container-name: ctx-panel;
  }
  .context-panel.resizing { animation: none; user-select: none; }

  .ctx-resize-handle {
    position: absolute; top: 0; bottom: 0; left: -3px; width: 6px;
    cursor: col-resize; z-index: 5; touch-action: none;
  }
  .ctx-resize-handle::after {
    content: ''; position: absolute; top: 0; bottom: 0; left: 2px; width: 2px;
    background: rgba(139,92,246,0.15); transition: background 150ms;
  }
  .ctx-resize-handle:hover::after,
  .context-panel.resizing .ctx-resize-handle::after {
    background: rgba(139,92,246,0.5);
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
