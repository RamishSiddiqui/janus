# Character Profile Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated `/gallery/[id]` route that shows a character's full profile — hero panel, 5 tabs (Profile / Memories / Lore / Stats / Edit) — replacing the direct "click → start chat" flow with "click → profile → start chat".

**Architecture:** New SvelteKit page at `src/routes/gallery/[id]/+page.svelte`. Gallery card `onclick` changes from `startChat()` to `goto('/gallery/' + id)`. The Memories tab reuses the existing `MemoryGraph` component filtered to `is_canon=true`. The Edit tab reuses the existing character editor logic extracted from `gallery/+page.svelte`. No new backend commands needed — all data comes from existing IPC calls.

**Tech Stack:** SvelteKit, Svelte 5 runes, existing `ipc.ts` (`getCharacter`, `listConversations`, `listMemories`, `listLorebookEntries`, `getCharacterState`, `updateCharacter`, `createConversation`), existing `MemoryGraph.svelte`.

**Branch:** `git checkout -b feature/character-profile-page`

---

## File Map

| Action | File |
|---|---|
| CREATE | `src/routes/gallery/[id]/+page.svelte` — the profile page |
| CREATE | `src/routes/gallery/[id]/+page.ts` — load function (SSR off) |
| MODIFY | `src/routes/gallery/+page.svelte` — change card click to `goto('/gallery/' + id)` |

No new backend commands. All data via existing IPC.

---

## Task 1: Branch + Route Scaffold

**Files:**
- Create: `src/routes/gallery/[id]/+page.ts`
- Create: `src/routes/gallery/[id]/+page.svelte` (skeleton only)
- Modify: `src/routes/gallery/+page.svelte` lines 356-360

- [ ] **Step 1: Create branch**

```bash
git checkout -b feature/character-profile-page
```

- [ ] **Step 2: Create `src/routes/gallery/[id]/+page.ts`**

```typescript
// Disable SSR — this page calls Tauri IPC which is browser-only
export const ssr = false;
export const prerender = false;
```

- [ ] **Step 3: Create skeleton `src/routes/gallery/[id]/+page.svelte`**

```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;
  const charId = $derived($page.params.id);
</script>

<svelte:head>
  <title>Character — Mythic</title>
</svelte:head>

<div class="profile-page">
  <p style="color:#8b8ba7;padding:40px">Loading character {charId}…</p>
</div>

<style>
  .profile-page {
    flex: 1; display: flex; height: 100%; overflow: hidden;
    background: linear-gradient(175deg, #0c0c1e, #09091a 60%, #07071a);
  }
</style>
```

- [ ] **Step 4: Change gallery card click to navigate to profile**

In `src/routes/gallery/+page.svelte`, find lines ~354-360:
```svelte
          onclick={() => startChat(char.id)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && startChat(char.id)}
```

Replace with:
```svelte
          onclick={() => goto('/gallery/' + char.id)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && goto('/gallery/' + char.id)}
```

- [ ] **Step 5: Verify route works**

```powershell
npx tauri dev
# Click any character card — should navigate to /gallery/<id> without crashing
# Expected: page shows "Loading character <id>…"
```

- [ ] **Step 6: Commit**

```bash
git add src/routes/gallery/[id]/+page.ts src/routes/gallery/[id]/+page.svelte src/routes/gallery/+page.svelte
git commit -m "feat(profile): scaffold /gallery/[id] route + wire gallery card navigation"
```

---

## Task 2: Hero Panel (left column)

**Files:**
- Modify: `src/routes/gallery/[id]/+page.svelte`

The hero panel loads character data and lists recent conversations. It has: avatar, name, tagline, tags, Start Chat button, Edit button, recent conversations list.

- [ ] **Step 1: Add data loading to `+page.svelte` script**

Replace the script block with:

```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  import { activeConversationId, loadConversations } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;
  const charId = $derived($page.params.id);

  // ── Character data ─────────────────────────────────────
  interface CharData {
    name: string;
    description: string;
    personality: string;
    scenario: string;
    first_mes: string;
    tags: string[];
    system_prompt: string;
    creator_notes: string;
    post_history_instructions: string;
  }

  let charName = $state('');
  let charData = $state<CharData | null>(null);
  let avatarUrl = $state<string | null>(null);
  let isLoading = $state(true);

  // ── Recent conversations ────────────────────────────────
  interface ConvSummary { id: string; title: string; updated_at: string; }
  let conversations = $state<ConvSummary[]>([]);

  // ── Active tab ──────────────────────────────────────────
  type Tab = 'profile' | 'memories' | 'lore' | 'stats' | 'edit';
  let activeTab = $state<Tab>('profile');

  $effect(() => {
    const id = charId;
    if (id && isTauri) {
      loadCharacter(id);
      loadConvs(id);
    }
  });

  async function resolveAvatar(avatarPath: string | null): Promise<string | null> {
    if (!avatarPath || !isTauri) return null;
    try {
      const { readFile, BaseDirectory } = await import('@tauri-apps/plugin-fs');
      const bytes = await readFile(avatarPath, { baseDir: BaseDirectory.AppData });
      const ext = avatarPath.split('.').pop()?.toLowerCase() || 'jpeg';
      const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg';
      return URL.createObjectURL(new Blob([bytes], { type: mime }));
    } catch { return null; }
  }

  async function loadCharacter(id: string) {
    isLoading = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const char = await ipc.getCharacter(id);
      charName = char.name;
      let parsed: Record<string, unknown> = {};
      try { parsed = JSON.parse(char.data); } catch {}
      charData = {
        name: char.name,
        description: (parsed.description as string) || '',
        personality: (parsed.personality as string) || '',
        scenario: (parsed.scenario as string) || '',
        first_mes: (parsed.first_mes as string) || '',
        tags: (parsed.tags as string[]) || [],
        system_prompt: (parsed.system_prompt as string) || '',
        creator_notes: (parsed.creator_notes as string) || '',
        post_history_instructions: (parsed.post_history_instructions as string) || '',
      };
      avatarUrl = await resolveAvatar(char.avatar_path);
    } catch (e) {
      toastError('Failed to load character');
      goto('/gallery');
    }
    isLoading = false;
  }

  async function loadConvs(id: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const all = await ipc.listConversations(10, 0);
      conversations = all
        .filter(c => c.character_id === id)
        .slice(0, 8)
        .map(c => ({ id: c.id, title: c.title, updated_at: c.updated_at }));
    } catch { conversations = []; }
  }

  async function startNewChat() {
    if (!isTauri) { goto('/'); return; }
    try {
      const ipc = await import('$lib/services/ipc');
      const conv = await ipc.createConversation(charId, charName);
      activeConversationId.set(conv.id);
      await loadConversations();
      goto('/');
    } catch { toastError('Failed to start chat'); }
  }

  async function resumeConversation(convId: string) {
    activeConversationId.set(convId);
    await loadConversations();
    goto('/');
  }

  function relativeTime(dateStr: string): string {
    const d = new Date(dateStr);
    const diff = Date.now() - d.getTime();
    const h = Math.floor(diff / 3600000);
    if (h < 1) return 'Just now';
    if (h < 24) return `${h}h ago`;
    const days = Math.floor(h / 24);
    return `${days}d ago`;
  }

  const accentColor = $derived(
    charData?.tags?.[0]
      ? ['#8B5CF6','#00C2FF','#BF40FF','#F59E0B','#10B981','#F43F5E'][
          Math.abs(charName.charCodeAt(0) - 65) % 6
        ]
      : '#8B5CF6'
  );
  const initials = $derived(charName.split(' ').map(w => w[0]).join('').slice(0, 2).toUpperCase());
</script>
```

- [ ] **Step 2: Add hero panel HTML + tabs skeleton**

Replace the template with:

```svelte
<svelte:head>
  <title>{charName || 'Character'} — Mythic</title>
</svelte:head>

<div class="profile-page">
  {#if isLoading}
    <div class="loading-state">
      <div class="loading-spinner"></div>
    </div>
  {:else}

    <!-- ── Hero Panel ──────────────────────────────────── -->
    <aside class="hero" style="--accent: {accentColor}">
      <div class="hero-glow"></div>

      <!-- Back -->
      <button class="back-btn" onclick={() => goto('/gallery')} aria-label="Back to gallery">
        ← Gallery
      </button>

      <!-- Avatar -->
      <div class="hero-av-wrap">
        <div class="hero-av" style="background: linear-gradient(135deg, {accentColor}cc, {accentColor})">
          {#if avatarUrl}
            <img src={avatarUrl} alt={charName} class="hero-av-img" />
          {:else}
            <span class="hero-initials">{initials}</span>
          {/if}
        </div>
        <div class="hero-av-ring"></div>
      </div>

      <h1 class="hero-name">{charName}</h1>
      {#if charData?.scenario}
        <p class="hero-tagline">{charData.scenario.slice(0, 90)}{charData.scenario.length > 90 ? '…' : ''}</p>
      {/if}

      <!-- Tags -->
      {#if charData?.tags?.length}
        <div class="hero-tags">
          {#each charData.tags.slice(0, 4) as tag}
            <span class="hero-tag">{tag}</span>
          {/each}
        </div>
      {/if}

      <!-- Actions -->
      <div class="hero-actions">
        <button class="btn-primary" id="profile-start-chat" onclick={startNewChat}>
          ▶ Start New Chat
        </button>
        <button class="btn-outline" onclick={() => activeTab = 'edit'}>
          ✏ Edit Character
        </button>
      </div>

      <!-- Recent conversations -->
      {#if conversations.length > 0}
        <div class="hero-divider"></div>
        <p class="hero-section-label">RECENT CONVERSATIONS</p>
        <div class="conv-list">
          {#each conversations as conv (conv.id)}
            <button class="conv-item" onclick={() => resumeConversation(conv.id)}>
              <span class="conv-title">{conv.title || 'Untitled'}</span>
              <span class="conv-meta">{relativeTime(conv.updated_at)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </aside>

    <!-- ── Main Content ────────────────────────────────── -->
    <main class="main">
      <!-- Tabs -->
      <nav class="tabs" aria-label="Character profile tabs">
        {#each (['profile','memories','lore','stats','edit'] as Tab[]) as tab}
          <button
            class="tab"
            class:active={activeTab === tab}
            onclick={() => activeTab = tab}
            aria-selected={activeTab === tab}
            id="tab-{tab}"
          >
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        {/each}
      </nav>

      <div class="tab-body">
        <!-- PROFILE TAB placeholder — filled in Task 3 -->
        {#if activeTab === 'profile'}
          <p style="color:#5a5a7a;padding:20px">Profile tab</p>
        {/if}

        <!-- MEMORIES placeholder — filled in Task 4 -->
        {#if activeTab === 'memories'}
          <p style="color:#5a5a7a;padding:20px">Memories tab</p>
        {/if}

        <!-- LORE placeholder — filled in Task 5 -->
        {#if activeTab === 'lore'}
          <p style="color:#5a5a7a;padding:20px">Lore tab</p>
        {/if}

        <!-- STATS placeholder — filled in Task 6 -->
        {#if activeTab === 'stats'}
          <p style="color:#5a5a7a;padding:20px">Stats tab</p>
        {/if}

        <!-- EDIT placeholder — filled in Task 7 -->
        {#if activeTab === 'edit'}
          <p style="color:#5a5a7a;padding:20px">Edit tab</p>
        {/if}
      </div>
    </main>

  {/if}
</div>
```

- [ ] **Step 3: Add CSS for hero panel + shell**

```svelte
<style>
  .profile-page {
    flex: 1; display: flex; height: 100%; overflow: hidden;
    background: #09091a;
  }
  .loading-state {
    flex: 1; display: flex; align-items: center; justify-content: center;
  }
  .loading-spinner {
    width: 32px; height: 32px; border-radius: 50%;
    border: 2px solid rgba(139,92,246,0.2);
    border-top-color: #8b5cf6;
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Hero */
  .hero {
    width: 268px; flex-shrink: 0;
    background: linear-gradient(175deg, #180830 0%, #0e0e20 50%, #0a0a18 100%);
    border-right: 1px solid rgba(139,92,246,0.12);
    display: flex; flex-direction: column;
    overflow: hidden; position: relative;
  }
  .hero-glow {
    position: absolute; top: 80px; left: 50%; transform: translateX(-50%);
    width: 180px; height: 180px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.2) 0%, transparent 70%);
    pointer-events: none;
  }
  .back-btn {
    background: none; border: none; color: #5a5a7a; font-size: 11px;
    padding: 14px 16px 0; cursor: pointer; text-align: left;
    transition: color 150ms;
  }
  .back-btn:hover { color: #c4a1ff; }
  .hero-av-wrap {
    display: flex; justify-content: center;
    padding: 24px 0 16px; position: relative; z-index: 1;
  }
  .hero-av {
    width: 84px; height: 84px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    position: relative; overflow: hidden;
    box-shadow: 0 0 0 3px rgba(139,92,246,0.3), 0 0 40px rgba(139,92,246,0.35);
  }
  .hero-av-img { width: 100%; height: 100%; object-fit: cover; }
  .hero-initials { font-size: 28px; font-weight: 800; color: rgba(255,255,255,0.9); }
  .hero-name {
    text-align: center; font-size: 16px; font-weight: 700;
    color: #f0eaff; letter-spacing: -0.3px; padding: 0 16px;
    margin: 0 0 6px;
  }
  .hero-tagline {
    text-align: center; font-size: 11px; color: #6b5f8a;
    padding: 0 20px; line-height: 1.5; margin: 0 0 12px;
  }
  .hero-tags {
    display: flex; gap: 5px; justify-content: center;
    flex-wrap: wrap; padding: 0 14px; margin-bottom: 16px;
  }
  .hero-tag {
    font-size: 10px; padding: 3px 9px; border-radius: 20px;
    background: rgba(139,92,246,0.12); border: 1px solid rgba(139,92,246,0.25);
    color: #b09ee0;
  }
  .hero-actions {
    padding: 0 14px; display: flex; flex-direction: column; gap: 7px;
    margin-bottom: 16px;
  }
  .btn-primary {
    height: 36px; width: 100%;
    background: linear-gradient(135deg, #7c3aed, #8b5cf6);
    border-radius: 10px; border: none; color: #fff;
    font-size: 13px; font-weight: 600; cursor: pointer;
    box-shadow: 0 4px 18px rgba(139,92,246,0.35);
    transition: box-shadow 180ms, transform 180ms;
  }
  .btn-primary:hover { box-shadow: 0 6px 24px rgba(139,92,246,0.5); transform: translateY(-1px); }
  .btn-outline {
    height: 32px; width: 100%; background: transparent;
    border-radius: 10px; border: 1px solid rgba(139,92,246,0.25);
    color: #9070cc; font-size: 12px; cursor: pointer;
    transition: background 150ms, border-color 150ms;
  }
  .btn-outline:hover { background: rgba(139,92,246,0.08); border-color: rgba(139,92,246,0.4); }
  .hero-divider { height: 1px; background: rgba(255,255,255,0.05); margin: 0 14px 12px; }
  .hero-section-label {
    font-size: 9px; font-weight: 700; letter-spacing: 1.2px;
    color: #3a3a5a; text-transform: uppercase; padding: 0 14px 6px; margin: 0;
  }
  .conv-list {
    flex: 1; overflow-y: auto; padding: 0 8px 16px;
    display: flex; flex-direction: column; gap: 2px;
  }
  .conv-list::-webkit-scrollbar { width: 3px; }
  .conv-list::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 3px; }
  .conv-item {
    width: 100%; background: none; border: none; border-radius: 8px;
    padding: 7px 10px; cursor: pointer; text-align: left;
    display: flex; flex-direction: column; gap: 2px;
    transition: background 120ms;
  }
  .conv-item:hover { background: rgba(139,92,246,0.08); }
  .conv-title { font-size: 11.5px; color: #c0b0d8; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .conv-meta { font-size: 9.5px; color: #3a3a5a; font-family: monospace; }

  /* Main */
  .main { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: #0b0b18; }
  .tabs {
    display: flex; border-bottom: 1px solid rgba(255,255,255,0.05);
    padding: 0 20px; flex-shrink: 0;
  }
  .tab {
    height: 42px; background: none; border: none;
    border-bottom: 2px solid transparent;
    padding: 0 14px; font-size: 12px; font-weight: 500;
    color: #4a4a6a; cursor: pointer; transition: all 150ms; white-space: nowrap;
  }
  .tab.active { color: #c4a1ff; border-bottom-color: #8b5cf6; }
  .tab:hover:not(.active) { color: #8b8ba7; }
  .tab-body { flex: 1; overflow-y: auto; padding: 24px; }
  .tab-body::-webkit-scrollbar { width: 4px; }
  .tab-body::-webkit-scrollbar-thumb { background: rgba(139,92,246,0.15); border-radius: 4px; }
</style>
```

- [ ] **Step 4: svelte-check — 0 errors**

```powershell
npm run check
# Expected: 0 errors
</div>
