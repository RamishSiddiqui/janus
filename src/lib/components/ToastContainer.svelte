<script lang="ts">
  import { toasts } from '$lib/stores/toast';
  import Icon from './Icon.svelte';
</script>

{#if $toasts.length > 0}
  <div class="toast-container" aria-live="polite">
    {#each $toasts as toast (toast.id)}
      <div class="toast toast-{toast.type}" role="alert">
        <Icon
          name={toast.type === 'success' ? 'check-circle' : toast.type === 'error' ? 'alert-circle' : 'info'}
          size={16}
          color={toast.type === 'success' ? '#10B981' : toast.type === 'error' ? '#F43F5E' : '#BF40FF'}
        />
        <span class="toast-msg">{toast.message}</span>
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
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 18px;
    border-radius: 14px;
    background: rgba(14,14,30,0.85);
    backdrop-filter: blur(16px);
    border: 1px solid rgba(139,92,246,0.08);
    box-shadow: 0 12px 36px rgba(0,0,0,0.5), 0 0 15px rgba(139,92,246,0.04);
    font-size: var(--text-md);
    font-weight: 500;
    color: #e8e0ff;
    pointer-events: auto;
    animation: toastSlideIn 350ms cubic-bezier(0.34,1.56,0.64,1) forwards;
    min-width: 240px;
    max-width: 400px;
  }

  .toast-success {
    border-left: 3px solid #10B981;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5), 0 0 12px rgba(16,185,129,0.06);
  }

  .toast-error {
    border-left: 3px solid #F43F5E;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5), 0 0 12px rgba(244,63,94,0.06);
  }

  .toast-info {
    border-left: 3px solid #BF40FF;
    box-shadow: 0 12px 36px rgba(0,0,0,0.5), 0 0 12px rgba(191,64,255,0.06);
  }

  .toast-msg {
    flex: 1;
    line-height: 1.4;
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
</style>
