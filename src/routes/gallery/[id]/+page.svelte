<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browser } from '$app/environment';
  import { activeConversationId, loadConversations } from '$lib/stores/chat';
  import { success, error as toastError } from '$lib/stores/toast';
  import MemoryGraph from '$lib/components/MemoryGraph.svelte';
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';

  const isTauri = browser && '__TAURI_INTERNALS__' in window;
  const charId = $derived($page.params.id);

  interface CharData {
    name: string; description: string; personality: string;
    scenario: string; first_mes: string; tags: string[];
    system_prompt: string;
  }

  interface ConvSummary { id: string; title: string; updated_at: string; }
  interface LoreEntry { id: string; keys: string[]; content: string; enabled: boolean; name?: string | null; }

  type Tab = 'profile' | 'memories' | 'lore' | 'stats' | 'edit';

  let charName = $state('');
  let charData = $state<CharData | null>(null);
  let avatarUrl = $state<string | null>(null);
  let isLoading = $state(true);
  let activeTab = $state<Tab>('profile');

  let conversations = $state<ConvSummary[]>([]);
  let totalMessages = $state(0);

  let memoryGraphData = $state<MemoryGraphData | null>(null);
  let isLoadingMemories = $state(false);
  let memoriesLoaded = $state(false);

  let loreEntries = $state<LoreEntry[]>([]);
  let isLoadingLore = $state(false);
  let loreLoaded = $state(false);

  let editName = $state(''); let editDesc = $state(''); let editPersonality = $state('');
  let editScenario = $state(''); let editFirstMes = $state('');
  let editSystemPrompt = $state(''); let editTags = $state('');
  let isSaving = $state(false);

  $effect(() => {
    const id = charId;
    if (id && isTauri) { loadCharacter(id); loadConvs(id); }
  });

  $effect(() => {
    if (activeTab === 'memories' && charId && isTauri) loadMemoryGraph(charId);
    if (activeTab === 'lore' && charId && isTauri) loadLore(charId);
    if (activeTab === 'edit' && charData) {
      editName = charName; editDesc = charData.description; editPersonality = charData.personality;
      editScenario = charData.scenario; editFirstMes = charData.first_mes;
      editSystemPrompt = charData.system_prompt; editTags = charData.tags.join(', ');
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
      };
      avatarUrl = await resolveAvatar(char.avatar_path);
    } catch {
      toastError('Failed to load character');
      goto('/gallery');
    }
    isLoading = false;
  }

  async function loadConvs(id: string) {
    try {
      const ipc = await import('$lib/services/ipc');
      const all = await ipc.listConversations(100, 0);
      const charConvs = all.filter((c: any) => c.character_id === id);
      conversations = charConvs.slice(0, 8).map((c: any) => ({ id: c.id, title: c.title, updated_at: c.updated_at }));
      totalMessages = charConvs.reduce((sum: number, c: any) => sum + (c.message_count ?? 0), 0);
    } catch { conversations = []; }
  }

  async function loadMemoryGraph(id: string) {
    if (memoriesLoaded || isLoadingMemories) return;
    isLoadingMemories = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const raw = await ipc.getMemoryGraph(id);
      const canonIds = new Set(raw.memories.filter((m: any) => m.is_canon).map((m: any) => m.id));
      memoryGraphData = {
        ...raw,
        memories: raw.memories.filter((m: any) => m.is_canon),
        links: raw.links.filter((l: any) => canonIds.has(l.source_memory_id)),
        conversations: [],
      };
    } catch { memoryGraphData = null; }
    isLoadingMemories = false;
    memoriesLoaded = true;
  }

  async function loadLore(id: string) {
    if (loreLoaded || isLoadingLore) return;
    isLoadingLore = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const raw = await ipc.listLorebookEntries(id);
      loreEntries = raw.map((e: any) => ({ id: e.id, keys: e.keys, content: e.content, enabled: e.enabled, name: e.name ?? null }));
    } catch { loreEntries = []; }
    isLoadingLore = false;
    loreLoaded = true;
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

  async function saveEdit() {
    if (!isTauri || !editName.trim()) return;
    isSaving = true;
    try {
      const ipc = await import('$lib/services/ipc');
      const tags = editTags.split(',').map((t: string) => t.trim()).filter(Boolean);
      await ipc.updateCharacter(charId!, editName, {
        description: editDesc, personality: editPersonality, scenario: editScenario,
        first_mes: editFirstMes, system_prompt: editSystemPrompt, tags,
      });
      charName = editName;
      charData = { ...charData!, description: editDesc, personality: editPersonality,
        scenario: editScenario, first_mes: editFirstMes, system_prompt: editSystemPrompt, tags };
      success('Character saved');
      activeTab = 'profile';
    } catch { toastError('Failed to save'); }
    isSaving = false;
  }

  function relativeTime(dateStr: string): string {
    const diff = Date.now() - new Date(dateStr).getTime();
    const h = Math.floor(diff / 3600000);
    if (h < 1) return 'Just now';
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  const accentColor = $derived(
    ['#8B5CF6','#00C2FF','#BF40FF','#F59E0B','#10B981','#F43F5E'][
      Math.abs((charName.charCodeAt(0) || 65) - 65) % 6
    ]
  );
  const initials = $derived(charName.split(' ').map(w => w[0]).join('').slice(0, 2).toUpperCase() || '?');
  const TABS: Tab[] = ['profile', 'memories', 'lore', 'stats', 'edit'];
</script>

<svelte:head><title>{charName || 'Character'} — Mythic</title></svelte:head>

<div class="profile-page">
  {#if isLoading}
    <div class="loading-state"><div class="loading-spinner"></div></div>
  {:else}

    <!-- Hero Panel -->
    <aside class="hero">
      <div class="hero-glow" style="background:radial-gradient(circle,{accentColor}33 0%,transparent 70%)"></div>
      <button class="back-btn" onclick={() => goto('/gallery')}>← Gallery</button>

      <div class="hero-av-wrap">
        <div class="hero-av" style="background:linear-gradient(135deg,{accentColor}99,{accentColor})">
          {#if avatarUrl}
            <img src={avatarUrl} alt={charName} class="hero-av-img" />
          {:else}
            <span class="hero-initials">{initials}</span>
          {/if}
        </div>
      </div>

      <h1 class="hero-name">{charName}</h1>
      {#if charData?.scenario}
        <p class="hero-tagline">{charData.scenario.slice(0, 90)}{charData.scenario.length > 90 ? '…' : ''}</p>
      {/if}

      {#if charData?.tags?.length}
        <div class="hero-tags">
          {#each charData.tags.slice(0, 4) as tag}
            <span class="hero-tag">{tag}</span>
          {/each}
        </div>
      {/if}

      <div class="hero-actions">
        <button class="btn-primary" id="profile-start-chat" onclick={startNewChat}>▶ Start New Chat</button>
        <button class="btn-outline" onclick={() => activeTab = 'edit'}>✏ Edit Character</button>
      </div>

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

    <!-- Main -->
    <main class="main">
      <nav class="tabs" aria-label="Character profile tabs">
        {#each TABS as tab}
          <button class="tab" class:active={activeTab === tab} onclick={() => activeTab = tab} id="tab-{tab}">
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        {/each}
      </nav>

      <div class="tab-body">

        <!-- Profile -->
        {#if activeTab === 'profile'}
          <div class="field-grid">
            {#if charData?.description}
              <div class="field-card full"><p class="field-label">Description</p><p class="field-text">{charData.description}</p></div>
            {/if}
            {#if charData?.personality}
              <div class="field-card"><p class="field-label">Personality</p><p class="field-text">{charData.personality}</p></div>
            {/if}
            {#if charData?.scenario}
              <div class="field-card"><p class="field-label">Scenario</p><p class="field-text">{charData.scenario}</p></div>
            {/if}
            {#if charData?.first_mes}
              <div class="field-card full"><p class="field-label">First Message</p><p class="field-text" style="font-style:italic">{charData.first_mes}</p></div>
            {/if}
            {#if !charData?.description && !charData?.personality}
              <p class="empty-tab">No profile data available.</p>
            {/if}
          </div>
        {/if}

        <!-- Memories -->
        {#if activeTab === 'memories'}
          <div class="memories-tab">
            <p class="tab-section-label">Canon Memories — shared across all conversations</p>
            {#if isLoadingMemories}
              <div class="tab-loading"><div class="loading-spinner"></div></div>
            {:else if memoryGraphData}
              <div class="memory-graph-wrap">
                <MemoryGraph
                  data={memoryGraphData}
                  avatars={charId ? { [charId]: avatarUrl } : {}}
                  onRefresh={() => { memoryGraphData = null; memoriesLoaded = false; if (charId) loadMemoryGraph(charId); }}
                />
              </div>
            {:else}
              <p class="empty-tab">No canon memories yet. Promote memories from conversation timelines.</p>
            {/if}
          </div>
        {/if}

        <!-- Lore -->
        {#if activeTab === 'lore'}
          {#if isLoadingLore}
            <div class="tab-loading"><div class="loading-spinner"></div></div>
          {:else if loreEntries.length === 0}
            <p class="empty-tab">No lorebook entries for this character.</p>
          {:else}
            <div class="lore-list">
              {#each loreEntries as entry (entry.id)}
                <div class="lore-entry" class:disabled={!entry.enabled}>
                  {#if entry.name}<p class="lore-name">{entry.name}</p>{/if}
                  <p class="lore-keys">🔑 {entry.keys.join(', ')}</p>
                  <p class="lore-content">{entry.content}</p>
                </div>
              {/each}
            </div>
          {/if}
        {/if}

        <!-- Stats -->
        {#if activeTab === 'stats'}
          <div class="stat-grid">
            <div class="stat-card"><p class="stat-val">{conversations.length}</p><p class="stat-lbl">Conversations</p></div>
            <div class="stat-card"><p class="stat-val">{totalMessages || '—'}</p><p class="stat-lbl">Messages</p></div>
            <div class="stat-card"><p class="stat-val">{memoryGraphData?.memories.length ?? '—'}</p><p class="stat-lbl">Canon Memories</p></div>
          </div>
          {#if conversations.length > 0}
            <div class="field-card full" style="margin-top:16px">
              <p class="field-label">Most Recent</p>
              <p class="field-text">{conversations[0].title || 'Untitled'} · {relativeTime(conversations[0].updated_at)}</p>
            </div>
          {/if}
        {/if}

        <!-- Edit -->
        {#if activeTab === 'edit'}
          <div class="edit-form">
            <div class="edit-field"><label class="edit-label" for="ef-name">Name *</label>
              <input id="ef-name" class="edit-input" bind:value={editName} placeholder="Character name" /></div>
            <div class="edit-field"><label class="edit-label" for="ef-desc">Description</label>
              <textarea id="ef-desc" class="edit-textarea" rows="4" bind:value={editDesc}></textarea></div>
            <div class="edit-field"><label class="edit-label" for="ef-pers">Personality</label>
              <textarea id="ef-pers" class="edit-textarea" rows="3" bind:value={editPersonality}></textarea></div>
            <div class="edit-field"><label class="edit-label" for="ef-sc">Scenario</label>
              <textarea id="ef-sc" class="edit-textarea" rows="2" bind:value={editScenario}></textarea></div>
            <div class="edit-field"><label class="edit-label" for="ef-fm">First Message</label>
              <textarea id="ef-fm" class="edit-textarea" rows="3" bind:value={editFirstMes}></textarea></div>
            <div class="edit-field"><label class="edit-label" for="ef-sys">System Prompt</label>
              <textarea id="ef-sys" class="edit-textarea" rows="2" bind:value={editSystemPrompt}></textarea></div>
            <div class="edit-field"><label class="edit-label" for="ef-tags">Tags (comma-separated)</label>
              <input id="ef-tags" class="edit-input" bind:value={editTags} placeholder="Fantasy, Adventure" /></div>
            <div class="edit-actions">
              <button class="btn-cancel" onclick={() => activeTab = 'profile'}>Cancel</button>
              <button class="btn-save" onclick={saveEdit} disabled={isSaving || !editName.trim()}>
                {isSaving ? 'Saving…' : 'Save Changes'}
              </button>
            </div>
          </div>
        {/if}

      </div>
    </main>
  {/if}
</div>

<style>
  .profile-page { flex:1; display:flex; height:100%; overflow:hidden; background:#09091a; }
  .loading-state { flex:1; display:flex; align-items:center; justify-content:center; }
  .loading-spinner { width:32px; height:32px; border-radius:50%; border:2px solid rgba(139,92,246,0.2); border-top-color:#8b5cf6; animation:spin 700ms linear infinite; }
  @keyframes spin { to { transform:rotate(360deg); } }

  /* Hero */
  .hero { width:268px; flex-shrink:0; background:linear-gradient(175deg,#180830 0%,#0e0e20 50%,#0a0a18 100%); border-right:1px solid rgba(139,92,246,0.12); display:flex; flex-direction:column; overflow:hidden; position:relative; }
  .hero-glow { position:absolute; top:80px; left:50%; transform:translateX(-50%); width:180px; height:180px; border-radius:50%; pointer-events:none; }
  .back-btn { background:none; border:none; color:#5a5a7a; font-size:11px; padding:14px 16px 0; cursor:pointer; text-align:left; transition:color 150ms; }
  .back-btn:hover { color:#c4a1ff; }
  .hero-av-wrap { display:flex; justify-content:center; padding:24px 0 16px; position:relative; z-index:1; }
  .hero-av { width:84px; height:84px; border-radius:50%; display:flex; align-items:center; justify-content:center; overflow:hidden; box-shadow:0 0 0 3px rgba(139,92,246,0.3),0 0 40px rgba(139,92,246,0.35); }
  .hero-av-img { width:100%; height:100%; object-fit:cover; }
  .hero-initials { font-size:28px; font-weight:800; color:rgba(255,255,255,0.9); }
  .hero-name { text-align:center; font-size:16px; font-weight:700; color:#f0eaff; letter-spacing:-0.3px; padding:0 16px; margin:0 0 6px; }
  .hero-tagline { text-align:center; font-size:11px; color:#6b5f8a; padding:0 20px; line-height:1.5; margin:0 0 12px; }
  .hero-tags { display:flex; gap:5px; justify-content:center; flex-wrap:wrap; padding:0 14px; margin-bottom:16px; }
  .hero-tag { font-size:10px; padding:3px 9px; border-radius:20px; background:rgba(139,92,246,0.12); border:1px solid rgba(139,92,246,0.25); color:#b09ee0; }
  .hero-actions { padding:0 14px; display:flex; flex-direction:column; gap:7px; margin-bottom:16px; }
  .btn-primary { height:36px; width:100%; background:linear-gradient(135deg,#7c3aed,#8b5cf6); border-radius:10px; border:none; color:#fff; font-size:13px; font-weight:600; cursor:pointer; box-shadow:0 4px 18px rgba(139,92,246,0.35); transition:box-shadow 180ms,transform 180ms; }
  .btn-primary:hover { box-shadow:0 6px 24px rgba(139,92,246,0.5); transform:translateY(-1px); }
  .btn-outline { height:32px; width:100%; background:transparent; border-radius:10px; border:1px solid rgba(139,92,246,0.25); color:#9070cc; font-size:12px; cursor:pointer; transition:background 150ms,border-color 150ms; }
  .btn-outline:hover { background:rgba(139,92,246,0.08); border-color:rgba(139,92,246,0.4); }
  .hero-divider { height:1px; background:rgba(255,255,255,0.05); margin:0 14px 12px; }
  .hero-section-label { font-size:9px; font-weight:700; letter-spacing:1.2px; color:#3a3a5a; text-transform:uppercase; padding:0 14px 6px; margin:0; }
  .conv-list { flex:1; overflow-y:auto; padding:0 8px 16px; display:flex; flex-direction:column; gap:2px; }
  .conv-list::-webkit-scrollbar { width:3px; }
  .conv-list::-webkit-scrollbar-thumb { background:rgba(139,92,246,0.15); border-radius:3px; }
  .conv-item { width:100%; background:none; border:none; border-radius:8px; padding:7px 10px; cursor:pointer; text-align:left; display:flex; flex-direction:column; gap:2px; transition:background 120ms; }
  .conv-item:hover { background:rgba(139,92,246,0.08); }
  .conv-title { font-size:11.5px; color:#c0b0d8; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .conv-meta { font-size:9.5px; color:#3a3a5a; font-family:monospace; }

  /* Main */
  .main { flex:1; display:flex; flex-direction:column; overflow:hidden; background:#0b0b18; }
  .tabs { display:flex; border-bottom:1px solid rgba(255,255,255,0.05); padding:0 20px; flex-shrink:0; }
  .tab { height:42px; background:none; border:none; border-bottom:2px solid transparent; padding:0 14px; font-size:12px; font-weight:500; color:#4a4a6a; cursor:pointer; transition:all 150ms; white-space:nowrap; font-family:inherit; }
  .tab.active { color:#c4a1ff; border-bottom-color:#8b5cf6; }
  .tab:hover:not(.active) { color:#8b8ba7; }
  .tab-body { flex:1; overflow-y:auto; padding:24px; }
  .tab-body::-webkit-scrollbar { width:4px; }
  .tab-body::-webkit-scrollbar-thumb { background:rgba(139,92,246,0.15); border-radius:4px; }

  /* Profile tab */
  .field-grid { display:grid; grid-template-columns:1fr 1fr; gap:14px; }
  .field-card { background:rgba(255,255,255,0.025); border:1px solid rgba(255,255,255,0.06); border-radius:12px; padding:14px 16px; }
  .field-card.full { grid-column:1/-1; }
  .field-label { font-size:9.5px; font-weight:700; letter-spacing:1px; color:#4a4a6a; text-transform:uppercase; margin:0 0 7px; }
  .field-text { font-size:12.5px; color:#8a8aaa; line-height:1.65; margin:0; }

  /* Memories tab */
  .memories-tab { display:flex; flex-direction:column; height:100%; gap:12px; }
  .tab-section-label { font-size:9.5px; font-weight:700; letter-spacing:1px; color:#4a4a6a; text-transform:uppercase; margin:0; }
  .memory-graph-wrap { flex:1; min-height:400px; border-radius:12px; overflow:hidden; border:1px solid rgba(139,92,246,0.1); }
  .tab-loading { display:flex; align-items:center; justify-content:center; padding:60px; }
  .empty-tab { color:#4a4a6a; font-size:13px; padding:40px 0; text-align:center; }

  /* Lore tab */
  .lore-list { display:flex; flex-direction:column; gap:10px; }
  .lore-entry { background:rgba(255,255,255,0.025); border:1px solid rgba(255,255,255,0.06); border-radius:10px; padding:12px 14px; }
  .lore-entry.disabled { opacity:0.45; }
  .lore-name { font-size:12px; font-weight:600; color:#c4b8e0; margin:0 0 4px; }
  .lore-keys { font-size:10.5px; color:#5a5a7a; font-family:monospace; margin:0 0 6px; }
  .lore-content { font-size:12px; color:#7c7c9a; line-height:1.55; margin:0; }

  /* Stats tab */
  .stat-grid { display:grid; grid-template-columns:repeat(3,1fr); gap:12px; }
  .stat-card { background:rgba(255,255,255,0.025); border:1px solid rgba(255,255,255,0.06); border-radius:12px; padding:18px; text-align:center; }
  .stat-val { font-size:28px; font-weight:800; background:linear-gradient(135deg,#c4a1ff,#8b5cf6); -webkit-background-clip:text; -webkit-text-fill-color:transparent; margin:0 0 4px; }
  .stat-lbl { font-size:10px; color:#4a4a6a; text-transform:uppercase; letter-spacing:0.5px; margin:0; }

  /* Edit tab */
  .edit-form { display:flex; flex-direction:column; gap:14px; max-width:680px; }
  .edit-field { display:flex; flex-direction:column; gap:5px; }
  .edit-label { font-size:9.5px; font-weight:700; letter-spacing:1px; color:#4a4a6a; text-transform:uppercase; }
  .edit-input { height:38px; padding:0 12px; border-radius:9px; background:rgba(14,14,30,0.6); border:1px solid rgba(139,92,246,0.1); color:#e0e0f0; font-size:13px; outline:none; transition:border-color 180ms; font-family:inherit; }
  .edit-input:focus { border-color:rgba(139,92,246,0.35); }
  .edit-textarea { padding:9px 12px; border-radius:9px; background:rgba(14,14,30,0.6); border:1px solid rgba(139,92,246,0.1); color:#e0e0f0; font-size:13px; line-height:1.6; resize:vertical; outline:none; transition:border-color 180ms; font-family:inherit; }
  .edit-textarea:focus { border-color:rgba(139,92,246,0.35); }
  .edit-actions { display:flex; justify-content:flex-end; gap:8px; padding-top:4px; }
  .btn-cancel { height:34px; padding:0 16px; background:transparent; border:1px solid rgba(255,255,255,0.08); border-radius:8px; color:#5a5a7a; font-size:12px; cursor:pointer; font-family:inherit; }
  .btn-save { height:34px; padding:0 20px; background:linear-gradient(135deg,#7c3aed,#8b5cf6); border:none; border-radius:8px; color:#fff; font-size:12px; font-weight:600; cursor:pointer; font-family:inherit; }
  .btn-save:disabled { opacity:0.5; cursor:not-allowed; }
</style>
