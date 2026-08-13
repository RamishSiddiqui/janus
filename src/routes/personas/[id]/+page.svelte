<script lang="ts">
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { browser } from "$app/environment";
  import Icon from "$lib/components/Icon.svelte";
  import { success, error as toastError, addToast } from "$lib/stores/toast";
  import { parseCharacterData } from "$lib/utils/character";
  import { sceneGenerations, getSceneGenerationState, trackPersonaPortraitGeneration, personaPortraitGenerationKey, describeProgress } from "$lib/stores/sceneGeneration";

  const isTauri = browser && "__TAURI_INTERNALS__" in window;
  const personaId = $derived($page.params.id);

  interface PersonaData {
    description: string;
    personality: string;
    scenario: string;
    tags: string[];
  }

  let personaName = $state("");
  let personaData = $state<PersonaData | null>(null);
  let avatarUrl = $state<string | null>(null);
  let avatarPath = $state<string | null>(null);
  let isLoading = $state(true);
  // Reads purely from the global sceneGenerations store, keyed by
  // personaId — never local component state, which resets to false on
  // remount (navigating away and back) even though a still-running
  // generation keeps going server-side. Same pattern/reasoning as
  // ContextNpcPanel.svelte's NPC portrait tracking.
  let portraitState = $derived(getSceneGenerationState($sceneGenerations, personaId ? personaPortraitGenerationKey(personaId) : null));

  let editName = $state("");
  let editDesc = $state("");
  let editPersonality = $state("");
  let editScenario = $state("");
  let editTags = $state("");
  let isSaving = $state(false);

  $effect(() => {
    const id = personaId;
    if (id && isTauri) loadPersona(id);
  });

  async function resolveAvatar(path: string | null): Promise<string | null> {
    if (!path || !isTauri) return null;
    try {
      const { loadFileAsBlobUrl } = await import("$lib/utils/blobUrl");
      return await loadFileAsBlobUrl(path);
    } catch {
      return null;
    }
  }

  async function loadPersona(id: string) {
    isLoading = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const persona = await ipc.getPersona(id);
      personaName = persona.name;
      const parsed = parseCharacterData(persona.data);
      personaData = {
        description: (parsed.description as string) || "",
        personality: (parsed.personality as string) || "",
        scenario: (parsed.scenario as string) || "",
        tags: (parsed.tags as string[]) || [],
      };
      avatarPath = persona.avatar_path;
      avatarUrl = await resolveAvatar(persona.avatar_path);

      editName = persona.name;
      editDesc = personaData.description;
      editPersonality = personaData.personality;
      editScenario = personaData.scenario;
      editTags = personaData.tags.join(", ");
    } catch {
      toastError("Failed to load persona");
      goto("/personas");
    }
    isLoading = false;
  }

  async function saveEdit() {
    if (!isTauri || !editName.trim() || !personaId) return;
    isSaving = true;
    try {
      const ipc = await import("$lib/services/ipc");
      const tags = editTags.split(",").map((t) => t.trim()).filter(Boolean);
      await ipc.updatePersona(personaId, editName, {
        description: editDesc,
        personality: editPersonality,
        scenario: editScenario,
        tags,
      });
      personaName = editName;
      personaData = {
        description: editDesc,
        personality: editPersonality,
        scenario: editScenario,
        tags,
      };
      success("Persona saved");
    } catch {
      toastError("Failed to save persona");
    }
    isSaving = false;
  }

  async function generatePortrait() {
    if (!isTauri || !personaId) return;
    try {
      const ipc = await import("$lib/services/ipc");
      const updated = await trackPersonaPortraitGeneration(personaId, () => ipc.generatePersonaPortrait(personaId));
      if (updated.avatar_path === avatarPath) {
        toastError("No image provider configured — set one in Providers first.");
      } else {
        avatarPath = updated.avatar_path;
        avatarUrl = await resolveAvatar(updated.avatar_path);
        success("Portrait generated");
      }
    } catch {
      toastError("Failed to generate portrait");
    }
  }

  /**
   * Moves this persona to Trash immediately — a real, durable backend
   * soft-delete. The Undo toast's action calls restorePersona and navigates
   * back here. Permanent removal only happens from the Trash page.
   */
  async function handleDelete() {
    if (!isTauri || !personaId) return;
    const id = personaId;
    const name = personaName || "Persona";

    try {
      const ipc = await import("$lib/services/ipc");
      await ipc.trashPersona(id);
    } catch {
      toastError("Failed to delete persona");
      return;
    }

    goto("/personas");
    addToast(`Moved ${name} to Trash`, 'info', 5500, {
      label: 'Undo',
      onClick: async () => {
        try {
          const ipc = await import("$lib/services/ipc");
          await ipc.restorePersona(id);
          goto(`/personas/${id}`);
        } catch {
          toastError("Failed to restore persona");
        }
      },
    });
  }

  const accentColor = "#8B5CF6";
  const initials = $derived(
    personaName.split(" ").map((w) => w[0]).join("").slice(0, 2).toUpperCase() || "?",
  );
</script>

<svelte:head><title>{personaName || "Persona"} — Janus</title></svelte:head>

<div class="persona-page">
  {#if isLoading}
    <div class="loading-state"><div class="loading-spinner"></div></div>
  {:else}
    <aside class="hero">
      <div class="hero-glow" style="background:radial-gradient(circle,{accentColor}33 0%,transparent 70%)"></div>
      <button class="back-btn" onclick={() => goto("/personas")}>← Personas</button>

      <div class="hero-av-wrap">
        <div class="hero-av" style="background:linear-gradient(135deg,{accentColor}99,{accentColor})">
          {#if avatarUrl}
            <img src={avatarUrl} alt={personaName} class="hero-av-img" />
          {:else}
            <span class="hero-initials">{initials}</span>
          {/if}
        </div>
      </div>

      <h1 class="hero-name">{personaName}</h1>

      <div class="hero-actions">
        <button class="btn-primary" onclick={generatePortrait} disabled={portraitState.isLoading}>
          {portraitState.isLoading ? describeProgress(portraitState.progress) : "🖼 Generate Portrait"}
        </button>
        <button class="btn-danger" onclick={handleDelete}>Delete Persona</button>
      </div>
    </aside>

    <main class="main">
      <div class="tab-body">
        <div class="edit-form">
          <div class="edit-field">
            <label class="edit-label" for="pf-name">Name *</label>
            <input id="pf-name" class="edit-input" bind:value={editName} placeholder="Persona name" />
          </div>
          <div class="edit-field">
            <label class="edit-label" for="pf-desc">Description</label>
            <textarea id="pf-desc" class="edit-textarea" rows="4" bind:value={editDesc} placeholder="Appearance, background, who this persona is..."></textarea>
          </div>
          <div class="edit-field">
            <label class="edit-label" for="pf-pers">Personality</label>
            <textarea id="pf-pers" class="edit-textarea" rows="3" bind:value={editPersonality} placeholder="Traits, demeanor, speech patterns..."></textarea>
          </div>
          <div class="edit-field">
            <label class="edit-label" for="pf-sc">Scenario</label>
            <textarea id="pf-sc" class="edit-textarea" rows="2" bind:value={editScenario} placeholder="Optional — context for how this persona shows up in a story..."></textarea>
          </div>
          <div class="edit-field">
            <label class="edit-label" for="pf-tags">Tags (comma-separated)</label>
            <input id="pf-tags" class="edit-input" bind:value={editTags} placeholder="Noble, Mercenary, Scholar" />
          </div>
          <div class="edit-actions">
            <button class="btn-save" onclick={saveEdit} disabled={isSaving || !editName.trim()}>
              {isSaving ? "Saving…" : "Save Changes"}
            </button>
          </div>
        </div>
      </div>
    </main>
  {/if}
</div>

<style>
  .persona-page {
    flex:1; display:flex; height:100%; overflow:hidden;
    background:linear-gradient(160deg,#0c0b1e 0%,#07071a 55%,#080816 100%);
    position:relative;
  }
  .loading-state { flex:1; display:flex; align-items:center; justify-content:center; }
  .loading-spinner {
    width:30px; height:30px; border-radius:50%;
    border:2px solid rgba(139,92,246,0.15); border-top-color:var(--accent-primary);
    animation:spin 700ms linear infinite;
  }
  @keyframes spin { to { transform:rotate(360deg); } }

  .hero {
    width:272px; flex-shrink:0; position:relative; z-index:1;
    background:linear-gradient(180deg,#110f24 0%,#0c0b1e 50%,#07071a 100%);
    border-right:1px solid rgba(139,92,246,0.1);
    display:flex; flex-direction:column; overflow:hidden;
  }
  .hero-glow {
    position:absolute; top:0; left:50%; transform:translateX(-50%);
    width:240px; height:200px; border-radius:50%; pointer-events:none; opacity:0.5;
  }
  .back-btn {
    background:none; border:none; color:var(--fg-muted);
    font-size:11px; letter-spacing:0.03em; padding:14px 18px 0;
    cursor:pointer; text-align:left;
  }
  .back-btn:hover { color:var(--fg-secondary); }
  .hero-av-wrap { display:flex; justify-content:center; padding:22px 0 16px; position:relative; z-index:1; }
  .hero-av {
    width:92px; height:92px; border-radius:50%;
    display:flex; align-items:center; justify-content:center; overflow:hidden;
    box-shadow:0 0 0 2px rgba(139,92,246,0.3),0 0 0 6px rgba(139,92,246,0.06),0 8px 40px rgba(0,0,0,0.5);
  }
  .hero-av-img { width:100%; height:100%; object-fit:cover; }
  .hero-initials { font-size:28px; font-weight:700; color:var(--accent-primary); letter-spacing:-0.02em; }
  .hero-name {
    font-size:16px; font-weight:700; text-align:center; letter-spacing:-0.3px;
    padding:0 18px; margin:0 0 16px;
    background:linear-gradient(135deg,#e8e0ff,#c4a1ff);
    -webkit-background-clip:text; -webkit-text-fill-color:transparent; background-clip:text;
  }
  .hero-actions { padding:0 14px; display:flex; flex-direction:column; gap:8px; margin-bottom:18px; }
  .btn-primary {
    height:38px; width:100%; border:none; border-radius:var(--rounded-md); cursor:pointer;
    background:linear-gradient(135deg,var(--accent-primary-hover),var(--accent-primary),#9f6ef7);
    color:#fff; font-size:12px; font-weight:600; letter-spacing:0.03em;
    box-shadow:0 4px 20px rgba(139,92,246,0.3);
  }
  .btn-primary:disabled { opacity:0.5; cursor:not-allowed; }
  .btn-danger {
    height:34px; width:100%; background:transparent; border-radius:var(--rounded-md);
    border:1px solid rgba(244,63,94,0.2); cursor:pointer; color:rgba(244,63,94,0.8); font-size:11.5px;
    margin-top: 8px;
  }
  .btn-danger:hover { border-color:rgba(244,63,94,0.4); background:rgba(244,63,94,0.06); }

  .main { flex:1; display:flex; flex-direction:column; overflow:hidden; }
  .tab-body { flex:1; overflow-y:auto; padding:28px; }
  .edit-form { display:flex; flex-direction:column; gap:16px; max-width:660px; }
  .edit-field { display:flex; flex-direction:column; gap:6px; }
  .edit-label { font-size:9px; font-weight:700; letter-spacing:0.12em; color:rgba(139,92,246,0.4); text-transform:uppercase; font-family:var(--font-mono); }
  .edit-input {
    height:40px; padding:0 13px; border-radius:var(--rounded-md);
    background:var(--surface-input); border:1px solid rgba(139,92,246,0.1);
    color:var(--fg-primary); font-size:13px; outline:none; font-family:var(--font-body);
  }
  .edit-input:focus { border-color:rgba(139,92,246,0.35); box-shadow:0 0 0 3px rgba(139,92,246,0.08); }
  .edit-textarea {
    padding:10px 13px; border-radius:var(--rounded-md);
    background:var(--surface-input); border:1px solid rgba(139,92,246,0.1);
    color:var(--fg-primary); font-size:13px; line-height:1.6; resize:vertical;
    outline:none; font-family:var(--font-body);
  }
  .edit-textarea:focus { border-color:rgba(139,92,246,0.35); box-shadow:0 0 0 3px rgba(139,92,246,0.08); }
  .edit-actions { display:flex; justify-content:flex-end; gap:10px; padding-top:4px; }
  .btn-save {
    height:36px; padding:0 22px; border:none; border-radius:var(--rounded-md);
    background:linear-gradient(135deg,var(--accent-primary-hover),var(--accent-primary));
    color:#fff; font-size:12px; font-weight:600; cursor:pointer; font-family:var(--font-body);
    box-shadow:0 4px 16px rgba(139,92,246,0.3);
  }
  .btn-save:hover:not(:disabled) { opacity:0.9; transform:translateY(-1px); }
  .btn-save:disabled { opacity:0.4; cursor:not-allowed; }
</style>
