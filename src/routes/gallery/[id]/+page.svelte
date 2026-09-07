<script lang="ts">
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { browser } from "$app/environment";
  import { activeConversationId, loadConversations } from "$lib/stores/chat";
  import { success, error as toastError } from "$lib/stores/toast";
  import { parseCharacterData } from "$lib/utils/character";
  import MemoryGraph from "$lib/components/MemoryGraph.svelte";
  import PersonaPicker from "$lib/components/PersonaPicker.svelte";
  import SplitHeading from "$lib/components/SplitHeading.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import WaveformBars from "$lib/components/WaveformBars.svelte";
  import { selectedPersonaId } from "$lib/stores/personas";
  import { get } from "svelte/store";
  import { playBase64Wav, primeAudioPreviewContext } from "$lib/utils/audioPreview";
  import type { MemoryGraph as MemoryGraphData, VoiceInfo } from "$lib/services/ipc";

  const isTauri = browser && "__TAURI_INTERNALS__" in window;
  const charId = $derived($page.params.id);

  interface CharData {
    name: string;
    description: string;
    personality: string;
    scenario: string;
    first_mes: string;
    tags: string[];
    system_prompt: string;
  }

  interface ConvSummary {
    id: string;
    title: string;
    updated_at: string;
  }
  interface LoreEntry {
    id: string;
    keys: string[];
    content: string;
    enabled: boolean;
    name?: string | null;
  }

  type Tab = "profile" | "memories" | "lore" | "stats" | "edit";

  let charName = $state("");
  let charData = $state<CharData | null>(null);
  let avatarUrl = $state<string | null>(null);
  let isLoading = $state(true);
  let activeTab = $state<Tab>("profile");

  let conversations = $state<ConvSummary[]>([]);
  let totalMessages = $state(0);

  let memoryGraphData = $state<MemoryGraphData | null>(null);
  let isLoadingMemories = $state(false);
  let memoriesLoaded = $state(false);

  let loreEntries = $state<LoreEntry[]>([]);
  let isLoadingLore = $state(false);
  let loreLoaded = $state(false);

  let editName = $state("");
  let editDesc = $state("");
  let editPersonality = $state("");
  let editScenario = $state("");
  let editFirstMes = $state("");
  let editSystemPrompt = $state("");
  let editTags = $state("");
  let isSaving = $state(false);

  let voiceId = $state<string | null>(null);
  let voices = $state<VoiceInfo[]>([]);
  let voicesLoaded = $state(false);
  let savingVoice = $state(false);
  let previewingVoice = $state(false);
  let voiceName = $derived(
    voices.find((v) => v.id === voiceId)?.name ?? voiceId ?? "",
  );
  let previewAnalyser = $state<AnalyserNode | null>(null);

  $effect(() => {
    const id = charId;
    if (id && isTauri) {
      loadCharacter(id);
      loadConvs(id);
    }
  });

  $effect(() => {
    if (activeTab === "memories" && charId && isTauri) loadMemoryGraph(charId);
    if (activeTab === "lore" && charId && isTauri) loadLore(charId);
    // Load lazily (not on every character page view) so browsing characters
    // that don't use TTS never triggers an engine load — but do it whenever
    // a voice is actually assigned (not just on the Edit tab) so the
    // sidebar's "Change" flow and voice-name display work without forcing
    // the user into Edit first.
    if ((activeTab === "edit" || voiceId) && isTauri) loadVoices();
    if (activeTab === "edit" && charData) {
      editName = charName;
      editDesc = charData.description;
      editPersonality = charData.personality;
      editScenario = charData.scenario;
      editFirstMes = charData.first_mes;
      editSystemPrompt = charData.system_prompt;
      editTags = charData.tags.join(", ");
    }
  });

  async function resolveAvatar(
    avatarPath: string | null,
  ): Promise<string | null> {
    if (!avatarPath || !isTauri) return null;
    try {
      const { loadFileAsBlobUrl } = await import("$lib/utils/blobUrl");
      return await loadFileAsBlobUrl(avatarPath);
    } catch {
      return null;
    }
  }

  async function loadCharacter(id: string) {
    isLoading = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const char = await ipc.getCharacter(id);
      charName = char.name;
      const parsed = parseCharacterData(char.data);
      charData = {
        name: char.name,
        description: (parsed.description as string) || "",
        personality: (parsed.personality as string) || "",
        scenario: (parsed.scenario as string) || "",
        first_mes: (parsed.first_mes as string) || "",
        tags: (parsed.tags as string[]) || [],
        system_prompt: (parsed.system_prompt as string) || "",
      };
      avatarUrl = await resolveAvatar(char.avatar_path);
      voiceId = char.voice_id;
    } catch {
      toastError("Failed to load character");
      goto("/gallery");
    }
    isLoading = false;
  }

  async function loadConvs(id: string) {
    try {
      const ipc = await import("$lib/services/ipc");
      const all = await ipc.listConversations(100, 0);
      const charConvs = all.filter((c: any) => c.character_id === id);
      conversations = charConvs
        .slice(0, 8)
        .map((c: any) => ({
          id: c.id,
          title: c.title,
          updated_at: c.updated_at,
        }));
      totalMessages = charConvs.reduce(
        (sum: number, c: any) => sum + (c.message_count ?? 0),
        0,
      );
    } catch {
      conversations = [];
    }
  }

  async function loadMemoryGraph(id: string) {
    if (memoriesLoaded || isLoadingMemories) return;
    isLoadingMemories = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const raw = await ipc.getMemoryGraph(id);
      const canonIds = new Set(
        raw.memories.filter((m: any) => m.is_canon).map((m: any) => m.id),
      );
      memoryGraphData = {
        ...raw,
        memories: raw.memories.filter((m: any) => m.is_canon),
        links: raw.links.filter((l: any) => canonIds.has(l.source_memory_id)),
        conversations: [],
      };
    } catch {
      memoryGraphData = null;
    }
    isLoadingMemories = false;
    memoriesLoaded = true;
  }

  async function loadLore(id: string) {
    if (loreLoaded || isLoadingLore) return;
    isLoadingLore = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const raw = await ipc.listLorebookEntries(id);
      loreEntries = raw.map((e: any) => ({
        id: e.id,
        keys: e.keys,
        content: e.content,
        enabled: e.enabled,
        name: e.name ?? null,
      }));
    } catch {
      loreEntries = [];
    }
    isLoadingLore = false;
    loreLoaded = true;
  }

  async function loadVoices() {
    if (voicesLoaded || !isTauri) return;
    try {
      const ipc = await import("$lib/services/ipc");
      voices = await ipc.ttsListVoices();
      voicesLoaded = true;
    } catch {
      voices = [];
    }
  }

  async function handleVoiceChange() {
    if (!isTauri || !charId) return;
    savingVoice = true;
    try {
      const ipc = await import("$lib/services/ipc");
      await ipc.ttsSetCharacterVoice(charId, voiceId);
    } catch {
      toastError("Failed to save voice");
    }
    savingVoice = false;
  }

  async function previewVoice() {
    if (!isTauri || !voiceId || previewingVoice) return;
    // Must run synchronously here, before the first `await` below — see
    // the doc comment on primeAudioPreviewContext for why.
    primeAudioPreviewContext();
    previewingVoice = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const audio = await ipc.ttsTestSpeak(
        `Hello, I'm ${editName || charName}.`,
        voiceId,
      );
      const playback = await playBase64Wav(audio);
      previewAnalyser = playback.analyser;
      await playback.ended;
    } catch (err) {
      toastError("Preview failed");
      console.error(err);
    }
    previewAnalyser = null;
    previewingVoice = false;
  }

  async function startNewChat() {
    if (!isTauri) {
      goto("/");
      return;
    }
    try {
      const { createConversation } = await import("$lib/stores/chat");
      const personaId = get(selectedPersonaId) ?? undefined;
      await createConversation(charId ?? '', charName ?? '', personaId);
      goto("/");
    } catch {
      toastError("Failed to start chat");
    }
  }

  async function resumeConversation(convId: string) {
    activeConversationId.set(convId);
    await loadConversations();
    goto("/");
  }

  async function saveEdit() {
    if (!isTauri || !editName.trim()) return;
    isSaving = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const tags = editTags
        .split(",")
        .map((t: string) => t.trim())
        .filter(Boolean);
      await ipc.updateCharacter(charId!, editName, {
        description: editDesc,
        personality: editPersonality,
        scenario: editScenario,
        first_mes: editFirstMes,
        system_prompt: editSystemPrompt,
        tags,
      });
      charName = editName;
      charData = {
        ...charData!,
        description: editDesc,
        personality: editPersonality,
        scenario: editScenario,
        first_mes: editFirstMes,
        system_prompt: editSystemPrompt,
        tags,
      };
      success("Character saved");
      activeTab = "profile";
    } catch {
      toastError("Failed to save");
    }
    isSaving = false;
  }

  function relativeTime(dateStr: string): string {
    const date = new Date(dateStr);
    if (isNaN(date.getTime())) return "";
    const diff = Date.now() - date.getTime();
    const h = Math.floor(diff / 3600000);
    if (h < 1) return "Just now";
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  const accentColor = $derived(
    ["#8B5CF6", "#00C2FF", "#BF40FF", "#F59E0B", "#10B981", "#F43F5E"][
      Math.abs((charName.charCodeAt(0) || 65) - 65) % 6
    ],
  );
  const initials = $derived(
    charName
      .split(" ")
      .map((w) => w[0])
      .join("")
      .slice(0, 2)
      .toUpperCase() || "?",
  );
  const TABS: Tab[] = ["profile", "memories", "lore", "stats", "edit"];
  const TAB_ICONS: Record<Tab, string> = {
    profile: "user",
    memories: "brain",
    lore: "book-open",
    stats: "bar-chart-3",
    edit: "pencil",
  };
  const TAG_COLORS = ["#9075f2", "#22d3ee", "#e879f9", "#fbbf24", "#34d399", "#fb7185"];
</script>

<svelte:head><title>{charName || "Character"} — Janus</title></svelte:head>

<div class="profile-page">
  {#if isLoading}
    <div class="loading-state"><div class="loading-spinner"></div></div>
  {:else}
    <!-- Hero Panel -->
    <aside class="hero">
      <div
        class="hero-glow"
        style="background:radial-gradient(circle,{accentColor}33 0%,transparent 70%)"
      ></div>
      <button class="back-btn" onclick={() => goto("/gallery")}
        >← Gallery</button
      >

      <div class="hero-av-wrap">
        <div
          class="hero-av-glow"
          style="background:radial-gradient(circle,{accentColor}55,transparent 70%)"
        ></div>
        <div
          class="hero-av-ring"
          style="background:conic-gradient(from 200deg,{accentColor},#CDA15F,{accentColor})"
        >
          <div
            class="hero-av"
            style="background:linear-gradient(135deg,{accentColor}99,{accentColor})"
          >
            {#if avatarUrl}
              <img src={avatarUrl} alt={charName} class="hero-av-img" />
            {:else}
              <span class="hero-initials">{initials}</span>
            {/if}
          </div>
        </div>
      </div>

      <h1 class="hero-name"><SplitHeading text={charName} violetColor={accentColor} /></h1>
      {#if charData?.scenario}
        <p class="hero-tagline">
          {charData.scenario.slice(0, 90)}{charData.scenario.length > 90
            ? "…"
            : ""}
        </p>
      {/if}

      {#if charData?.tags?.length}
        <div class="hero-tags">
          {#each charData.tags.slice(0, 4) as tag, i}
            <span class="hero-tag">
              <span class="hero-tag-dot" style="background:{TAG_COLORS[i % TAG_COLORS.length]}"></span>
              {tag}
            </span>
          {/each}
        </div>
      {/if}

      <div class="hero-voice">
        <Icon name="volume-2" size={12} color="var(--fg-muted)" />
        <span class="hero-voice-name">{voiceId ? voiceName : "Default voice"}</span>
        {#if voiceId}
          <button
            class="hero-voice-preview"
            onclick={previewVoice}
            disabled={previewingVoice}
            title="Preview voice"
          >
            {#if previewingVoice}
              <WaveformBars analyser={previewAnalyser} active={previewingVoice} color="var(--accent-primary, #8b5cf6)" />
            {:else}
              ▶
            {/if}
          </button>
        {/if}
        <button
          class="hero-voice-edit"
          onclick={() => (activeTab = "edit")}
          title="Change voice"
        >
          Change
        </button>
      </div>

      <div class="hero-actions">
        <PersonaPicker />
        <button
          class="btn-primary"
          id="profile-start-chat"
          onclick={startNewChat}>▶ Start New Chat</button
        >
        <button class="btn-outline" onclick={() => (activeTab = "edit")}
          >✏ Edit Character</button
        >
      </div>

      {#if conversations.length > 0}
        <div class="hero-divider"></div>
        <p class="hero-section-label">RECENT CONVERSATIONS</p>
        <div class="conv-list">
          {#each conversations as conv (conv.id)}
            <button
              class="conv-item"
              onclick={() => resumeConversation(conv.id)}
            >
              <span class="conv-dot" style="background:{accentColor}"></span>
              <span class="conv-item-body">
                <span class="conv-title">{conv.title || "Untitled"}</span>
                <span class="conv-meta">{relativeTime(conv.updated_at)}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </aside>

    <!-- Main -->
    <main class="main">
      <nav class="tabs" aria-label="Character profile tabs">
        {#each TABS as tab}
          <button
            class="tab"
            class:active={activeTab === tab}
            style={activeTab === tab ? `background:${accentColor};border-color:${accentColor};box-shadow:0 8px 22px -8px ${accentColor}` : ''}
            onclick={() => (activeTab = tab)}
            id="tab-{tab}"
          >
            <Icon name={TAB_ICONS[tab]} size={14} color={activeTab === tab ? '#0a0812' : 'var(--fg-muted)'} />
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        {/each}
      </nav>

      <div class="tab-body">
        <!-- Profile -->
        {#if activeTab === "profile"}
          <div class="field-grid">
            {#if charData?.description}
              <div class="field-card full accent-violet">
                <p class="field-label">Description</p>
                <p class="field-text">{charData.description}</p>
              </div>
            {/if}
            {#if charData?.personality}
              <div class="field-card accent-fuchsia">
                <p class="field-label">Personality</p>
                <p class="field-text">{charData.personality}</p>
              </div>
            {/if}
            {#if charData?.scenario}
              <div class="field-card accent-amber">
                <p class="field-label">Scenario</p>
                <p class="field-text">{charData.scenario}</p>
              </div>
            {/if}
            {#if charData?.first_mes}
              <div class="field-card full accent-cyan quote-card">
                <p class="field-label">First Message</p>
                <p class="field-text" style="font-style:italic">
                  {charData.first_mes}
                </p>
              </div>
            {/if}
            {#if !charData?.description && !charData?.personality}
              <p class="empty-tab">No profile data available.</p>
            {/if}
          </div>
        {/if}

        <!-- Memories -->
        {#if activeTab === "memories"}
          <div class="memories-tab">
            <p class="tab-section-label">
              Canon Memories — shared across all conversations
            </p>
            {#if isLoadingMemories}
              <div class="tab-loading"><div class="loading-spinner"></div></div>
            {:else if memoryGraphData}
              <div class="memory-graph-wrap">
                <MemoryGraph
                  data={memoryGraphData}
                  avatars={charId ? { [charId]: avatarUrl } : {}}
                  onRefresh={() => {
                    memoryGraphData = null;
                    memoriesLoaded = false;
                    if (charId) loadMemoryGraph(charId);
                  }}
                />
              </div>
            {:else}
              <p class="empty-tab">
                No canon memories yet. Promote memories from conversation
                timelines.
              </p>
            {/if}
          </div>
        {/if}

        <!-- Lore -->
        {#if activeTab === "lore"}
          {#if isLoadingLore}
            <div class="tab-loading"><div class="loading-spinner"></div></div>
          {:else if loreEntries.length === 0}
            <p class="empty-tab">No lorebook entries for this character.</p>
          {:else}
            <div class="lore-list">
              {#each loreEntries as entry (entry.id)}
                <div class="lore-entry" class:disabled={!entry.enabled}>
                  {#if entry.name}<p class="lore-name">{entry.name}</p>{/if}
                  <p class="lore-keys">🔑 {entry.keys.join(", ")}</p>
                  <p class="lore-content">{entry.content}</p>
                </div>
              {/each}
            </div>
          {/if}
        {/if}

        <!-- Stats -->
        {#if activeTab === "stats"}
          <div class="stat-grid">
            <div class="stat-card">
              <p class="stat-val">{conversations.length}</p>
              <p class="stat-lbl">Conversations</p>
            </div>
            <div class="stat-card">
              <p class="stat-val">{totalMessages || "—"}</p>
              <p class="stat-lbl">Messages</p>
            </div>
            <div class="stat-card">
              <p class="stat-val">{memoryGraphData?.memories.length ?? "—"}</p>
              <p class="stat-lbl">Canon Memories</p>
            </div>
          </div>
          {#if conversations.length > 0}
            <div class="field-card full" style="margin-top:16px">
              <p class="field-label">Most Recent</p>
              <p class="field-text">
                {conversations[0].title || "Untitled"} · {relativeTime(
                  conversations[0].updated_at,
                )}
              </p>
            </div>
          {/if}
        {/if}

        <!-- Edit -->
        {#if activeTab === "edit"}
          <div class="edit-form">
            <div class="edit-field">
              <label class="edit-label" for="ef-name">Name *</label>
              <input
                id="ef-name"
                class="edit-input"
                bind:value={editName}
                placeholder="Character name"
              />
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-desc">Description</label>
              <textarea
                id="ef-desc"
                class="edit-textarea"
                rows="4"
                bind:value={editDesc}
              ></textarea>
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-pers">Personality</label>
              <textarea
                id="ef-pers"
                class="edit-textarea"
                rows="3"
                bind:value={editPersonality}
              ></textarea>
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-sc">Scenario</label>
              <textarea
                id="ef-sc"
                class="edit-textarea"
                rows="2"
                bind:value={editScenario}
              ></textarea>
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-fm">First Message</label>
              <textarea
                id="ef-fm"
                class="edit-textarea"
                rows="3"
                bind:value={editFirstMes}
              ></textarea>
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-sys">System Prompt</label>
              <textarea
                id="ef-sys"
                class="edit-textarea"
                rows="2"
                bind:value={editSystemPrompt}
              ></textarea>
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-tags"
                >Tags (comma-separated)</label
              >
              <input
                id="ef-tags"
                class="edit-input"
                bind:value={editTags}
                placeholder="Fantasy, Adventure"
              />
            </div>
            <div class="edit-field">
              <label class="edit-label" for="ef-voice">Voice</label>
              <div class="edit-actions" style="justify-content: flex-start; gap: 8px;">
                <select
                  id="ef-voice"
                  class="edit-input"
                  bind:value={voiceId}
                  onchange={handleVoiceChange}
                  disabled={savingVoice}
                >
                  <option value={null}>Use default voice</option>
                  {#each voices as v (v.id)}
                    <option value={v.id}>{v.name}</option>
                  {/each}
                </select>
                <button
                  class="btn-cancel"
                  onclick={previewVoice}
                  disabled={!voiceId || previewingVoice}
                  type="button"
                  style="display:inline-flex;align-items:center;gap:6px;"
                >
                  {#if previewingVoice}
                    <WaveformBars analyser={previewAnalyser} active={previewingVoice} color="var(--accent-primary, #8b5cf6)" />
                  {/if}
                  {previewingVoice ? "Playing…" : "Preview"}
                </button>
              </div>
            </div>
            <div class="edit-actions">
              <button class="btn-cancel" onclick={() => (activeTab = "profile")}
                >Cancel</button
              >
              <button
                class="btn-save"
                onclick={saveEdit}
                disabled={isSaving || !editName.trim()}
              >
                {isSaving ? "Saving…" : "Save Changes"}
              </button>
            </div>
          </div>
        {/if}
      </div>
    </main>
  {/if}
</div>

<style>
  /* ── Layout shell ── */
  .profile-page {
    flex:1; display:flex; height:100%; overflow:hidden;
    background:linear-gradient(160deg,#0c0b1e 0%,#07071a 55%,#080816 100%);
    position:relative;
  }
  .profile-page::before {
    content:''; position:absolute; inset:0; pointer-events:none;
    background:radial-gradient(ellipse 60% 40% at 70% 10%,rgba(139,92,246,0.05) 0%,transparent 70%);
  }

  .loading-state { flex:1; display:flex; align-items:center; justify-content:center; }
  .loading-spinner {
    width:30px; height:30px; border-radius:50%;
    border:2px solid rgba(139,92,246,0.15); border-top-color:var(--accent-primary);
    animation:spin 700ms linear infinite;
  }
  @keyframes spin { to { transform:rotate(360deg); } }

  /* ── Hero ── */
  .hero {
    width:272px; flex-shrink:0; position:relative; z-index:1;
    background:linear-gradient(180deg,#110f24 0%,#0c0b1e 50%,#07071a 100%);
    border-right:1px solid rgba(139,92,246,0.1);
    display:flex; flex-direction:column; overflow:hidden;
  }
  .hero::after {
    content:''; position:absolute; top:0; right:0; width:1px; height:35%;
    background:linear-gradient(180deg,transparent,rgba(139,92,246,0.35),transparent);
    pointer-events:none;
  }
  .hero-glow {
    position:absolute; top:0; left:50%; transform:translateX(-50%);
    width:240px; height:200px; border-radius:50%; pointer-events:none; opacity:0.5;
  }
  .back-btn {
    background:none; border:none; color:var(--fg-muted);
    font-size:11px; letter-spacing:0.03em; padding:14px 18px 0;
    cursor:pointer; text-align:left; transition:color var(--duration-fast);
    display:flex; align-items:center; gap:5px;
  }
  .back-btn:hover { color:var(--fg-secondary); }
  .hero-av-wrap { display:flex; justify-content:center; padding:22px 0 16px; position:relative; z-index:1; }
  .hero-av-glow {
    position:absolute; top:8px; left:50%; transform:translateX(-50%);
    width:120px; height:120px; border-radius:50%; filter:blur(18px); pointer-events:none; opacity:0.85;
  }
  .hero-av-ring {
    position:relative; width:98px; height:98px; border-radius:50%; padding:3px;
    transition:transform var(--duration-slow);
  }
  .hero-av-ring:hover { transform:scale(1.03); }
  .hero-av {
    width:92px; height:92px; border-radius:50%;
    display:flex; align-items:center; justify-content:center; overflow:hidden;
    box-shadow:0 8px 40px rgba(0,0,0,0.5);
    position:relative; background-clip:padding-box;
  }
  .hero-av::after {
    content:''; position:absolute; inset:0; border-radius:50%;
    background:linear-gradient(135deg,rgba(255,255,255,0.15),transparent); pointer-events:none;
  }
  .hero-av-img { width:100%; height:100%; object-fit:cover; }
  .hero-initials { font-size:28px; font-weight:700; color:var(--accent-primary); letter-spacing:-0.02em; user-select:none; }

  .hero-name {
    font-size:19px; font-weight:800; text-align:center; letter-spacing:-0.3px;
    padding:0 18px; margin:0 0 5px; text-wrap:balance;
  }
  .hero-tagline { text-align:center; font-size:11.5px; font-style:italic; color:var(--fg-muted); padding:0 20px; line-height:1.55; margin:0 0 14px; }
  .hero-tags { display:flex; gap:5px; justify-content:center; flex-wrap:wrap; padding:0 14px; margin-bottom:18px; }
  .hero-tag {
    display:inline-flex; align-items:center; gap:5px;
    font-size:9.5px; font-weight:600; letter-spacing:0.06em; padding:3px 10px 3px 8px;
    border-radius:var(--rounded-full); text-transform:uppercase;
    background:rgba(255,255,255,0.04); border:1px solid var(--border-subtle); color:var(--fg-muted);
  }
  .hero-tag-dot { width:5px; height:5px; border-radius:50%; flex-shrink:0; }
  .hero-voice {
    display:flex; align-items:center; gap:6px; padding:6px 12px; margin:0 14px 14px;
    border-radius:var(--rounded-md); background:rgba(255,255,255,0.03); border:1px solid var(--border-subtle);
    font-size:11px; color:var(--fg-muted);
  }
  .hero-voice-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; text-transform:capitalize; }
  .hero-voice-preview, .hero-voice-edit {
    background:none; border:none; cursor:pointer; color:var(--fg-muted); font-size:10px;
    padding:2px 6px; border-radius:var(--rounded-sm); transition:color var(--duration-fast),background var(--duration-fast);
  }
  .hero-voice-preview:hover:not(:disabled), .hero-voice-edit:hover { color:var(--fg); background:rgba(255,255,255,0.06); }
  .hero-voice-preview:disabled { opacity:0.5; cursor:default; }
  .hero-actions { padding:0 14px; display:flex; flex-direction:column; gap:8px; margin-bottom:18px; }
  .btn-primary {
    height:38px; width:100%; border:none; border-radius:var(--rounded-md); cursor:pointer;
    background:linear-gradient(135deg,var(--accent-primary-hover),var(--accent-primary),#9f6ef7);
    background-size:200% 100%; background-position:right;
    color:#fff; font-size:12px; font-weight:600; letter-spacing:0.03em;
    transition:background-position 400ms,box-shadow var(--duration-normal),transform var(--duration-fast);
    box-shadow:0 4px 20px rgba(139,92,246,0.3); position:relative; overflow:hidden;
  }
  .btn-primary::before {
    content:''; position:absolute; top:0; left:-100%; width:60%; height:100%;
    background:linear-gradient(90deg,transparent,rgba(255,255,255,0.12),transparent);
    transition:left 400ms;
  }
  .btn-primary:hover { background-position:left; box-shadow:0 6px 28px rgba(139,92,246,0.45); transform:translateY(-1px); }
  .btn-primary:hover::before { left:150%; }
  .btn-outline {
    height:34px; width:100%; background:transparent; border-radius:var(--rounded-md);
    border:1px solid rgba(139,92,246,0.2); cursor:pointer; color:var(--fg-muted); font-size:11.5px;
    transition:border-color var(--duration-fast),color var(--duration-fast),background var(--duration-fast);
  }
  .btn-outline:hover { border-color:rgba(139,92,246,0.4); color:var(--fg-secondary); background:rgba(139,92,246,0.06); }
  .hero-divider { height:1px; margin:0 16px 14px; background:linear-gradient(90deg,transparent,rgba(139,92,246,0.15),transparent); }
  .hero-section-label { font-size:9px; font-weight:700; letter-spacing:0.12em; text-transform:uppercase; color:rgba(139,92,246,0.3); padding:0 18px 7px; margin:0; font-family:var(--font-mono); }
  .conv-list { flex:1; overflow-y:auto; padding:0 8px 16px; display:flex; flex-direction:column; gap:1px; }
  .conv-list::-webkit-scrollbar { width:2px; }
  .conv-list::-webkit-scrollbar-thumb { background:rgba(139,92,246,0.12); border-radius:2px; }
  .conv-item {
    width:100%; background:none; border:none; border-radius:var(--rounded-sm);
    padding:8px 10px; cursor:pointer; text-align:left; display:flex; align-items:center; gap:8px;
    border-left:2px solid transparent; transition:background var(--duration-fast),border-color var(--duration-fast),transform var(--duration-fast);
  }
  .conv-item:hover { background:var(--surface-hover); border-left-color:rgba(139,92,246,0.35); transform:translateX(1px); }
  .conv-dot { width:5px; height:5px; border-radius:50%; flex-shrink:0; opacity:0.75; }
  .conv-item-body { min-width:0; flex:1; display:flex; flex-direction:column; gap:3px; }
  .conv-title { font-size:12px; color:var(--fg-secondary); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-style:italic; }
  .conv-meta { font-size:9.5px; color:var(--fg-muted); font-family:var(--font-mono); }

  /* ── Main ── */
  .main { flex:1; display:flex; flex-direction:column; overflow:hidden; }
  .tabs { display:flex; gap:8px; flex-wrap:wrap; padding:20px 28px 4px; flex-shrink:0; }
  .tab {
    display:flex; align-items:center; gap:7px;
    height:36px; padding:0 16px; border-radius:999px;
    background:rgba(255,255,255,0.04); border:1px solid var(--border-subtle);
    font-size:12.5px; font-weight:600; letter-spacing:0.02em; color:var(--fg-muted); cursor:pointer;
    transition:all 220ms cubic-bezier(0.16,1,0.3,1);
    white-space:nowrap; font-family:var(--font-body); position:relative;
  }
  .tab.active { color:#0a0812; font-weight:700; transform:scale(1.05); }
  .tab:hover:not(.active) { color:var(--fg-secondary); background:rgba(255,255,255,0.08); border-color:rgba(255,255,255,0.16); }
  .tab-body { flex:1; overflow-y:auto; padding:24px 28px; animation:fadeUp 280ms var(--ease-out) both; }
  @keyframes fadeUp { from{opacity:0;transform:translateY(10px)} to{opacity:1;transform:translateY(0)} }
  .tab-body::-webkit-scrollbar { width:3px; }
  .tab-body::-webkit-scrollbar-thumb { background:rgba(139,92,246,0.12); border-radius:2px; }

  /* ── Profile Tab ── */
  .field-grid { display:grid; grid-template-columns:1fr 1fr; gap:14px; }
  .field-card {
    background:var(--surface-card); border:1px solid var(--border-subtle); border-radius:var(--rounded-lg);
    padding:16px 18px; position:relative; overflow:hidden; backdrop-filter:blur(8px);
    transition:border-color var(--duration-slow),transform var(--duration-normal);
  }
  .field-card::before {
    content:''; position:absolute; top:0; left:0; width:3px; height:100%;
    background:var(--field-accent, var(--accent-primary)); opacity:0.75;
  }
  .field-card:hover { border-color:var(--border-active); transform:translateY(-1px); }
  .field-card.full { grid-column:1/-1; }
  .field-card.accent-violet { --field-accent:#9075f2; }
  .field-card.accent-fuchsia { --field-accent:#e879f9; }
  .field-card.accent-amber { --field-accent:#fbbf24; }
  .field-card.accent-cyan { --field-accent:#22d3ee; }
  .field-label {
    font-size:9px; font-weight:700; letter-spacing:0.12em; text-transform:uppercase;
    color:var(--field-accent, rgba(139,92,246,0.45)); margin:0 0 9px; font-family:var(--font-mono);
  }
  .field-text { font-size:13px; color:var(--fg-secondary); line-height:1.68; margin:0; }
  .quote-card { padding-top:22px; }
  .quote-card::after {
    content:'“'; position:absolute; top:-14px; right:18px;
    font-size:80px; font-family:Georgia,serif; color:var(--field-accent);
    opacity:0.1; line-height:1; pointer-events:none;
  }
  .quote-card .field-text { position:relative; z-index:1; }

  /* ── Memories Tab ── */
  .memories-tab { display:flex; flex-direction:column; height:100%; gap:12px; }
  .tab-section-label { font-size:9px; font-weight:700; letter-spacing:0.12em; color:rgba(139,92,246,0.3); text-transform:uppercase; margin:0; font-family:var(--font-mono); }
  .memory-graph-wrap { flex:1; min-height:400px; border-radius:var(--rounded-lg); overflow:hidden; border:1px solid var(--border-subtle); }
  .tab-loading { display:flex; align-items:center; justify-content:center; padding:80px; }
  .empty-tab { font-size:13.5px; color:var(--fg-muted); padding:60px 0; text-align:center; font-style:italic; }

  /* ── Lore Tab ── */
  .lore-list { display:flex; flex-direction:column; gap:10px; }
  .lore-entry {
    background:var(--surface-card); border:1px solid var(--border-subtle); border-radius:var(--rounded-lg);
    padding:14px 16px; border-left:2px solid rgba(139,92,246,0.2); backdrop-filter:blur(8px);
    transition:border-color var(--duration-fast),transform var(--duration-fast);
  }
  .lore-entry:hover { border-left-color:var(--accent-primary); transform:translateX(2px); }
  .lore-entry.disabled { opacity:0.4; }
  .lore-name { font-size:12px; font-weight:600; color:var(--fg-secondary); margin:0 0 5px; }
  .lore-keys { font-size:10px; color:var(--fg-muted); font-family:var(--font-mono); margin:0 0 7px; opacity:0.7; }
  .lore-content { font-size:12.5px; color:var(--fg-muted); line-height:1.6; margin:0; }

  /* ── Stats Tab ── */
  .stat-grid { display:grid; grid-template-columns:repeat(3,1fr); gap:14px; }
  .stat-card {
    background:var(--surface-card); border:1px solid var(--border-subtle); border-radius:var(--rounded-lg);
    padding:26px 18px; text-align:center; position:relative; overflow:hidden; backdrop-filter:blur(8px);
    transition:border-color var(--duration-slow),transform var(--duration-normal),box-shadow var(--duration-slow);
  }
  .stat-card:hover { border-color:rgba(139,92,246,0.25); transform:translateY(-3px); box-shadow:0 12px 40px rgba(0,0,0,0.3),0 0 0 1px rgba(139,92,246,0.12); }
  .stat-card::before {
    content:''; position:absolute; top:0; left:50%; transform:translateX(-50%);
    width:60%; height:1px; background:linear-gradient(90deg,transparent,rgba(139,92,246,0.4),transparent);
  }
  .stat-card::after {
    content:''; position:absolute; bottom:-30px; left:50%; transform:translateX(-50%);
    width:80px; height:80px; border-radius:50%;
    background:radial-gradient(circle,rgba(139,92,246,0.08),transparent); pointer-events:none;
  }
  .stat-val {
    font-size:38px; font-weight:800; letter-spacing:-0.03em; margin:0 0 6px;
    background:linear-gradient(135deg,#c4a1ff,var(--accent-primary));
    -webkit-background-clip:text; -webkit-text-fill-color:transparent; background-clip:text;
    filter:drop-shadow(0 0 20px rgba(139,92,246,0.3));
  }
  .stat-lbl { font-size:9px; font-weight:700; letter-spacing:0.1em; text-transform:uppercase; color:rgba(139,92,246,0.35); margin:0; font-family:var(--font-mono); }

  /* ── Edit Tab ── */
  .edit-form { display:flex; flex-direction:column; gap:16px; max-width:660px; }
  .edit-field { display:flex; flex-direction:column; gap:6px; }
  .edit-label { font-size:9px; font-weight:700; letter-spacing:0.12em; color:rgba(139,92,246,0.4); text-transform:uppercase; font-family:var(--font-mono); }
  .edit-input {
    height:40px; padding:0 13px; border-radius:var(--rounded-md);
    background:var(--surface-input); border:1px solid rgba(139,92,246,0.1);
    color:var(--fg-primary); font-size:13px; outline:none; font-family:var(--font-body);
    transition:border-color var(--duration-normal),box-shadow var(--duration-normal);
  }
  .edit-input:focus { border-color:rgba(139,92,246,0.35); box-shadow:0 0 0 3px rgba(139,92,246,0.08); }
  select.edit-input {
    appearance:none; -webkit-appearance:none; cursor:pointer; padding-right:32px;
    background-image:url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='%238b8ba7' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
    background-repeat:no-repeat; background-position:right 10px center; background-size:14px;
  }
  .edit-textarea {
    padding:10px 13px; border-radius:var(--rounded-md);
    background:var(--surface-input); border:1px solid rgba(139,92,246,0.1);
    color:var(--fg-primary); font-size:13px; line-height:1.6; resize:vertical;
    outline:none; font-family:var(--font-body);
    transition:border-color var(--duration-normal),box-shadow var(--duration-normal);
  }
  .edit-textarea:focus { border-color:rgba(139,92,246,0.35); box-shadow:0 0 0 3px rgba(139,92,246,0.08); }
  .edit-actions { display:flex; justify-content:flex-end; gap:10px; padding-top:4px; }
  .btn-cancel {
    height:36px; padding:0 18px; background:transparent;
    border:1px solid var(--border-subtle); border-radius:var(--rounded-md);
    color:var(--fg-muted); font-size:12px; cursor:pointer; font-family:var(--font-body);
    transition:border-color var(--duration-fast),color var(--duration-fast);
  }
  .btn-cancel:hover { border-color:rgba(139,92,246,0.2); color:var(--fg-secondary); }
  .btn-save {
    height:36px; padding:0 22px; border:none; border-radius:var(--rounded-md);
    background:linear-gradient(135deg,var(--accent-primary-hover),var(--accent-primary));
    color:#fff; font-size:12px; font-weight:600; cursor:pointer; font-family:var(--font-body);
    box-shadow:0 4px 16px rgba(139,92,246,0.3);
    transition:opacity var(--duration-fast),transform var(--duration-fast),box-shadow var(--duration-normal);
  }
  .btn-save:hover:not(:disabled) { opacity:0.9; transform:translateY(-1px); box-shadow:0 6px 24px rgba(139,92,246,0.4); }
  .btn-save:disabled { opacity:0.4; cursor:not-allowed; }
</style>
