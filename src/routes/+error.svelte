<script lang="ts">
  import { page } from '$app/stores';
  import Icon from '$lib/components/Icon.svelte';
</script>

<svelte:head>
  <title>Error — Mythic</title>
</svelte:head>

<div class="error-page">
  <div class="error-card animate-fade-in-scale">
    <div class="error-icon" aria-hidden="true">
      <Icon name="zap" size={48} color="var(--accent-primary)" />
    </div>
    <h1 class="error-code">{$page.status}</h1>
    <p class="error-message">{$page.error?.message || 'Something went wrong'}</p>
    <a href="/" class="error-btn">
      <Icon name="home" size={16} color="#FFFFFF" />
      <span>Return Home</span>
    </a>
  </div>
</div>

<style>
  .error-page {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 24px;
  }

  .error-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 48px;
    border-radius: var(--rounded-lg);
    background: var(--surface-card);
    border: 1px solid var(--border-subtle);
    text-align: center;
    max-width: 400px;
  }

  .error-icon {
    width: 80px;
    height: 80px;
    border-radius: var(--rounded-full);
    background: rgba(139, 92, 246, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .error-code {
    font-size: 48px;
    font-weight: 700;
    color: var(--fg-primary);
    font-family: var(--font-mono);
    margin: 0;
  }

  .error-message {
    font-size: 14px;
    color: var(--fg-secondary);
    line-height: 1.5;
    margin: 0;
  }

  .error-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    border-radius: var(--rounded-md);
    background: var(--accent-primary);
    color: #FFFFFF;
    font-size: 13px;
    font-weight: 600;
    font-family: var(--font-body);
    text-decoration: none;
    margin-top: 8px;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .error-btn:hover {
    background: var(--accent-primary-hover);
  }
</style>
