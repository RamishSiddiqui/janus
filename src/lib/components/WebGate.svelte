<script lang="ts">
  /**
   * The sign-in screen for browsers (issue #94): "Janus Threshold". Two gates
   * fill the screen and slide apart to reveal the forms. Shown only when the
   * page is served by Janus's own web server and nobody is signed in.
   */
  import { copyText as copyToClipboard } from '#lib/utils/insecureContext.js';
  import { onMount } from 'svelte';
  import {
    webAuth,
    type WebSession,
    type WebStatus,
    type WebUser,
  } from '#lib/services/webBridge.js';
  import {
    LEVELS,
    MIN_PASSPHRASE,
    agoLabel,
    cleanRecoveryKey,
    formatRecoveryKey,
    passphraseStrength,
    usernameOk,
  } from '#lib/utils/webAccount.js';

  let { status, onenter }: { status: WebStatus; onenter: (user: WebUser) => void } = $props();

  type View = 'landing' | 'login' | 'recover' | 'pass' | 'key' | 'welcome';
  type Flow = 'setup' | 'signup' | 'reset' | 'change' | null;

  let view = $state<View>('landing');
  let flow = $state<Flow>(null);
  let peek = $state(false);
  let busy = $state(false);

  const first = $derived(!status.has_users);
  const signupOpen = $derived(status.signup_mode === 'open');

  // form fields
  let loginUser = $state('');
  let loginPass = $state('');
  let loginMsg = $state('');
  let trust = $state(true);
  let recUser = $state('');
  let recKey = $state('');
  let recMsg = $state('');
  let newUser = $state('');
  let newPass = $state('');
  let newPass2 = $state('');
  let acctMsg = $state('');
  let showLogin = $state(false);
  let showNew = $state(false);

  // results
  let user = $state<WebUser | null>(null);
  let issuedKey = $state('');
  let saved = $state(false);
  let copyNote = $state('');
  let sessions = $state<WebSession[]>([]);
  let tempPass = '';
  let shakeEl = $state<'login' | 'recover' | 'pass' | null>(null);

  const strength = $derived(passphraseStrength(newPass));
  const okUser = $derived(usernameOk(newUser.trim().toLowerCase()));
  const okLen = $derived(newPass.length >= MIN_PASSPHRASE);
  const okMatch = $derived(!!newPass && newPass === newPass2);
  const needsUser = $derived(flow === 'setup' || flow === 'signup');
  const canContinue = $derived((!needsUser || okUser) && okLen && okMatch && !busy);

  const STEP_LABELS: Record<string, string[]> = {
    setup: ['Account', 'Key', 'Enter'],
    signup: ['Account', 'Key', 'Enter'],
    reset: ['Recover', 'Phrase', 'Key', 'Enter'],
    change: ['Phrase', 'Enter'],
  };
  const STEP_AT: Record<string, Partial<Record<View, number>>> = {
    setup: { pass: 0, key: 1, welcome: 2 },
    signup: { pass: 0, key: 1, welcome: 2 },
    reset: { pass: 1, key: 2, welcome: 3 },
    change: { pass: 0, welcome: 1 },
  };
  const stepLabels = $derived(flow ? STEP_LABELS[flow] : []);
  const stepIndex = $derived(flow ? (STEP_AT[flow][view] ?? -1) : -1);

  function go(next: View) {
    view = next;
    if (next === 'login' || next === 'landing') flow = null;
    // Focus the first field once the gates have moved.
    setTimeout(
      () => {
        const el = document.querySelector<HTMLElement>(`[data-view-name="${next}"] [data-autofocus]`);
        el?.focus({ preventScroll: true });
      },
      next === 'landing' ? 0 : 420,
    );
  }

  function start(f: Exclude<Flow, null>) {
    flow = f;
    newUser = '';
    newPass = '';
    newPass2 = '';
    acctMsg = '';
    go('pass');
  }

  function fail(which: 'login' | 'recover' | 'pass', setMsg: (m: string) => void, e: unknown) {
    const msg = (e as { message?: string })?.message ?? 'Something went wrong. Try again.';
    setMsg(msg);
    shakeEl = which;
    setTimeout(() => (shakeEl = null), 500);
  }

  async function submitLogin(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    if (!loginUser.trim() || !loginPass) {
      loginMsg = 'Enter your username and passphrase.';
      shakeEl = 'login';
      setTimeout(() => (shakeEl = null), 500);
      return;
    }
    busy = true;
    loginMsg = '';
    try {
      const res = await webAuth.login(loginUser.trim(), loginPass, trust);
      user = res.user;
      if (res.user.must_change) {
        tempPass = loginPass;
        start('change');
      } else {
        await openWelcome();
      }
      loginPass = '';
    } catch (err) {
      fail('login', (m) => (loginMsg = m), err);
    } finally {
      busy = false;
    }
  }

  function onRecKey(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    recKey = formatRecoveryKey(cleanRecoveryKey(input.value));
    recMsg = '';
  }

  function submitRecover(e: SubmitEvent) {
    e.preventDefault();
    if (!recUser.trim()) {
      recMsg = 'Enter your username.';
      shakeEl = 'recover';
      setTimeout(() => (shakeEl = null), 500);
      return;
    }
    if (cleanRecoveryKey(recKey).length !== 16) {
      recMsg =
        "That doesn't look like a Janus recovery key. It has four groups after JANUS, like JANUS-XXXX-XXXX-XXXX-XXXX.";
      shakeEl = 'recover';
      setTimeout(() => (shakeEl = null), 500);
      return;
    }
    start('reset');
  }

  async function submitPass(e: SubmitEvent) {
    e.preventDefault();
    if (!canContinue) return;
    busy = true;
    acctMsg = '';
    try {
      if (flow === 'setup' || flow === 'signup') {
        const res = await webAuth.register(newUser.trim().toLowerCase(), newPass, trust);
        user = res.user;
        issuedKey = res.recovery_key ?? '';
        prepKey();
        go('key');
      } else if (flow === 'reset') {
        const res = await webAuth.reset(recUser.trim(), recKey, newPass, trust);
        user = res.user;
        issuedKey = res.recovery_key ?? '';
        prepKey();
        go('key');
      } else if (flow === 'change') {
        await webAuth.changePassphrase(tempPass, newPass);
        tempPass = '';
        if (user) user = { ...user, must_change: false };
        await openWelcome();
      }
      newPass = '';
      newPass2 = '';
    } catch (err) {
      fail('pass', (m) => (acctMsg = m), err);
    } finally {
      busy = false;
    }
  }

  function prepKey() {
    saved = false;
    copyNote = '';
  }

  async function copyKey() {
    copyNote = (await copyToClipboard(issuedKey)) ? 'Copied' : 'Select the key and press Ctrl+C';
  }

  async function openWelcome() {
    try {
      sessions = await webAuth.sessions();
    } catch {
      sessions = [];
    }
    go('welcome');
  }

  async function signOutDevice(id: string) {
    try {
      await webAuth.revokeSession(id);
      sessions = sessions.filter((s) => s.id !== id);
    } catch {
      // The list refreshes next time; nothing to undo.
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    if (view === 'login') go('landing');
    else if (view === 'recover') go('login');
  }

  const welcomeTitle = $derived(
    flow === 'reset'
      ? ['Passphrase', 'reset.']
      : flow === 'signup'
        ? ['You are', 'in.']
        : flow === 'setup'
          ? ['The gates are', 'open.']
          : ['Welcome', 'back.'],
  );
  const welcomeLede = $derived(
    flow === 'reset'
      ? 'Other devices have been signed out. This one is signed in.'
      : flow === 'signup'
        ? 'Account created.'
        : flow === 'setup'
          ? 'Admin account created. Nobody else gets past the door unless you let them.'
          : trust
            ? 'Signed in for 30 days on this device.'
            : 'Signed in until you close this tab.',
  );

  // ── drifting dust ──
  let stageEl: HTMLDivElement;
  let canvas: HTMLCanvasElement;

  onMount(() => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    let W = 0;
    let H = 0;
    let raf = 0;
    const t0 = performance.now();
    type P = { x: number; y: number; r: number; vy: number; vx: number; a: number; f: number; ph: number; gold: boolean };
    let parts: P[] = [];
    const rnd = (a: number, b: number) => a + Math.random() * (b - a);

    function seed() {
      const n = Math.max(36, Math.min(110, Math.round((W * H) / 22000)));
      parts = [];
      for (let i = 0; i < n; i++) {
        const near = Math.random() < 0.5;
        parts.push({
          x: near ? W / 2 + (Math.random() + Math.random() + Math.random() - 1.5) * W * 0.34 : rnd(0, W),
          y: rnd(0, H),
          r: rnd(0.6, 1.8),
          vy: -rnd(0.06, 0.32),
          vx: rnd(-0.03, 0.03),
          a: rnd(0.12, 0.55),
          f: rnd(0.4, 1.4),
          ph: rnd(0, 6.28),
          gold: Math.random() < 0.62,
        });
      }
    }
    function size() {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      W = stageEl.clientWidth;
      H = stageEl.clientHeight;
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      seed();
    }
    function draw(now: number) {
      const t = (now - t0) / 1000;
      ctx!.clearRect(0, 0, W, H);
      for (const p of parts) {
        p.y += p.vy;
        p.x += p.vx + Math.sin(t * p.f + p.ph) * 0.12;
        if (p.y < -4) p.y = H + 4;
        if (p.x < -4) p.x = W + 4;
        else if (p.x > W + 4) p.x = -4;
        const a = p.a * (0.6 + 0.4 * Math.sin(t * p.f * 1.7 + p.ph));
        ctx!.fillStyle = p.gold ? `rgba(232,203,150,${a.toFixed(3)})` : `rgba(185,167,255,${a.toFixed(3)})`;
        ctx!.beginPath();
        ctx!.arc(p.x, p.y, p.r, 0, 6.2832);
        ctx!.fill();
      }
      if (!reduce) raf = requestAnimationFrame(draw);
    }
    const onResize = () => {
      size();
      if (reduce) draw(performance.now());
    };
    const onVisible = () => {
      if (reduce) return;
      if (document.hidden) cancelAnimationFrame(raf);
      else raf = requestAnimationFrame(draw);
    };
    size();
    draw(performance.now());
    window.addEventListener('resize', onResize);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', onResize);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });

  const doors = $derived(view === 'landing' ? 'closed' : view === 'welcome' ? 'wide' : 'ajar');
</script>

<svelte:window onkeydown={onKey} />
<svelte:head>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link
    rel="stylesheet"
    href="https://fonts.googleapis.com/css2?family=Fraunces:ital,opsz,wght@0,9..144,300;0,9..144,400;1,9..144,300;1,9..144,400&display=swap"
  />
</svelte:head>

<div
  class="stage"
  bind:this={stageEl}
  data-view={view}
  data-doors={doors}
  data-peek={peek && view === 'landing' ? '' : undefined}
  data-mode={first ? 'first' : 'returning'}
>
  <header class="top">
    <span class="brand" aria-label="Janus">
      <svg viewBox="-61 -83 122 166" fill="none" stroke-width="18" stroke-linecap="round" aria-hidden="true"
        ><defs
          ><linearGradient id="jsplit" gradientUnits="userSpaceOnUse" x1="0" y1="-80" x2="0" y2="80"
            ><stop offset="0" stop-color="#CDA15F" /><stop offset=".485" stop-color="#CDA15F" /><stop
              offset=".515"
              stop-color="#9075F2"
            /><stop offset="1" stop-color="#9075F2" /></linearGradient
          ></defs
        ><path
          stroke="url(#jsplit)"
          d="M -52,52 Q -52,74 -30,74 Q 0,74 0,46 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52"
        /></svg
      >
      <span><b class="w1">JA</b><b class="w2">NUS</b></span>
    </span>
    <div class="top-right">
      <span class="chip"><i class="dot"></i><span class="addr">{location.host}</span></span>
      <span class="chip gate" class:open={view === 'welcome'}>{view === 'welcome' ? 'Gates open' : 'Gates closed'}</span>
    </div>
  </header>

  <section class="vestibule" aria-label="Sign in">
    <div class="col">
      {#if stepIndex >= 0}
        <ol class="steps">
          {#each stepLabels as label, i (label)}
            <li class:done={i < stepIndex} class:cur={i === stepIndex} aria-current={i === stepIndex ? 'step' : undefined}>
              {label}
            </li>
          {/each}
        </ol>
      {/if}

      {#if view === 'login'}
        <form class="view" class:shake={shakeEl === 'login'} data-view-name="login" onsubmit={submitLogin} novalidate>
          <button type="button" class="back" onclick={() => go('landing')}>&larr; Back</button>
          <p class="eyebrow">{location.hostname}</p>
          <h2>Welcome <em>back.</em></h2>
          <p class="lede">Sign in to Janus.</p>
          <div class="field">
            <label class="lab" for="loginUser">Username</label>
            <div class="input-row" class:bad={!!loginMsg}>
              <input
                class="txt"
                id="loginUser"
                type="text"
                autocomplete="username"
                autocapitalize="none"
                spellcheck="false"
                data-autofocus
                bind:value={loginUser}
                oninput={() => (loginMsg = '')}
              />
            </div>
          </div>
          <div class="field">
            <label class="lab" for="loginPass">Passphrase</label>
            <div class="input-row" class:bad={!!loginMsg}>
              <input
                class="txt"
                id="loginPass"
                type={showLogin ? 'text' : 'password'}
                autocomplete="current-password"
                spellcheck="false"
                bind:value={loginPass}
                oninput={() => (loginMsg = '')}
              />
              <button type="button" class="ghost" aria-pressed={showLogin} onclick={() => (showLogin = !showLogin)}>
                {showLogin ? 'Hide' : 'Show'}
              </button>
            </div>
          </div>
          <p class="msg" role="alert">{loginMsg}</p>
          <label class="check"><input type="checkbox" bind:checked={trust} /><span>Keep this device signed in for 30 days</span></label>
          <button class="cta" type="submit" disabled={busy}>
            <span>{busy ? 'Unlatching' : 'Unlatch'}</span>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
          </button>
          <div class="actions">
            <button type="button" class="link" onclick={() => go('recover')}>Use recovery key instead</button>
            {#if signupOpen}
              <button type="button" class="link" onclick={() => start('signup')}>Create an account</button>
            {:else}
              <span class="note-mute">New here? Ask the admin to add you.</span>
            {/if}
          </div>
        </form>
      {:else if view === 'recover'}
        <form class="view" class:shake={shakeEl === 'recover'} data-view-name="recover" onsubmit={submitRecover} novalidate>
          <button type="button" class="back" onclick={() => go('login')}>&larr; Back</button>
          <p class="eyebrow">Recovery</p>
          <h2>Use your <em>recovery key.</em></h2>
          <p class="lede">
            The key you saved when you created your account. It lets you choose a new passphrase and signs out every
            other device on your account.
          </p>
          <div class="field">
            <label class="lab" for="recUser">Username</label>
            <div class="input-row">
              <input class="txt" id="recUser" type="text" autocomplete="username" autocapitalize="none" spellcheck="false" data-autofocus bind:value={recUser} oninput={() => (recMsg = '')} />
            </div>
          </div>
          <div class="field">
            <label class="lab" for="recKey">Recovery key</label>
            <div class="input-row" class:bad={!!recMsg}>
              <input class="txt mono" id="recKey" type="text" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="JANUS-XXXX-XXXX-XXXX-XXXX" value={recKey} oninput={onRecKey} />
            </div>
          </div>
          <p class="msg" role="alert">{recMsg}</p>
          <button class="cta" type="submit">
            <span>Continue</span>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
          </button>
        </form>
      {:else if view === 'pass'}
        <form class="view" class:shake={shakeEl === 'pass'} data-view-name="pass" onsubmit={submitPass} novalidate>
          {#if flow === 'signup' || flow === 'setup'}
            <button type="button" class="back" onclick={() => go(flow === 'signup' ? 'login' : 'landing')}>&larr; Back</button>
          {/if}
          <p class="eyebrow">
            {flow === 'reset' ? 'Step two' : flow === 'change' ? 'One more thing' : flow === 'setup' ? 'First account' : 'New account'}
          </p>
          {#if flow === 'reset'}
            <h2>Choose a <em>new passphrase.</em></h2>
            <p class="lede">This replaces the old one everywhere. Any device still signed in with it will be signed out.</p>
          {:else if flow === 'change'}
            <h2>Choose your <em>own passphrase.</em></h2>
            <p class="lede">The admin gave you a temporary one. Pick a passphrase only you know to continue.</p>
          {:else if flow === 'setup'}
            <h2>Create the <em>admin account.</em></h2>
            <p class="lede">
              You are the first one here, so you run this Janus: add people, switch signups on or off, and sign devices
              out. Janus keeps only a salted argon2id hash of your passphrase.
            </p>
          {:else}
            <h2>Create your <em>account.</em></h2>
            <p class="lede">Choose a username and a passphrase. Janus keeps only a salted argon2id hash, never the passphrase itself.</p>
          {/if}
          {#if needsUser}
            <div class="field">
              <label class="lab" for="newUser">Username</label>
              <div class="input-row">
                <input class="txt" id="newUser" type="text" autocomplete="username" autocapitalize="none" spellcheck="false" data-autofocus bind:value={newUser} oninput={() => (acctMsg = '')} />
              </div>
            </div>
          {/if}
          <div class="field">
            <label class="lab" for="newPass">Passphrase</label>
            <div class="input-row">
              <input class="txt" id="newPass" type={showNew ? 'text' : 'password'} autocomplete="new-password" spellcheck="false" data-autofocus={!needsUser ? '' : undefined} bind:value={newPass} />
              <button type="button" class="ghost" aria-pressed={showNew} onclick={() => (showNew = !showNew)}>{showNew ? 'Hide' : 'Show'}</button>
            </div>
            <div class="meter" data-level={newPass ? strength : 0} aria-hidden="true"><i></i><i></i><i></i><i></i></div>
            <span class="meter-label" aria-live="polite">{newPass ? LEVELS[strength] : LEVELS[0]}</span>
          </div>
          <div class="field">
            <label class="lab" for="newPass2">Confirm</label>
            <div class="input-row"><input class="txt" id="newPass2" type={showNew ? 'text' : 'password'} autocomplete="new-password" spellcheck="false" bind:value={newPass2} /></div>
          </div>
          <ul class="rules">
            {#if needsUser}<li class:met={okUser}>Username is 3 to 24 letters, numbers, . _ or -</li>{/if}
            <li class:met={okLen}>Passphrase is at least {MIN_PASSPHRASE} characters</li>
            <li class:met={okMatch}>Both entries match</li>
          </ul>
          <p class="msg" role="alert">{acctMsg}</p>
          <div class="actions">
            <button class="cta" type="submit" disabled={!canContinue}>
              <span>{busy ? 'Working' : 'Continue'}</span>
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
            </button>
          </div>
        </form>
      {:else if view === 'key'}
        <form
          class="view"
          data-view-name="key"
          onsubmit={(e) => {
            e.preventDefault();
            if (saved) void openWelcome();
          }}
        >
          <p class="eyebrow">{flow === 'reset' ? 'Step three' : 'Step two'}</p>
          <h2>{flow === 'reset' ? 'Here is your' : 'Keep this'} <em>{flow === 'reset' ? 'new key.' : 'recovery key.'}</em></h2>
          <p class="lede">
            If you forget the passphrase, this key lets you set a new one from any device on the network. Janus cannot
            show it again.
          </p>
          <div class="keybox" aria-label={'Recovery key: ' + issuedKey.split('-').join(' ')}>
            {#each issuedKey.split('-') as group, i (i)}<span>{group}</span>{/each}
          </div>
          <div class="keyrow">
            <button type="button" class="ghost" onclick={copyKey}>Copy key</button>
            <span class="note" aria-live="polite">{copyNote}</span>
          </div>
          <label class="check"><input type="checkbox" bind:checked={saved} data-autofocus /><span>I have saved it somewhere safe</span></label>
          <button class="cta" type="submit" disabled={!saved}>
            <span>Seal the door</span>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
          </button>
        </form>
      {:else if view === 'welcome'}
        <section class="view" data-view-name="welcome">
          <p class="eyebrow">Gates open · {user?.role === 'admin' ? 'Admin' : 'Member'}</p>
          <h2>{welcomeTitle[0]} <em>{welcomeTitle[1]}</em></h2>
          <p class="lede">{welcomeLede}</p>
          {#if user?.role === 'admin' && !first}
            <p class="lede note-mute">
              {signupOpen
                ? 'Signups are open: anyone on this network can create an account. Change it in Settings → Network access.'
                : 'Signups are off: only you can add people. Do it from Settings → Network access.'}
            </p>
          {/if}
          {#if sessions.length}
            <ul class="devices">
              {#each sessions as s (s.id)}
                <li>
                  <div>
                    <span>{s.label}{#if s.current}<span class="here">  ●</span>{/if}</span>
                    <span class="meta-line">{s.current ? 'This device' : 'Signed in'} · {agoLabel(s.last_seen)}</span>
                  </div>
                  {#if !s.current}
                    <button type="button" class="ghost" aria-label={'Sign out ' + s.label} onclick={() => signOutDevice(s.id)}>Sign out</button>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
          <button class="cta" type="button" data-autofocus onclick={() => user && onenter(user)}>
            <span>Step inside</span>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
          </button>
        </section>
      {/if}
    </div>
  </section>

  <div class="door door-l" aria-hidden="true"><span class="lintel">Ianus</span><i class="pull"></i></div>
  <div class="door door-r" aria-hidden="true"><span class="lintel">Geminus</span><i class="pull"></i></div>
  <div class="seam" aria-hidden="true"></div>
  <div class="spill" aria-hidden="true"></div>
  <canvas bind:this={canvas} class="dust" aria-hidden="true"></canvas>

  <section class="hero">
    {#if first}
      <p class="eyebrow">Nobody has claimed this Janus yet</p>
    {:else}
      <p class="eyebrow">Janus is awake on this network</p>
    {/if}
    <h1>Every story begins at a <em>threshold.</em></h1>
    {#if first}
      <p class="lede">
        Janus is reachable from this network, but there are no accounts yet. Whoever creates the first one becomes the
        admin and decides who else gets in.
      </p>
      <div class="actions center">
        <button class="cta" type="button" onmouseenter={() => (peek = true)} onmouseleave={() => (peek = false)} onfocus={() => (peek = true)} onblur={() => (peek = false)} onclick={() => { peek = false; start('setup'); }}>
          <span>Create the admin account</span>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
        </button>
      </div>
    {:else}
      <p class="lede">
        Your characters, memories and keys stay on the machine hosting Janus. Pass the gate and everything is exactly
        where you left it.
      </p>
      <div class="actions center">
        <button class="cta" type="button" onmouseenter={() => (peek = true)} onmouseleave={() => (peek = false)} onfocus={() => (peek = true)} onblur={() => (peek = false)} onclick={() => { peek = false; go('login'); }}>
          <span>Enter</span>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h11M9 4l4 4-4 4" /></svg>
        </button>
        {#if signupOpen}
          <button class="link" type="button" onclick={() => start('signup')}>New here? Create an account</button>
        {:else}
          <span class="note-mute">New here? Ask the admin to add you.</span>
        {/if}
      </div>
    {/if}
  </section>

  <footer class="meta"><span>Local network only</span><span>Passphrase hashed, never stored</span><span>No account, no cloud</span></footer>
</div>

<style>
  .stage {
    --gap: min(33rem, calc(100vw - 2rem));
    --ink: #06060d;
    --ink-2: #0b0b17;
    --ink-3: #14142a;
    --gold: #cda15f;
    --gold-hi: #e8cb96;
    --violet: #9075f2;
    --violet-hi: #b9a7ff;
    --text: #ece9f6;
    --dim: #b2afc6;
    --mute: #8d8aa6;
    --line: rgba(205, 161, 95, 0.32);
    --line-soft: rgba(205, 161, 95, 0.14);
    --hair: rgba(236, 233, 246, 0.14);
    --danger: #ff8a9a;
    --ok: #7be0b0;
    --display: 'Fraunces', Georgia, 'Times New Roman', serif;
    --body: 'Raleway', system-ui, -apple-system, 'Segoe UI', sans-serif;
    --mono: 'Geist Mono', ui-monospace, 'SF Mono', Consolas, monospace;
    --ease: cubic-bezier(0.76, 0, 0.18, 1);
    position: fixed;
    inset: 0;
    z-index: 2000;
    overflow: hidden;
    isolation: isolate;
    color: var(--text);
    font: 400 15px/1.55 var(--body);
    -webkit-font-smoothing: antialiased;
    background:
      radial-gradient(60% 50% at 50% 0%, rgba(144, 117, 242, 0.16), transparent 70%),
      radial-gradient(70% 60% at 50% 110%, rgba(205, 161, 95, 0.14), transparent 70%), var(--ink);
  }
  .stage :global(*) {
    box-sizing: border-box;
  }
  .stage :global(button),
  .stage :global(input) {
    font: inherit;
    color: inherit;
  }
  .stage :global(:focus-visible) {
    outline: 2px solid var(--violet-hi);
    outline-offset: 3px;
  }

  /* top bar */
  .top {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 6;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 1.1rem clamp(1rem, 3vw, 2.2rem);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    font-weight: 600;
    font-size: 0.78rem;
    letter-spacing: 0.44em;
  }
  .brand svg {
    width: 1.45rem;
    height: 2rem;
    flex: none;
  }
  .brand b {
    font-weight: 600;
  }
  .w1 {
    color: #9075f2;
  }
  .w2 {
    color: #cda15f;
  }
  .top-right {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--hair);
    border-radius: 99px;
    background: rgba(6, 6, 13, 0.6);
    backdrop-filter: blur(8px);
    font: 500 0.7rem/1 var(--mono);
    letter-spacing: 0.04em;
    color: var(--dim);
  }
  .chip .dot {
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 3px rgba(123, 224, 176, 0.16);
  }
  .chip.gate::before {
    content: '';
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 50%;
    background: var(--gold);
    box-shadow: 0 0 0 3px rgba(205, 161, 95, 0.16);
  }
  .chip.gate.open {
    color: var(--gold-hi);
    border-color: var(--line);
  }
  @media (max-width: 640px) {
    .chip .addr {
      display: none;
    }
  }

  /* gates */
  .door {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 50%;
    z-index: 2;
    transition: transform 1.1s var(--ease);
    will-change: transform;
  }
  .door-l {
    left: 0;
    border-right: 1px solid var(--line);
    background-image:
      radial-gradient(90% 55% at 100% 62%, rgba(144, 117, 242, 0.16), transparent 70%),
      repeating-linear-gradient(90deg, rgba(255, 255, 255, 0.014) 0 1px, transparent 1px 9px),
      linear-gradient(180deg, #0e0e1e, #07070e);
  }
  .door-r {
    right: 0;
    border-left: 1px solid var(--line);
    background-image:
      radial-gradient(90% 55% at 0% 62%, rgba(205, 161, 95, 0.14), transparent 70%),
      repeating-linear-gradient(90deg, rgba(255, 255, 255, 0.014) 0 1px, transparent 1px 9px),
      linear-gradient(180deg, #0e0e1e, #07070e);
  }
  .door::before,
  .door::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--line-soft) 18%, var(--line-soft) 82%, transparent);
  }
  .door::before {
    top: calc(clamp(4.6rem, 11vh, 6.4rem) + 2.6rem);
  }
  .door::after {
    bottom: 3.4rem;
  }
  .lintel {
    position: absolute;
    top: clamp(4.6rem, 11vh, 6.4rem);
    font: 500 clamp(0.8rem, 1.6vw, 1.1rem) / 1 var(--body);
    letter-spacing: 0.55em;
    font-kerning: none;
    color: rgba(232, 203, 150, 0.7);
    text-transform: uppercase;
    white-space: nowrap;
  }
  .door-l .lintel {
    right: clamp(1.4rem, 7%, 5rem);
    margin-right: -0.55em;
  }
  .door-r .lintel {
    left: clamp(1.4rem, 7%, 5rem);
  }
  .pull {
    position: absolute;
    top: 79%;
    width: 3.4rem;
    height: 3.4rem;
    margin-top: -1.7rem;
    border-radius: 50%;
    border: 1.5px solid rgba(205, 161, 95, 0.7);
    box-shadow:
      0 0 0 0.55rem rgba(205, 161, 95, 0.05),
      inset 0 0 16px rgba(205, 161, 95, 0.2),
      0 10px 28px rgba(0, 0, 0, 0.55);
  }
  .pull::after {
    content: '';
    position: absolute;
    inset: 1.1rem;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, var(--gold-hi), var(--gold) 60%, #7a5a2a);
  }
  .door-l .pull {
    right: clamp(1.2rem, 6%, 4.5rem);
  }
  .door-r .pull {
    left: clamp(1.2rem, 6%, 4.5rem);
  }
  .stage[data-doors='closed'][data-peek] .door-l {
    transform: translateX(-6px);
  }
  .stage[data-doors='closed'][data-peek] .door-r {
    transform: translateX(6px);
  }
  .stage[data-doors='ajar'] .door-l {
    transform: translateX(calc(var(--gap) / -2));
  }
  .stage[data-doors='ajar'] .door-r {
    transform: translateX(calc(var(--gap) / 2));
  }
  .stage[data-doors='wide'] .door-l {
    transform: translateX(-94%);
  }
  .stage[data-doors='wide'] .door-r {
    transform: translateX(94%);
  }

  .seam {
    position: absolute;
    z-index: 3;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    pointer-events: none;
    background: linear-gradient(180deg, transparent, var(--gold) 22%, var(--violet) 72%, transparent);
    box-shadow:
      0 0 22px 2px rgba(205, 161, 95, 0.42),
      0 0 90px 12px rgba(144, 117, 242, 0.26);
    animation: breathe 5s ease-in-out infinite;
    transform-origin: 50% 50%;
    transition:
      opacity 0.9s ease,
      transform 1.4s cubic-bezier(0.25, 0.6, 0.25, 1),
      box-shadow 1.4s ease;
  }
  .stage:not([data-view='landing']) .seam,
  .stage:not([data-view='landing']) .spill {
    opacity: 0 !important;
  }
  .seam::after {
    content: '';
    position: absolute;
    left: -1px;
    right: -1px;
    top: 0;
    height: 26%;
    opacity: 0;
    transform: translateY(-100%);
    background: linear-gradient(180deg, transparent, rgba(255, 244, 220, 0.95) 50%, transparent);
    transition: opacity 0.8s ease;
  }
  .stage[data-peek] .seam {
    animation: none;
    opacity: 0.95;
    transform: scaleX(1.7);
    transition-duration: 1.6s, 2.2s, 2.2s;
    box-shadow:
      0 0 26px 3px rgba(232, 203, 150, 0.34),
      0 0 90px 18px rgba(144, 117, 242, 0.2);
  }
  .stage[data-peek] .seam::after {
    opacity: 1;
    animation: glint 3.6s cubic-bezier(0.45, 0, 0.2, 1) 0.35s infinite;
  }
  @keyframes glint {
    0% {
      transform: translateY(-100%);
    }
    70%,
    100% {
      transform: translateY(385%);
    }
  }
  .spill {
    position: absolute;
    z-index: 3;
    left: 50%;
    top: 0;
    bottom: 0;
    width: min(16vw, 15rem);
    pointer-events: none;
    mix-blend-mode: screen;
    transform: translateX(-50%) scaleX(0.15);
    opacity: 0;
    transform-origin: 50% 50%;
    background:
      linear-gradient(90deg, transparent 42%, rgba(255, 241, 210, 0.22) 50%, transparent 58%),
      radial-gradient(ellipse 50% 60% at 50% 55%, rgba(232, 203, 150, 0.2), rgba(144, 117, 242, 0.1) 45%, transparent 75%);
    transition:
      transform 1.2s ease,
      opacity 1s ease;
  }
  .stage[data-peek] .spill {
    transform: translateX(-50%) scaleX(1);
    opacity: 1;
    transition:
      transform 2.6s cubic-bezier(0.2, 0.7, 0.2, 1) 0.15s,
      opacity 2.2s ease 0.15s;
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }
  .dust {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: 3;
    pointer-events: none;
  }

  /* vestibule */
  .vestibule {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: var(--gap);
    transform: translateX(-50%);
    z-index: 1;
    display: flex;
    overflow-y: auto;
    background:
      radial-gradient(80% 50% at 50% 30%, rgba(205, 161, 95, 0.13), transparent 70%),
      radial-gradient(90% 60% at 50% 100%, rgba(144, 117, 242, 0.16), transparent 70%), var(--ink-2);
    border-inline: 1px solid var(--line-soft);
    visibility: hidden;
    transition:
      visibility 0s linear 1.1s,
      width 0.9s var(--ease);
  }
  .stage[data-doors='ajar'] .vestibule,
  .stage[data-doors='wide'] .vestibule {
    visibility: visible;
    transition-delay: 0s;
  }
  .stage[data-doors='wide'] .vestibule {
    width: 100%;
    border-inline: 0;
  }
  .col {
    width: 100%;
    max-width: 26rem;
    margin: auto;
    padding: 5.4rem 1.4rem 6.2rem;
  }
  .view {
    animation: rise 0.7s 0.25s both cubic-bezier(0.2, 0.7, 0.2, 1);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(14px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .steps {
    list-style: none;
    margin: 0 0 2.2rem;
    padding: 0;
    display: flex;
    gap: 0.45rem;
    font: 500 0.6rem/1 var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--mute);
  }
  .steps li {
    flex: 1;
    border-top: 2px solid var(--hair);
    padding-top: 0.55rem;
  }
  .steps li.done {
    border-color: var(--gold);
    color: var(--dim);
  }
  .steps li.cur {
    border-color: var(--violet-hi);
    color: var(--text);
  }

  .eyebrow {
    margin: 0;
    font: 500 0.72rem/1 var(--mono);
    letter-spacing: 0.2em;
    text-transform: uppercase;
    color: var(--gold-hi);
  }
  h2 {
    margin: 0.8rem 0 0.8rem;
    font: 300 clamp(2rem, 5vw, 2.7rem) / 1.1 var(--display);
    font-variation-settings: 'opsz' 96;
    letter-spacing: -0.008em;
    text-wrap: balance;
  }
  h2 em,
  h1 em {
    font-style: italic;
  }
  .lede {
    margin: 0;
    color: var(--dim);
    max-width: 34rem;
    text-wrap: pretty;
  }
  .field {
    margin-top: 1.7rem;
  }
  .lab {
    display: block;
    margin-bottom: 0.35rem;
    font: 600 0.68rem/1.2 var(--body);
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--mute);
  }
  .input-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border-bottom: 1px solid var(--hair);
    transition:
      border-color 0.2s,
      box-shadow 0.2s;
  }
  .input-row:focus-within {
    border-color: var(--violet-hi);
    box-shadow: 0 1px 0 0 var(--violet-hi);
  }
  .input-row.bad {
    border-color: var(--danger);
    box-shadow: 0 1px 0 0 var(--danger);
  }
  .txt {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    outline: 0;
    padding: 0.7rem 0;
    font: 500 1.15rem/1.3 var(--body);
    letter-spacing: 0.02em;
    color: var(--text);
  }
  .txt.mono {
    font: 500 1rem/1.3 var(--mono);
    letter-spacing: 0.08em;
  }
  .stage .txt:focus-visible,
  .stage .check input:focus-visible {
    outline: none;
  }
  .stage .check input:focus-visible {
    box-shadow: 0 0 0 3px rgba(185, 167, 255, 0.35);
  }
  .txt::placeholder {
    color: var(--mute);
    opacity: 0.7;
  }
  .msg {
    min-height: 1.4rem;
    margin: 0.9rem 0 0;
    font-size: 0.86rem;
    color: var(--danger);
  }
  .check {
    display: flex;
    gap: 0.7rem;
    align-items: flex-start;
    margin: 1.2rem 0;
    color: var(--dim);
    font-size: 0.9rem;
    cursor: pointer;
  }
  .check input {
    appearance: none;
    -webkit-appearance: none;
    flex: none;
    width: 1.15rem;
    height: 1.15rem;
    margin: 0.12rem 0 0;
    border: 1.5px solid var(--mute);
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
    display: grid;
    place-items: center;
    transition:
      border-color 0.2s,
      background 0.2s;
  }
  .check input:hover {
    border-color: var(--gold-hi);
  }
  .check input:checked {
    background: var(--gold);
    border-color: var(--gold);
  }
  .check input:checked::after {
    content: '';
    width: 0.3rem;
    height: 0.58rem;
    margin-top: -0.1rem;
    border: solid #1a1306;
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }

  .cta {
    position: relative;
    overflow: hidden;
    isolation: isolate;
    appearance: none;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 1.4rem;
    padding: 0.55rem 0.55rem 0.55rem 1.8rem;
    border-radius: 99px;
    border: 1px solid rgba(205, 161, 95, 0.55);
    background: rgba(205, 161, 95, 0.05);
    color: var(--gold-hi);
    font: 600 0.78rem/1 var(--body);
    letter-spacing: 0.24em;
    text-transform: uppercase;
    box-shadow:
      0 0 0 0 rgba(205, 161, 95, 0),
      inset 0 0 22px rgba(205, 161, 95, 0.06);
    transition:
      color 0.5s ease 0.05s,
      border-color 0.5s ease,
      box-shadow 0.7s ease;
  }
  .cta::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: inherit;
    background: linear-gradient(180deg, var(--gold-hi), var(--gold));
    transform: scaleX(0);
    transform-origin: 50% 50%;
    transition: transform 0.75s cubic-bezier(0.7, 0, 0.2, 1);
  }
  .cta svg {
    flex: none;
    box-sizing: content-box;
    width: 0.95rem;
    height: 0.95rem;
    padding: 0.7rem;
    border-radius: 50%;
    border: 1px solid rgba(205, 161, 95, 0.5);
    transition:
      transform 0.5s ease,
      border-color 0.5s ease;
  }
  .cta:hover:not(:disabled),
  .cta:focus-visible {
    color: #1a1306;
    border-color: var(--gold-hi);
    box-shadow:
      0 0 36px -6px rgba(205, 161, 95, 0.55),
      inset 0 0 22px rgba(205, 161, 95, 0.06);
  }
  .cta:hover:not(:disabled)::before,
  .cta:focus-visible::before {
    transform: scaleX(1);
  }
  .cta:hover:not(:disabled) svg,
  .cta:focus-visible svg {
    transform: translateX(3px);
    border-color: rgba(26, 19, 6, 0.45);
  }
  .cta:active {
    transform: translateY(1px);
  }
  .cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .link {
    appearance: none;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    color: var(--dim);
    text-decoration: underline;
    text-decoration-color: var(--line);
    text-underline-offset: 0.32em;
    transition: color 0.2s;
  }
  .link:hover {
    color: var(--text);
  }
  .ghost {
    appearance: none;
    cursor: pointer;
    background: transparent;
    border: 0;
    border-radius: 99px;
    color: var(--mute);
    padding: 0.4rem 0.2rem;
    font: 600 0.66rem/1 var(--body);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    transition: color 0.2s;
  }
  .ghost:hover {
    color: var(--gold-hi);
  }
  .devices .ghost,
  .keyrow .ghost {
    border: 1px solid var(--hair);
    padding: 0.5rem 0.9rem;
  }
  .devices .ghost:hover,
  .keyrow .ghost:hover {
    border-color: var(--line);
  }
  .back {
    appearance: none;
    background: none;
    border: 0;
    padding: 0 0 1.4rem;
    cursor: pointer;
    color: var(--mute);
    font: 600 0.7rem/1 var(--body);
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .back:hover {
    color: var(--text);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem 1.6rem;
    align-items: center;
    margin-top: 1.6rem;
  }
  .actions.center {
    justify-content: center;
    margin-top: 2.1rem;
  }
  .note-mute {
    color: var(--mute);
    font-size: 0.86rem;
  }

  .meter {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.3rem;
    margin-top: 1rem;
  }
  .meter i {
    height: 3px;
    background: var(--hair);
    transition: background 0.25s;
  }
  .meter[data-level='1'] i:nth-child(-n + 1) {
    background: var(--danger);
  }
  .meter[data-level='2'] i:nth-child(-n + 2) {
    background: var(--gold);
  }
  .meter[data-level='3'] i:nth-child(-n + 3) {
    background: var(--violet-hi);
  }
  .meter[data-level='4'] i {
    background: var(--ok);
  }
  .meter-label {
    display: block;
    margin-top: 0.5rem;
    font: 500 0.7rem/1 var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--dim);
  }
  .rules {
    list-style: none;
    margin: 1.1rem 0 0;
    padding: 0;
    display: grid;
    gap: 0.4rem;
    font-size: 0.86rem;
    color: var(--mute);
  }
  .rules li::before {
    content: '\25CB';
    margin-right: 0.6rem;
    color: var(--mute);
  }
  .rules li.met {
    color: var(--dim);
  }
  .rules li.met::before {
    content: '\25CF';
    color: var(--ok);
  }

  .keybox {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.4rem 1rem;
    margin: 1.7rem 0 0.9rem;
    padding: 1.15rem 1rem;
    border: 1px solid var(--line);
    background: rgba(205, 161, 95, 0.05);
    font: 500 clamp(0.95rem, 4.3vw, 1.2rem) / 1.4 var(--mono);
    letter-spacing: 0.12em;
    color: var(--gold-hi);
  }
  .keybox span:first-child {
    color: var(--mute);
  }
  .keyrow {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    min-height: 2.2rem;
  }
  .keyrow .note {
    font: 500 0.72rem/1 var(--mono);
    color: var(--ok);
  }

  .devices {
    list-style: none;
    margin: 1.7rem 0 1.9rem;
    padding: 0;
    border-top: 1px solid var(--hair);
  }
  .devices li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.85rem 0;
    border-bottom: 1px solid var(--hair);
    font-size: 0.92rem;
  }
  .meta-line {
    display: block;
    margin-top: 0.15rem;
    font: 500 0.68rem/1.3 var(--mono);
    letter-spacing: 0.04em;
    color: var(--mute);
  }
  .here {
    color: var(--ok);
  }

  .shake {
    animation: shake 0.45s cubic-bezier(0.36, 0.07, 0.19, 0.97);
  }
  @keyframes shake {
    10%,
    90% {
      transform: translateX(-2px);
    }
    20%,
    80% {
      transform: translateX(4px);
    }
    30%,
    50%,
    70% {
      transform: translateX(-7px);
    }
    40%,
    60% {
      transform: translateX(7px);
    }
  }

  /* landing hero */
  .hero {
    position: absolute;
    inset: 0;
    z-index: 4;
    display: grid;
    place-content: center;
    justify-items: center;
    text-align: center;
    padding: 6rem 1.2rem 8rem;
    pointer-events: none;
    transition:
      opacity 0.5s ease,
      visibility 0s linear 0s;
  }
  .hero > :global(*) {
    pointer-events: auto;
  }
  h1 {
    margin: 1.3rem 0 1.6rem;
    font: 300 clamp(2.4rem, 6vw, 5.2rem) / 1.06 var(--display);
    font-variation-settings: 'opsz' 144;
    letter-spacing: -0.012em;
    text-wrap: balance;
    max-width: 17ch;
    text-shadow:
      0 0 28px rgba(6, 6, 13, 0.95),
      0 0 4px rgba(6, 6, 13, 0.8);
  }
  h1 em {
    font-weight: 400;
    background: linear-gradient(100deg, #cda15f 20%, #f3ddb0 45%, #cda15f 70%);
    background-size: 220% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    filter: drop-shadow(0 0 16px rgba(6, 6, 13, 0.9));
    animation: shimmer 8s linear infinite;
  }
  @keyframes shimmer {
    to {
      background-position: -220% 0;
    }
  }
  .hero .lede {
    font-size: 1.02rem;
    max-width: 33rem;
    text-shadow:
      0 0 18px rgba(6, 6, 13, 0.95),
      0 0 3px rgba(6, 6, 13, 0.8);
  }
  .hero .eyebrow {
    text-shadow: 0 0 14px rgba(6, 6, 13, 0.95);
  }
  .meta {
    position: absolute;
    z-index: 4;
    left: 0;
    right: 0;
    bottom: 1.6rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.4rem 1.8rem;
    padding: 0 1rem;
    font: 500 0.66rem/1.4 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--mute);
    transition: opacity 0.5s ease;
  }
  .meta span::before {
    content: '\25C6';
    margin-right: 0.55rem;
    color: var(--gold);
    font-size: 0.55rem;
    vertical-align: 0.1em;
  }
  .stage:not([data-view='landing']) .hero,
  .stage:not([data-view='landing']) .meta {
    opacity: 0;
    visibility: hidden;
    transition:
      opacity 0.35s ease,
      visibility 0s linear 0.35s;
  }
  @media (max-height: 640px) {
    .hero {
      padding-top: 4.6rem;
      padding-bottom: 7rem;
    }
    .hero .lede {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .stage :global(*),
    .stage :global(*::before),
    .stage :global(*::after) {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
      transition-delay: 0s !important;
    }
  }
</style>
