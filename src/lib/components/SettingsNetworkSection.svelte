<script lang="ts">
  import { onMount } from 'svelte';
  import { success, error as toastError } from '$lib/stores/toast';
  import type { AccountStatus, AccountUser, WebAccessStatus } from '$lib/services/ipc';

  let account = $state<AccountStatus | null>(null);
  let web = $state<WebAccessStatus | null>(null);
  let users = $state<AccountUser[]>([]);
  let loadError = $state('');
  let busy = $state(false);

  // network form
  let enabled = $state(false);
  let lan = $state(false);
  let port = $state(7860);

  // add account / switch account
  let newName = $state('');
  let newTemp = $state('');
  let createdKey = $state<{ user: string; key: string } | null>(null);
  let switchUser = $state('');
  let switchPass = $state('');
  let confirmDelete = $state<string | null>(null);

  // first account (nobody has one yet)
  let adminName = $state('');
  let adminPass = $state('');
  let adminPass2 = $state('');
  let adminKey = $state('');
  const canCreateAdmin = $derived(
    /^[a-z0-9][a-z0-9._-]{2,23}$/.test(adminName.trim().toLowerCase()) &&
      adminPass.length >= 12 &&
      adminPass === adminPass2,
  );

  async function createAdmin() {
    busy = true;
    try {
      const res = await (await ipc()).authRegister(adminName.trim().toLowerCase(), adminPass);
      adminKey = res.recovery_key ?? '';
      adminPass = '';
      adminPass2 = '';
      await load();
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not create the account.');
    } finally {
      busy = false;
    }
  }

  const isAdmin = $derived(account?.user?.role === 'admin');
  const address = $derived(
    web?.bound ? `http://${web.bound.replace('0.0.0.0', '<this-computer>')}` : null,
  );

  async function ipc() {
    return import('$lib/services/ipc');
  }

  async function load() {
    try {
      const api = await ipc();
      account = await api.authStatus();
      if (account.user?.role === 'admin') {
        web = await api.webStatus();
        enabled = web.settings.enabled;
        lan = web.settings.lan;
        port = web.settings.port;
        users = await api.authListUsers();
      }
    } catch (e) {
      loadError = (e as { message?: string }).message ?? 'Could not load account settings.';
    }
  }

  onMount(load);

  async function applyNetwork() {
    busy = true;
    try {
      const api = await ipc();
      web = await api.webSetNetwork(enabled, lan, Number(port));
      success(web.running ? 'Browser access is on' : 'Browser access is off');
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not change browser access.');
      await load();
    } finally {
      busy = false;
    }
  }

  async function setSignup(mode: 'open' | 'admin_only') {
    try {
      await (await ipc()).authSetSignupMode(mode);
      if (account) account = { ...account, signup_mode: mode };
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not change signup.');
    }
  }

  async function addUser() {
    busy = true;
    try {
      const res = await (await ipc()).authCreateUser(newName.trim(), newTemp);
      createdKey = res.recovery_key ? { user: res.user.username, key: res.recovery_key } : null;
      newName = '';
      newTemp = '';
      users = await (await ipc()).authListUsers();
      success('Account created. They choose their own passphrase when they first sign in.');
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not create the account.');
    } finally {
      busy = false;
    }
  }

  async function removeUser(id: string) {
    try {
      await (await ipc()).authDeleteUser(id);
      users = users.filter((u) => u.id !== id);
      confirmDelete = null;
      success('Account and its data deleted');
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not delete the account.');
    }
  }

  async function switchTo(userId: string) {
    busy = true;
    try {
      await (await ipc()).authSwitchTo(userId);
      location.href = '/';
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not switch accounts.');
      busy = false;
    }
  }

  async function switchAccount() {
    busy = true;
    try {
      await (await ipc()).authLogin(switchUser.trim(), switchPass);
      // Everything on screen belongs to the previous account; start fresh.
      location.href = '/';
    } catch (e) {
      toastError((e as { message?: string }).message ?? 'Could not sign in.');
      busy = false;
    }
  }
</script>

<div class="panel-heading animate-fade-in-up stagger-3">
  <span class="panel-heading-title">Network Access</span>
  <span class="panel-heading-desc">Let other devices on your network use Janus from a browser</span>
</div>

{#if account?.impersonator}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Acting as {account.user?.username}</span>
        <span class="setting-desc">You are using this window as that account, for {account.impersonator.username}.</span>
      </div>
      <button class="settings-btn primary" disabled={busy} onclick={() => switchTo(account!.impersonator!.id)}>
        Return to {account.impersonator.username}
      </button>
    </div>
  </section>
{/if}

{#if adminKey}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-label">
      <span class="setting-name">Save your recovery key</span>
      <span class="setting-desc">It lets you set a new passphrase if you forget yours. It is shown once.</span>
    </div>
    <p class="net-key"><code>{adminKey}</code></p>
    <div class="button-row">
      <button class="settings-btn outline" onclick={() => navigator.clipboard?.writeText(adminKey)}>Copy</button>
      <button class="settings-btn primary" onclick={() => (adminKey = '')}>I have saved it</button>
    </div>
  </section>
{/if}

{#if loadError}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <p class="clear-warn">{loadError}</p>
  </section>
{:else if account && !account.has_users}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-label">
      <span class="setting-name">Create the admin account</span>
      <span class="setting-desc">
        Janus has no accounts yet. The first one becomes the admin, owns everything you have now, and decides who else
        can join. Needed before browser access can be turned on.
      </span>
    </div>
    <div class="net-form">
      <label class="net-field">
        <span class="setting-name">Username</span>
        <input class="net-input full" autocomplete="off" bind:value={adminName} />
        <span class="setting-desc">3 to 24 letters, numbers, dot, dash or underscore</span>
      </label>
      <label class="net-field">
        <span class="setting-name">Passphrase</span>
        <input class="net-input full" type="password" autocomplete="new-password" bind:value={adminPass} />
        <span class="setting-desc">At least 12 characters. A sentence you will remember works well.</span>
      </label>
      <label class="net-field">
        <span class="setting-name">Confirm passphrase</span>
        <input class="net-input full" type="password" autocomplete="new-password" bind:value={adminPass2} />
      </label>
      <div class="button-row">
        <button class="settings-btn primary net-submit" disabled={busy || !canCreateAdmin} onclick={createAdmin}>Create admin account</button>
      </div>
    </div>
  </section>
{:else if account && !isAdmin}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-label">
      <span class="setting-name">Signed in as {account.user?.username ?? 'nobody'}</span>
      <span class="setting-desc">Only the admin can change network access and accounts.</span>
    </div>
  </section>
{:else if account}
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Browser access</span>
        <span class="setting-desc">Serves Janus over HTTP. Everyone signs in with their own account.</span>
      </div>
      <button class="toggle-switch" class:on={enabled} role="switch" aria-checked={enabled} aria-label="Browser access" onclick={() => (enabled = !enabled)}>
        <span class="toggle-knob"></span>
      </button>
    </div>
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Other devices on my network</span>
        <span class="setting-desc">Off: only this computer. On: phones and laptops on your Wi-Fi. Do not use on public networks.</span>
      </div>
      <button class="toggle-switch" class:on={lan} role="switch" aria-checked={lan} aria-label="Allow other devices" disabled={!enabled} onclick={() => (lan = !lan)}>
        <span class="toggle-knob"></span>
      </button>
    </div>
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Port</span>
        <span class="setting-desc">1024 or higher</span>
      </div>
      <input class="net-input" type="number" min="1024" max="65535" bind:value={port} aria-label="Port" />
    </div>
    <div class="button-row">
      <button class="settings-btn primary" disabled={busy} onclick={applyNetwork}>Apply</button>
    </div>
    {#if web?.running && address}
      <span class="backup-status">Running at {address}{web.settings.lan ? ' (use this computer\'s address on your network)' : ''}</span>
    {:else}
      <span class="backup-status">Not running</span>
    {/if}
    <p class="setting-desc net-note">
      Connections are plain HTTP, so passphrases are visible to anyone who can watch your network traffic. Use it on a
      home network you trust.
    </p>
  </section>

  <div class="panel-heading animate-fade-in-up stagger-3 net-gap">
    <span class="panel-heading-title">Accounts</span>
    <span class="panel-heading-desc">Each account has its own characters, chats and providers. As admin you can use this window as any of them without their passphrase.</span>
  </div>
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="setting-row">
      <div class="setting-label">
        <span class="setting-name">Who can create accounts</span>
        <span class="setting-desc">{account.signup_mode === 'open' ? 'Anyone who can reach Janus can sign up.' : 'Only you add people.'}</span>
      </div>
      <div class="theme-toggle" role="group" aria-label="Signup mode">
        <button class="theme-btn" class:active={account.signup_mode === 'open'} onclick={() => setSignup('open')}>Open</button>
        <button class="theme-btn" class:active={account.signup_mode === 'admin_only'} onclick={() => setSignup('admin_only')}>Admin adds</button>
      </div>
    </div>

    <ul class="net-users">
      {#each users as u (u.id)}
        <li>
          <div class="setting-label">
            <span class="setting-name">{u.username}{u.id === account.user?.id ? ' (you)' : ''}</span>
            <span class="setting-desc">{u.role === 'admin' ? 'Admin' : 'Member'}{u.must_change ? ' · has not chosen a passphrase yet' : ''}</span>
          </div>
          {#if u.id !== account.user?.id}
            {#if confirmDelete === u.id}
              <div class="button-row">
                <button class="settings-btn danger" onclick={() => removeUser(u.id)}>Delete account and data</button>
                <button class="settings-btn outline" onclick={() => (confirmDelete = null)}>Cancel</button>
              </div>
            {:else}
              <div class="button-row">
                <button class="settings-btn outline" disabled={busy} onclick={() => switchTo(u.id)}>Use this window as them</button>
                <button class="settings-btn outline" onclick={() => (confirmDelete = u.id)}>Delete</button>
              </div>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>

    <div class="net-add">
      <span class="setting-name">Add an account</span>
      <div class="button-row">
        <input class="net-input wide" placeholder="Username" autocomplete="off" bind:value={newName} aria-label="New username" />
        <input class="net-input wide" type="password" placeholder="Temporary passphrase" autocomplete="new-password" bind:value={newTemp} aria-label="Temporary passphrase" />
        <button class="settings-btn primary" disabled={busy || !newName.trim() || newTemp.length < 12} onclick={addUser}>Add</button>
      </div>
      <span class="setting-desc">At least 12 characters. They replace it the first time they sign in.</span>
      {#if createdKey}
        <p class="setting-desc">Recovery key for {createdKey.user} (shown once): <code>{createdKey.key}</code></p>
      {/if}
    </div>
  </section>
{/if}

{#if account && account.has_users && !isAdmin && !account.impersonator}
  <div class="panel-heading animate-fade-in-up stagger-3 net-gap">
    <span class="panel-heading-title">This window</span>
    <span class="panel-heading-desc">Signed in as {account.user?.username ?? 'nobody'}. Sign in as someone else here.</span>
  </div>
  <section class="settings-section settings-section-bounded animate-fade-in-up stagger-3">
    <div class="button-row">
      <input class="net-input wide" placeholder="Username" autocomplete="off" bind:value={switchUser} aria-label="Username to switch to" />
      <input class="net-input wide" type="password" placeholder="Passphrase" autocomplete="current-password" bind:value={switchPass} aria-label="Passphrase" />
      <button class="settings-btn outline" disabled={busy || !switchUser.trim() || !switchPass} onclick={switchAccount}>Switch</button>
    </div>
  </section>
{/if}

<style>
  .net-input {
    height: 34px;
    padding: 0 12px;
    border-radius: 10px;
    background: rgba(14, 14, 30, 0.6);
    border: 1px solid rgba(139, 92, 246, 0.08);
    color: #e0e0f0;
    font: inherit;
    font-size: 12px;
    width: 90px;
  }
  .net-input.wide {
    flex: 1;
    width: auto;
    min-width: 0;
  }
  .net-input:focus-visible {
    outline: 2px solid var(--accent-primary);
  }
  .net-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 18px;
    padding-top: 18px;
    border-top: 1px solid rgba(139, 92, 246, 0.08);
    max-width: 420px;
  }
  .net-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .net-input.full {
    flex: none;
    width: 100%;
    height: 40px;
    padding: 0 14px;
    font-size: 13px;
  }
  .net-input.full:focus {
    border-color: rgba(139, 92, 246, 0.4);
  }
  .net-submit {
    padding: 10px 22px;
  }
  .net-key {
    margin: 12px 0;
    font-size: var(--text-md);
  }
  .net-note {
    margin-top: 10px;
  }
  .net-gap {
    margin-top: 22px;
  }
  .net-users {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
  }
  .net-users li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid rgba(139, 92, 246, 0.08);
  }
  .net-add {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid rgba(139, 92, 246, 0.08);
  }
  code {
    font-family: var(--font-mono);
    color: #cda15f;
  }
</style>
