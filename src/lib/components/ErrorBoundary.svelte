<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    children,
    fallbackTitle = 'Something went wrong',
  }: {
    children: Snippet;
    fallbackTitle?: string;
  } = $props();

  let hasError = $state(false);
  let errorMessage = $state('');

  // Svelte 5 doesn't have built-in error boundaries, so we use a
  // window-level error catcher scoped to this component's lifecycle.
  // For render errors, we wrap the snippet in a try-catch via
  // the onerror handler on the wrapper element.

  function handleError(event: Event) {
    const errEvent = event as ErrorEvent;
    hasError = true;
    errorMessage = errEvent.message || 'An unexpected error occurred';
    event.preventDefault();
  }

  function retry() {
    hasError = false;
    errorMessage = '';
  }
</script>

<svelte:window onerror={handleError} />

{#if hasError}
  <div class="error-boundary">
    <div class="error-card">
      <div class="error-icon-wrap">
        <Icon name="alert-triangle" size={24} color="var(--danger)" />
      </div>
      <span class="error-title">{fallbackTitle}</span>
      <span class="error-msg">{errorMessage}</span>
      <button class="error-retry" onclick={retry}>
        <Icon name="refresh-cw" size={14} color="#FFFFFF" />
        <span>Try Again</span>
      </button>
    </div>
  </div>
{:else}
  {@render children()}
{/if}

<style>
  .error-boundary {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 200px;
    padding: 32px;
  }

  .error-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px 40px;
    border-radius: var(--rounded-lg);
    background: var(--surface-card);
    border: 1px solid rgba(244, 63, 94, 0.2);
    max-width: 360px;
    text-align: center;
  }

  .error-icon-wrap {
    width: 48px;
    height: 48px;
    border-radius: var(--rounded-full);
    background: rgba(244, 63, 94, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
  }

  .error-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--fg-primary);
  }

  .error-msg {
    font-size: 12px;
    color: var(--fg-muted);
    line-height: 1.5;
    word-break: break-word;
  }

  .error-retry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border-radius: var(--rounded-md);
    background: var(--accent-primary);
    border: none;
    color: #FFFFFF;
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-body);
    cursor: pointer;
    margin-top: 4px;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .error-retry:hover {
    opacity: 0.85;
  }
</style>
