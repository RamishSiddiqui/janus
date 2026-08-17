<script lang="ts">
  import { toasts, pauseToast, resumeToast, dismissToast } from '$lib/stores/toast';
  import Icon from './Icon.svelte';
</script>

{#if $toasts.length > 0}
  <div class="toast-container" aria-live="polite">
    {#each $toasts as toast (toast.id)}
      <div
        class="toast toast-{toast.type}"
        role="alert"
        onmouseenter={() => pauseToast(toast.id)}
        onmouseleave={() => resumeToast(toast.id)}
      >
        <div class="toast-icon-wrap">
          <Icon
            name={toast.type === 'success' ? 'check' : toast.type === 'error' ? 'alert-circle' : 'info'}
            size={14}
            color={toast.type === 'success' ? 'var(--success)' : toast.type === 'error' ? 'var(--danger)' : 'var(--accent-primary)'}
          />
        </div>
        <span class="toast-msg">{toast.message}</span>
        {#if toast.action}
          <button class="toast-action" onclick={toast.action.onClick}>{toast.action.label}</button>
        {/if}
        <button class="toast-close" onclick={() => dismissToast(toast.id)} aria-label="Dismiss">
          <Icon name="x" size={13} color="var(--fg-muted)" />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 500;
    display: flex;
    flex-direction: column;
    gap: 10px;
    pointer-events: none;
  }

  /* "Liquid Obsidian Glass" — same recipe as ChatMessage.svelte's .ai-bubble
     (frosted blur, inset sheen, luminous gradient accent line instead of a
     flat solid border) so toasts read as the same surface material as the
     rest of the app instead of a generic flat notification box. */
  .toast {
    position: relative;
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 13px 14px 13px 20px;
    border-radius: 16px;
    background: rgba(11, 11, 28, 0.78);
    backdrop-filter: blur(18px) saturate(160%);
    border: 1px solid rgba(255, 255, 255, 0.05);
    --toast-accent: var(--accent-primary);
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.04) inset,
      0 -1px 0 rgba(0, 0, 0, 0.2) inset,
      0 16px 40px rgba(0, 0, 0, 0.5),
      0 4px 12px rgba(0, 0, 0, 0.3),
      0 0 24px color-mix(in srgb, var(--toast-accent) 12%, transparent);
    font-size: var(--text-md);
    font-weight: 500;
    color: #ece8fb;
    pointer-events: auto;
    animation: toastSlideIn 350ms cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
    min-width: 240px;
    max-width: 460px;
    overflow: hidden;
    transition: box-shadow 300ms ease;
  }

  /* Luminous accent line — fades in/out top and bottom rather than a flat
     bar, matching the ai-bubble/multi-char accent line elsewhere. */
  .toast::before {
    content: '';
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 2.5px;
    border-radius: 0 2px 2px 0;
    background: linear-gradient(180deg, transparent, var(--toast-accent), transparent);
    box-shadow: 0 0 10px var(--toast-accent);
    opacity: 0.85;
  }
  /* Soft diagonal top sheen */
  .toast::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(160deg, rgba(255, 255, 255, 0.03) 0%, transparent 45%);
    pointer-events: none;
    border-radius: inherit;
  }

  .toast-success { --toast-accent: var(--success); }
  .toast-error   { --toast-accent: var(--danger); }
  .toast-info    { --toast-accent: var(--accent-primary); }

  .toast-icon-wrap {
    position: relative;
    z-index: 1;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--toast-accent) 14%, transparent);
    box-shadow: 0 0 10px color-mix(in srgb, var(--toast-accent) 25%, transparent);
  }

  .toast-msg {
    position: relative;
    z-index: 1;
    flex: 1;
    line-height: 1.45;
    /* Error messages can now carry a real (if summarized) diagnostic
       sentence — wrap and select rather than clip, since the user may
       want to read or copy it. */
    white-space: pre-wrap;
    user-select: text;
  }

  .toast-action {
    position: relative;
    z-index: 1;
    flex-shrink: 0;
    padding: 5px 12px;
    border-radius: 8px;
    border: 1px solid rgba(139, 92, 246, 0.25);
    background: rgba(139, 92, 246, 0.14);
    color: #c4a1ff;
    font-size: var(--text-sm);
    font-weight: 700;
    cursor: pointer;
    transition: background 150ms, border-color 150ms, transform 100ms;
  }
  .toast-action:hover {
    background: rgba(139, 92, 246, 0.24);
    border-color: rgba(139, 92, 246, 0.45);
  }
  .toast-action:active {
    transform: scale(0.96);
  }

  .toast-close {
    position: relative;
    z-index: 1;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border-radius: 7px;
    border: none;
    background: none;
    cursor: pointer;
    opacity: 0.55;
    transition: opacity 150ms, background 150ms;
  }
  .toast-close:hover {
    opacity: 1;
    background: color-mix(in srgb, var(--toast-accent) 12%, transparent);
  }

  @keyframes toastSlideIn {
    from {
      opacity: 0;
      transform: translateX(40px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translateX(0) scale(1);
    }
  }

  /* Light theme — glass ground shifts from near-black to near-white,
     everything else (accent line, sheen direction, glows) stays the same. */
  :global([data-theme="light"]) .toast {
    background: rgba(255, 255, 255, 0.88);
    border-color: rgba(139, 92, 246, 0.1);
    color: #2a2a3e;
  }
  :global([data-theme="light"]) .toast::after {
    background: linear-gradient(160deg, rgba(255, 255, 255, 0.5) 0%, transparent 45%);
  }
</style>
