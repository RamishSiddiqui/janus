<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  const isTauri = browser && '__TAURI_INTERNALS__' in window;
  const charId = $derived($page.params.id);
</script>

<svelte:head><title>Character — Mythic</title></svelte:head>

<div class="profile-page">
  <p style="color:#8b8ba7;padding:40px">Loading character {charId}…</p>
</div>

<style>
  .profile-page { flex: 1; display: flex; height: 100%; overflow: hidden; background: #09091a; }
</style>
