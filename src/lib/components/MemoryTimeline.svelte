<script lang="ts">
  import type { MemoryGraph as MemoryGraphData } from '$lib/services/ipc';
  import { shareMemory, unlinkMemory, deleteMemory } from '$lib/services/ipc';
  import { undoableDelete } from '$lib/stores/toast';
  import { buildTimelineEntries, type TimelineEntry, type TimelineGroup, type TimelineLinkRow, type MemoryItem } from '$lib/utils/groupMemories';
  import Icon from './Icon.svelte';
  import MemoryActionPanel from './MemoryActionPanel.svelte';

  interface Props {
    data: MemoryGraphData;
    onRefresh?: () => void;
    /** Conversation ids to show, controlled by the host's TimelineFilter.
        `null`/`undefined` means "no filter — show every conversation" (canon
        is always shown regardless, it has no single conversation home). */
    visibleConvIds?: Set<string> | null;
  }

  let { data, onRefresh = () => {}, visibleConvIds = null }: Props = $props();

  const PALETTE = ['#c4a1ff', '#00f2ff', '#fb7185', '#fbbf24', '#34d399', '#d580ff'];
  const CANON_COLOR = '#daa520';

  const CATEGORY_ICONS: Record<string, string> = {
    trait: '🧬', event: '⚡', relationship: '💫', goal: '🎯',
    discovery: '🔮', preference: '💭', fact: '📋',
  };

  // ── Lanes ──
  // Canon lane + one lane per conversation, grouped by character
  // Shared conversations (2+ characters) get characterId='__shared__'
  interface Lane {
    id: string;         // 'canon' or conversation_id
    label: string;
    color: string;
    characterId?: string;
    characterName?: string;
    // Shared conversation participants
    participantIds?: string[];
    participantColors?: string[];
    participantNames?: string[];
  }

  // Build character name map from the multi-character data
  let charNameMap = $derived.by(() => {
    const map = new Map<string, string>();
    if (data.characters) {
      data.characters.forEach(c => map.set(c.id, c.name));
    }
    return map;
  });

  let isMultiChar = $derived(!!(data.characters && data.characters.length > 1));

  let lanes = $derived.by(() => {
    const result: Lane[] = [];
    // Canon lane first
    const hasCanon = data.memories.some(m => m.is_canon);
    if (hasCanon) {
      result.push({ id: 'canon', label: 'Canon', color: CANON_COLOR });
    }

    if (!isMultiChar) {
      data.conversations.forEach((c, i) => {
        result.push({ id: c.id, label: c.title, color: PALETTE[i % PALETTE.length] });
      });
      return result;
    }

    // ── Multi-character: detect shared conversations ──
    // A conversation is "shared" if memories from 2+ characters exist in it
    const convChars = new Map<string, Set<string>>();
    data.memories.forEach(m => {
      if (!m.conversation_id || !m.character_id) return;
      if (!convChars.has(m.conversation_id)) convChars.set(m.conversation_id, new Set());
      convChars.get(m.conversation_id)!.add(m.character_id);
    });

    const sharedConvIds = new Set<string>();
    convChars.forEach((chars, convId) => { if (chars.size > 1) sharedConvIds.add(convId); });

    // Exclusive conversations grouped by character
    const exclusiveGrouped = new Map<string, typeof data.conversations>();
    const sharedConvs: typeof data.conversations = [];

    data.conversations.forEach(c => {
      if (sharedConvIds.has(c.id)) {
        sharedConvs.push(c);
      } else {
        const charId = c.character_id ?? 'unknown';
        if (!exclusiveGrouped.has(charId)) exclusiveGrouped.set(charId, []);
        exclusiveGrouped.get(charId)!.push(c);
      }
    });

    // Add exclusive lanes per character
    let colorIdx = 0;
    for (const [charId, convs] of exclusiveGrouped) {
      const charName = charNameMap.get(charId) ?? charId;
      convs.forEach(c => {
        result.push({
          id: c.id, label: c.title,
          color: PALETTE[colorIdx % PALETTE.length],
          characterId: charId, characterName: charName,
        });
        colorIdx++;
      });
    }

    // Add shared ("Crossroads") lanes at the end
    sharedConvs.forEach(c => {
      const participants = convChars.get(c.id) ?? new Set();
      const partColors = [...participants].map((pid, i) => {
        // Find the first exclusive lane of this character for its color
        const charLane = result.find(l => l.characterId === pid);
        return charLane?.color ?? PALETTE[(colorIdx + i) % PALETTE.length];
      });
      result.push({
        id: c.id, label: c.title,
        color: partColors[0] ?? PALETTE[colorIdx % PALETTE.length],
        characterId: '__shared__',
        characterName: 'Crossroads',
        participantIds: [...participants],
        participantColors: partColors,
        participantNames: [...participants].map(pid => charNameMap.get(pid) ?? pid),
      });
      colorIdx++;
    });

    return result;
  });

  // ── Timeline visibility filter ──
  // Which lanes to actually render — driven by the host's TimelineFilter
  // (in the stats strip) via `visibleConvIds`. Canon is never filterable —
  // it has no single conversation home, so it always stays visible.
  let visibleLaneIds = $derived.by(() => {
    if (!visibleConvIds) return new Set(lanes.map(l => l.id));
    return new Set<string>(['canon', ...visibleConvIds]);
  });
  let visibleLanes = $derived(lanes.filter(l => visibleLaneIds.has(l.id)));

  // Conversation titles, for the "hidden link" tooltip below.
  let convTitleMap = $derived(new Map((data.conversations ?? []).map(c => [c.id, c.title])));

  // lane_id (conversation_id) → titles of conversations it links to that
  // are currently filtered out — surfaced as a small badge on the lane's
  // own header so hiding a timeline doesn't silently hide the fact that a
  // visible conversation still connects to it. Deliberately checked at the
  // conversation level (matching how the link line itself is drawn between
  // two *lanes* in the unfiltered view) rather than trying to pin the
  // connection to one specific memory — `linked_memory_id` isn't always
  // populated (e.g. a live sync link that hasn't materialized a concrete
  // copy yet), which made a memory-level check miss real connections.
  let laneHiddenLinks = $derived.by(() => {
    const map = new Map<string, string[]>();
    const markHidden = (laneId: string | null | undefined, otherConvId: string | null | undefined) => {
      if (!laneId || !otherConvId || laneId === otherConvId || !visibleLaneIds.has(laneId) || visibleLaneIds.has(otherConvId)) return;
      const title = convTitleMap.get(otherConvId) ?? 'another timeline';
      const existing = map.get(laneId) ?? [];
      if (!existing.includes(title)) map.set(laneId, [...existing, title]);
    };
    for (const l of data.links) {
      const sourceConvId = memConvMap.get(l.source_memory_id);
      markHidden(sourceConvId, l.target_conversation_id);
      markHidden(l.target_conversation_id, sourceConvId);
    }
    return map;
  });

  // Character groups for the spanning header row
  interface CharGroup {
    charId: string;
    name: string;
    startCol: number; // 0-based grid column start
    span: number;     // how many columns this group spans
    color: string;    // first lane's color for accent
    isShared: boolean;
    participantColors?: string[];
    participantNames?: string[];
  }

  let charGroups = $derived.by(() => {
    if (!isMultiChar) return [];
    const groups: CharGroup[] = [];
    let i = 0;
    while (i < visibleLanes.length) {
      const lane = visibleLanes[i];
      if (!lane.characterId) { i++; continue; } // skip canon

      if (lane.characterId === '__shared__') {
        // Shared group — collect all consecutive shared lanes
        const start = i;
        while (i < visibleLanes.length && visibleLanes[i].characterId === '__shared__') i++;
        groups.push({
          charId: '__shared__',
          name: 'Crossroads',
          startCol: start,
          span: i - start,
          color: '#ff9f43',
          isShared: true,
          participantColors: [...new Set(
            visibleLanes.slice(start, i).flatMap(l => l.participantColors ?? [])
          )],
          participantNames: [...new Set(
            visibleLanes.slice(start, i).flatMap(l => l.participantNames ?? [])
          )],
        });
      } else {
        // Exclusive character group
        const charId = lane.characterId;
        const start = i;
        while (i < visibleLanes.length && visibleLanes[i].characterId === charId) i++;
        groups.push({
          charId,
          name: lane.characterName ?? charId,
          startCol: start,
          span: i - start,
          color: visibleLanes[start].color,
          isShared: false,
        });
      }
    }
    return groups;
  });

  // Map character_id → their exclusive lane color (for shared lane card tinting)
  let charColorMap = $derived.by(() => {
    const map = new Map<string, string>();
    lanes.forEach(l => {
      if (l.characterId && l.characterId !== '__shared__' && !map.has(l.characterId)) {
        map.set(l.characterId, l.color);
      }
    });
    return map;
  });

  // Map memory_id → lane_id
  let memLaneMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.memories.forEach(m => {
      map.set(m.id, m.is_canon ? 'canon' : (m.conversation_id ?? 'canon'));
    });
    return map;
  });

  // Map memory_id → conversation_id (for links)
  let memConvMap = $derived.by(() => {
    const map = new Map<string, string>();
    data.memories.forEach(m => {
      if (m.conversation_id) map.set(m.id, m.conversation_id);
    });
    return map;
  });

  // ── Timeline entries (grouped) ──
  // Only memories in a currently-visible lane are laid out — canon always
  // qualifies since its lane id ('canon') is always in visibleLaneIds.
  let visibleMemories = $derived(data.memories.filter(m => {
    const laneId = m.is_canon ? 'canon' : (m.conversation_id ?? 'canon');
    return visibleLaneIds.has(laneId);
  }));
  let rows = $derived.by(() => buildTimelineEntries(visibleMemories, data.links));

  // ── Group expand/collapse state ──
  let expandedGroups = $state<Set<string>>(new Set());

  function toggleGroup(groupId: string): void {
    const next = new Set(expandedGroups);
    if (next.has(groupId)) {
      next.delete(groupId);
    } else {
      next.add(groupId);
    }
    expandedGroups = next;
  }

  /** Resolve the accent color for a memory item in context of its lane */
  function memColor(item: MemoryItem, laneId: string): string {
    const li = laneIndex(laneId);
    const lane = visibleLanes[li];
    const isSharedLane = lane?.characterId === '__shared__';
    const charId = item.memory.character_id ?? undefined;
    if (isSharedLane && charId) {
      return charColorMap.get(charId) ?? laneColor(laneId);
    }
    return laneColor(laneId);
  }

  function fmtShort(iso: string): string {
    if (!iso) return '';
    try {
      // Strip SurrealDB datetime wrappers: d'...' or "..."
      let cleaned = iso.replace('_link', '').replace(/^d['"]/, '').replace(/['"]$/, '').trim();
      const d = new Date(cleaned);
      if (isNaN(d.getTime())) return 'just now';
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    } catch { return ''; }
  }

  /** Grid-column index within the currently VISIBLE lanes (for layout math). */
  function laneIndex(laneId: string): number {
    return visibleLanes.findIndex(l => l.id === laneId);
  }

  /** Color is looked up from the full lane list so it stays stable even if
      the lane is later hidden (e.g. for a "hidden link" badge referencing it). */
  function laneColor(laneId: string): string {
    return lanes.find(l => l.id === laneId)?.color ?? '#5a5a7a';
  }

  // ── Action Panel state ──
  let selectedMemory = $state<MemoryGraphData['memories'][0] | null>(null);
  let selectedGroupMemories = $state<MemoryGraphData['memories']>([]);
  let selectedMemoryLinks = $derived(
    selectedMemory
      ? data.links.filter(l => l.source_memory_id === selectedMemory!.id)
      : []
  );

  function handleMemoryClick(item: MemoryItem, group: TimelineGroup): void {
    const mem = data.memories.find(m => m.id === item.memory.id);
    if (!mem) return;
    selectedMemory = mem;
    if (group.isGroup) {
      selectedGroupMemories = group.memories
        .map(gi => data.memories.find(m => m.id === gi.memory.id))
        .filter((m): m is MemoryGraphData['memories'][0] => !!m);
    } else {
      selectedGroupMemories = [];
    }
  }

  function handlePanelClose(): void {
    selectedMemory = null;
    selectedGroupMemories = [];
  }

  async function handlePanelShare(config: {
    sourceMemoryId: string;
    targetConversationId: string;
    linkType: 'copy' | 'sync';
    direction: 'one_way' | 'two_way';
    syncMode: 'auto' | 'manual';
  }) {
    await shareMemory(
      config.sourceMemoryId,
      config.targetConversationId,
      config.linkType,
      config.direction,
      config.syncMode,
    );
    onRefresh();
  }

  async function handlePanelUnlink(linkId: string) {
    await unlinkMemory(linkId);
    onRefresh();
  }

  function handlePanelDelete(memoryId: string) {
    selectedMemory = null;
    undoableDelete(
      'Memory removed',
      async () => {
        await deleteMemory(memoryId);
        onRefresh();
      },
      () => { /* nothing was hidden optimistically — nothing to restore */ },
    );
  }
</script>

<div class="tl-container">
  {#if lanes.length === 0}
    <div class="tl-empty">
      <div class="empty-icon-wrap"><Icon name="inbox" size={28} /></div>
      <p>No memories to display</p>
    </div>
  {:else}
    <!-- Lane headers -->
    <div class="lane-header-wrap">
      {#if visibleLanes.length > 0}
      {#if isMultiChar && charGroups.length > 0}
        <!-- Character group name row — names span across their lanes -->
        <div class="char-row" style="--lane-count: {visibleLanes.length};">
          {#each charGroups as g, gi}
            <div
              class="char-span"
              class:shared={g.isShared}
              style="
                grid-column: {g.startCol + 1} / span {g.span};
                --char-color: {g.color};
              "
            >
              {#if g.isShared}
                <!-- Shared "Crossroads" pill with multi-dot -->
                <div class="char-pill crossroads-pill">
                  {#each g.participantColors ?? [] as pc}
                    <span class="char-dot" style="background: {pc}; --char-color: {pc};"></span>
                  {/each}
                  <span class="char-name-text crossroads-text">Crossroads</span>
                </div>
              {:else}
                <div class="char-pill">
                  <span class="char-dot" style="background: {g.color};"></span>
                  <span class="char-name-text">{g.name}</span>
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      <!-- Lane label row -->
      <div class="lane-row" style="--lane-count: {visibleLanes.length};">
        {#each visibleLanes as lane}
          <div class="lane-col">
            {#if lane.participantColors && lane.participantColors.length > 0}
              <!-- Shared lane: show participant dots above title -->
              <div class="lane-participants">
                {#each lane.participantNames ?? [] as pName, pi}
                  <span class="participant-tag" style="--pt-color: {lane.participantColors?.[pi] ?? '#888'};">
                    {pName.split(' ')[0]}
                  </span>
                {/each}
              </div>
            {/if}
            <span class="lane-title-row">
              <span class="lane-label" style="color: {lane.color};">{lane.label}</span>
              {#if laneHiddenLinks.has(lane.id)}
                {@const hiddenTitles = laneHiddenLinks.get(lane.id)!}
                <span
                  class="hidden-link-pill"
                  title="This conversation shared memories with {hiddenTitles.join(', ')} — hidden by the timeline filter"
                >
                  <Icon name="link" size={11} />
                </span>
              {/if}
            </span>
            {#if lane.participantColors && lane.participantColors.length >= 2}
              <!-- Gradient underline blending participant colors -->
              <div class="lane-underline" style="background: linear-gradient(90deg, {lane.participantColors.join(', ')});"></div>
            {:else}
              <div class="lane-underline" style="background: {lane.color};"></div>
            {/if}
          </div>
        {/each}
      </div>
      {#if isMultiChar && charGroups.length > 0}
        <!-- Group highlight borders beneath lane labels -->
        <div class="group-dividers" style="--lane-count: {visibleLanes.length};">
          {#each charGroups as g}
            <div
              class="group-border"
              class:shared-border={g.isShared}
              style="
                grid-column: {g.startCol + 1} / span {g.span};
                --gb-color: {g.isShared ? '#ff9f43' : g.color};
              "
            ></div>
          {/each}
        </div>
      {/if}
      {/if}
    </div>

    {#if visibleLanes.length === 0}
      <div class="tl-empty">
        <div class="empty-icon-wrap"><Icon name="filter" size={28} /></div>
        <p>No timelines selected — pick one above to see its memories</p>
      </div>
    {:else}
    <!-- Timeline body -->
    <div class="tl-scroll">
      <div class="tl-body" style="--lane-count: {visibleLanes.length};">
        <!-- Character group zone highlights -->
        {#if isMultiChar && charGroups.length > 0}
          {#each charGroups as g, gi}
            <div
              class="char-zone"
              class:shared-zone={g.isShared}
              style="
                left: calc({g.startCol} * (100% / {visibleLanes.length}));
                width: calc({g.span} * (100% / {visibleLanes.length}));
                --zone-color: {g.isShared ? '#ff9f43' : g.color};
              "
            ></div>
          {/each}
        {/if}

        <!-- Vertical lane guides -->
        {#each visibleLanes as lane, li}
          <div
            class="lane-guide"
            style="left: calc({li} * (100% / {visibleLanes.length}) + (100% / {visibleLanes.length}) / 2); --guide-color: {lane.color};"
          ></div>
        {/each}

        <!-- Rows -->
        {#each rows as entry, ri (entry.type === 'group' ? entry.id : `link-${ri}`)}
          {#if entry.type === 'group'}
            {@const group = entry as TimelineGroup}
            {@const li = laneIndex(group.laneId)}
            {@const primary = group.memories[0]}
            {@const color = memColor(primary, group.laneId)}
            {@const isExpanded = expandedGroups.has(group.id)}
            <div
              class="mem-row"
              style="
                --accent: {color};
                --delay: {Math.min(ri * 25, 600)}ms;
                --lane-offset: calc({li} * (100% / {visibleLanes.length}));
                --lane-width: calc(100% / {visibleLanes.length});
              "
            >
              <div class="mem-cell" style="margin-left: var(--lane-offset); width: var(--lane-width);">
                <div class="mem-dot" style="background: {color}; box-shadow: 0 0 8px {color}44;"></div>
                <div class="mem-card" class:mem-group={group.isGroup} class:is-expanded={group.isGroup && isExpanded} class:is-selected={selectedMemory?.id === primary.memory.id}>
                  <div
                    class="card-click-target"
                    role="button"
                    tabindex="0"
                    onclick={() => handleMemoryClick(primary, group)}
                    onkeydown={(e) => e.key === 'Enter' && handleMemoryClick(primary, group)}
                  >
                  <div class="card-accent" style="background: linear-gradient(180deg, {color}, {color}33);"></div>
                  <div class="card-inner">
                    <!-- Primary (first) memory — always visible -->
                    <div class="card-head">
                      <span class="cat-icon">{primary.categoryIcon}</span>
                      <span class="card-content">{primary.content}</span>
                    </div>
                    <div class="card-meta">
                      <span class="meta-time">{fmtShort(primary.memory.created_at)}</span>
                      <span class="meta-badge" class:auto={primary.memory.source === 'auto'}>
                        {primary.memory.source === 'auto' ? 'auto' : 'pinned'}
                      </span>
                      {#if (primary.memory.version ?? 1) > 1}
                        <span class="meta-badge version">v{primary.memory.version}</span>
                      {/if}
                      {#if primary.memory.parent_id}
                        <span class="meta-badge inherited">inherited</span>
                      {/if}
                    </div>

                    {#if group.isGroup}
                      <!-- Expandable group content -->
                      <div class="group-expand-wrap" class:expanded={isExpanded}>
                        {#each group.memories.slice(1) as item (item.memory.id)}
                          <div class="group-divider"></div>
                          <div
                            class="group-child-click"
                            class:child-selected={selectedMemory?.id === item.memory.id}
                            role="button"
                            tabindex="0"
                            onclick={(e) => { e.stopPropagation(); handleMemoryClick(item, group); }}
                            onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); handleMemoryClick(item, group); } }}
                          >
                          <div class="card-head">
                            <span class="cat-icon">{item.categoryIcon}</span>
                            <span class="card-content">{item.content}</span>
                          </div>
                          <div class="card-meta">
                            <span class="meta-time">{fmtShort(item.memory.created_at)}</span>
                            <span class="meta-badge" class:auto={item.memory.source === 'auto'}>
                              {item.memory.source === 'auto' ? 'auto' : 'pinned'}
                            </span>
                            {#if (item.memory.version ?? 1) > 1}
                              <span class="meta-badge version">v{item.memory.version}</span>
                            {/if}
                            {#if item.memory.parent_id}
                              <span class="meta-badge inherited">inherited</span>
                            {/if}
                          </div>
                          </div>
                        {/each}
                      </div>

                      <!-- Expand / Collapse trigger -->
                      <button
                        class="group-expand-btn"
                        type="button"
                        onclick={(e) => { e.stopPropagation(); toggleGroup(group.id); }}
                      >
                        {#if isExpanded}
                          Collapse ▴
                        {:else}
                          + {group.memories.length - 1} more {group.memories.length - 1 === 1 ? 'memory' : 'memories'} ▾
                        {/if}
                      </button>
                    {/if}
                  </div>
                  </div>
                </div>
              </div>
            </div>
          {:else}
            {@const link = entry as TimelineLinkRow}
            {#if visibleLaneIds.has(link.toLaneId)}
            {@const fromIdx = laneIndex(link.fromLaneId)}
            {@const toIdx = laneIndex(link.toLaneId)}
            {@const minIdx = Math.min(fromIdx, toIdx)}
            {@const maxIdx = Math.max(fromIdx, toIdx)}
            {@const isSync = link.linkType === 'sync'}
            {@const isTwoWay = link.linkLabel.includes('⇄')}
            {@const flowColor = isSync ? 'rgba(0,242,255,0.45)' : 'rgba(139,92,246,0.4)'}
            {@const glowColor = isSync ? 'rgba(0,242,255,0.15)' : 'rgba(139,92,246,0.1)'}
            <div
              class="link-row"
              style="
                --delay: {Math.min(ri * 25, 600)}ms;
                --link-left: calc({minIdx} * (100% / {visibleLanes.length}) + (100% / {visibleLanes.length}) / 2);
                --link-width: calc({(maxIdx - minIdx)} * (100% / {visibleLanes.length}));
              "
            >
              <div class="link-line" class:two-way={isTwoWay} style="margin-left: var(--link-left); width: var(--link-width); --flow-color: {flowColor}; --glow-color: {glowColor};">
                <!-- Glow underlay -->
                <div class="link-glow"></div>
                {#if isTwoWay}
                  <div class="link-track track-forward"></div>
                  <div class="link-track track-reverse"></div>
                  <div class="link-dot dot-forward"></div>
                  <div class="link-dot dot-reverse"></div>
                {:else}
                  <div class="link-track track-forward"></div>
                  <div class="link-dot dot-forward"></div>
                {/if}
                <span class="link-badge" style="--badge-color: {flowColor};">
                  {link.linkLabel}
                </span>
              </div>
            </div>
            {/if}
          {/if}
        {/each}
      </div>
    </div>
    {/if}
  {/if}

  <!-- Memory Action Panel -->
  <MemoryActionPanel
    memory={selectedMemory}
    groupMemories={selectedGroupMemories}
    links={data.links}
    conversations={data.conversations}
    onClose={handlePanelClose}
    onShare={handlePanelShare}
    onUnlink={handlePanelUnlink}
    onDelete={handlePanelDelete}
  />
</div>

<style>
  .tl-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: 'Raleway', -apple-system, sans-serif;
  }

  .tl-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 14px;
    color: #4a4a6a;
  }
  .empty-icon-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    background: rgba(139, 92, 246, 0.04);
    border: 1px solid rgba(139, 92, 246, 0.06);
  }
  .tl-empty p { font-size: 12px; margin: 0; }

  /* ══════ Lane Header Wrapper ══════ */
  .lane-header-wrap {
    flex-shrink: 0;
    border-bottom: 1px solid rgba(139, 92, 246, 0.06);
    padding: 0 32px;
  }

  /* ── Character group row (spanning) ── */
  .char-row {
    display: grid;
    grid-template-columns: repeat(var(--lane-count), 1fr);
    padding: 12px 0 6px;
  }

  .char-span {
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .char-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 12px 3px 8px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 20px;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .char-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: 0 0 6px var(--char-color, rgba(139,92,246,0.4));
  }

  .char-name-text {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.2px;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
  }

  /* ── Crossroads (shared group) styling ── */
  .crossroads-pill {
    border-color: rgba(255, 159, 67, 0.15);
    background: rgba(255, 159, 67, 0.05);
    gap: 4px;
    padding: 3px 14px 3px 10px;
  }

  .crossroads-pill .char-dot {
    margin-right: -3px;
  }
  .crossroads-pill .char-dot:last-of-type {
    margin-right: 4px;
  }

  .crossroads-text {
    color: rgba(255, 159, 67, 0.7);
  }

  /* ── Participant tags on shared lanes ── */
  .lane-participants {
    display: flex;
    gap: 4px;
    justify-content: center;
    flex-wrap: wrap;
    margin-bottom: 2px;
  }

  .participant-tag {
    font-size: 7.5px;
    font-weight: 600;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--pt-color, #888);
    opacity: 0.7;
    border-left: 2px solid var(--pt-color, #888);
    padding-left: 4px;
    line-height: 1;
  }

  /* ── Lane label row ── */
  .lane-row {
    display: grid;
    grid-template-columns: repeat(var(--lane-count), 1fr);
    padding: 4px 0 0;
  }

  .lane-row:first-child {
    padding-top: 14px;
  }

  .lane-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding-bottom: 10px;
  }

  .lane-title-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    max-width: 100%;
  }

  .lane-label {
    font-size: 11px;
    font-weight: 650;
    letter-spacing: -0.2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    padding: 0 8px;
    text-align: center;
  }

  .lane-underline {
    height: 2px;
    width: 48px;
    border-radius: 2px;
    opacity: 0.6;
  }

  /* ── Group divider borders ── */
  .group-dividers {
    display: grid;
    grid-template-columns: repeat(var(--lane-count), 1fr);
    height: 3px;
  }

  .group-border {
    height: 2px;
    margin: 0 8px;
    border-radius: 1px;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--gb-color, rgba(139,92,246,0.15)) 15%,
      var(--gb-color, rgba(139,92,246,0.15)) 85%,
      transparent 100%
    );
    opacity: 0.2;
  }

  .group-border.shared-border {
    opacity: 0.35;
    height: 2px;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(255, 159, 67, 0.3) 20%,
      rgba(255, 159, 67, 0.5) 50%,
      rgba(255, 159, 67, 0.3) 80%,
      transparent 100%
    );
  }

  /* ── Character zone background in body ── */
  .char-zone {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px solid color-mix(in srgb, var(--zone-color) 6%, transparent);
    border-right: 1px solid color-mix(in srgb, var(--zone-color) 6%, transparent);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--zone-color) 3%, transparent) 0%,
      transparent 40%
    );
    pointer-events: none;
    z-index: 0;
  }

  .char-zone.shared-zone {
    border-left: 1px dashed rgba(255, 159, 67, 0.1);
    border-right: 1px dashed rgba(255, 159, 67, 0.1);
    background: linear-gradient(
      180deg,
      rgba(255, 159, 67, 0.03) 0%,
      transparent 50%
    );
  }

  /* ══════ Timeline Body ══════ */
  .tl-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 32px 60px;
  }
  .tl-scroll::-webkit-scrollbar { width: 3px; }
  .tl-scroll::-webkit-scrollbar-thumb { background: rgba(139, 92, 246, 0.12); border-radius: 3px; }

  .tl-body {
    position: relative;
    padding-top: 20px;
  }

  /* ══════ Lane Guides (vertical dotted lines) ══════ */
  .lane-guide {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    transform: translateX(-0.5px);
    background: repeating-linear-gradient(
      to bottom,
      color-mix(in srgb, var(--guide-color) 8%, transparent) 0px,
      color-mix(in srgb, var(--guide-color) 8%, transparent) 4px,
      transparent 4px,
      transparent 12px
    );
    pointer-events: none;
    z-index: 0;
  }

  /* ══════ Memory Row ══════ */
  .mem-row {
    position: relative;
    z-index: 1;
    animation: fadeUp 350ms cubic-bezier(0.16, 1, 0.3, 1) both;
    animation-delay: var(--delay);
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .mem-cell {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 8px;
  }

  .mem-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 6px;
    z-index: 2;
    transition: transform 200ms;
  }

  .mem-row:hover .mem-dot {
    transform: scale(1.4);
  }

  .mem-card {
    display: flex;
    flex: 1;
    background: linear-gradient(135deg, rgba(14, 14, 30, 0.7), rgba(10, 10, 26, 0.5));
    border: 1px solid rgba(139, 92, 246, 0.06);
    border-radius: 10px;
    overflow: hidden;
    margin-bottom: 6px;
    transition: all 250ms cubic-bezier(0.16, 1, 0.3, 1);
    min-width: 0;
  }

  .mem-row:hover .mem-card {
    border-color: color-mix(in srgb, var(--accent) 18%, transparent);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2), 0 0 0 1px color-mix(in srgb, var(--accent) 5%, transparent);
    transform: translateY(-1px);
  }

  .mem-card.is-selected {
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent), 0 4px 20px rgba(0, 0, 0, 0.3);
  }

  .card-click-target {
    display: flex;
    flex: 1;
    min-width: 0;
    cursor: pointer;
  }

  .group-child-click {
    cursor: pointer;
    border-radius: 6px;
    padding: 2px 0;
    transition: background 150ms ease;
  }

  .group-child-click:hover {
    background: rgba(139, 92, 246, 0.06);
  }

  .group-child-click.child-selected {
    background: rgba(139, 92, 246, 0.12);
  }

  .card-accent {
    width: 3px;
    flex-shrink: 0;
  }

  .card-inner {
    flex: 1;
    padding: 8px 10px;
    min-width: 0;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    gap: 5px;
    margin-bottom: 4px;
  }

  .cat-icon {
    font-size: 11px;
    flex-shrink: 0;
    line-height: 1.4;
  }

  .card-content {
    font-size: 12px;
    color: #c8c8e0;
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  .meta-time {
    font-size: 9px;
    color: #3a3a5a;
    font-family: 'JetBrains Mono', monospace;
  }

  .meta-badge {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .meta-badge.auto {
    background: rgba(0, 242, 255, 0.06);
    color: #00c4cc;
  }

  .meta-badge.version {
    background: rgba(139, 92, 246, 0.08);
    color: #a78bfa;
  }

  .meta-badge.inherited {
    background: rgba(218, 165, 32, 0.06);
    color: #d4a017;
  }

  /* ── Hidden-link indicator — this lane's memories also connect to a
     conversation currently hidden by the timeline filter above. Compact
     pill next to the lane title; hovering it spells out which
     conversation(s) it's still linked to. ── */
  .hidden-link-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 21px;
    height: 21px;
    flex-shrink: 0;
    border-radius: 50%;
    background: rgba(255, 159, 67, 0.16);
    border: 1px solid rgba(255, 159, 67, 0.4);
    color: #ff9f43;
    animation: hiddenLinkPulse 2.2s ease-in-out infinite;
    cursor: help;
  }

  @keyframes hiddenLinkPulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(255, 159, 67, 0.45); }
    50% { box-shadow: 0 0 0 5px rgba(255, 159, 67, 0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .hidden-link-pill { animation: none; }
  }

  /* ══════ Link Row ══════ */
  .link-row {
    position: relative;
    z-index: 1;
    padding: 4px 0;
    animation: fadeUp 350ms cubic-bezier(0.16, 1, 0.3, 1) both;
    animation-delay: var(--delay);
  }

  .link-line {
    position: relative;
    height: 28px;
    display: flex;
    align-items: center;
  }

  /* Glow underlay */
  .link-glow {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 8px;
    transform: translateY(-50%);
    background: var(--glow-color);
    filter: blur(6px);
    border-radius: 4px;
  }

  /* Dashed track line */
  .link-track {
    position: absolute;
    left: 0;
    right: 0;
    height: 0;
    border-top: 1.5px dashed var(--flow-color);
  }

  /* Single track: centered */
  .link-track.track-forward {
    top: 50%;
  }

  /* Two-way: offset the two tracks */
  .link-line.two-way .track-forward {
    top: calc(50% - 3px);
  }

  .link-track.track-reverse {
    top: calc(50% + 3px);
  }

  /* Traveling dot */
  .link-dot {
    position: absolute;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--flow-color);
    box-shadow: 0 0 6px var(--flow-color), 0 0 12px var(--glow-color);
    z-index: 1;
  }

  .link-dot.dot-forward {
    top: 50%;
    transform: translateY(-50%);
    animation: dotTravel 2.5s linear infinite;
  }

  /* Two-way: offset dot positions */
  .link-line.two-way .dot-forward {
    top: calc(50% - 3px);
  }

  .link-dot.dot-reverse {
    top: calc(50% + 3px);
    transform: translateY(-50%);
    animation: dotTravelReverse 2.5s linear infinite;
  }

  @keyframes dotTravel {
    0% { left: 0; }
    100% { left: calc(100% - 6px); }
  }

  @keyframes dotTravelReverse {
    0% { left: calc(100% - 6px); }
    100% { left: 0; }
  }

  /* Badge */
  .link-badge {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-size: 8px;
    font-weight: 700;
    font-family: 'Raleway', sans-serif;
    color: var(--badge-color);
    background: rgba(7, 7, 26, 0.92);
    border: 1px solid var(--badge-color);
    border-radius: 6px;
    padding: 2px 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    white-space: nowrap;
    z-index: 2;
  }

  /* ══════ Grouped Memory Card ══════ */
  .mem-card.mem-group {
    box-shadow:
      2px 2px 0 rgba(14, 14, 30, 0.8),
      4px 4px 0 rgba(14, 14, 30, 0.6);
  }

  .mem-card.mem-group.is-expanded {
    box-shadow:
      0 4px 20px rgba(0, 0, 0, 0.25),
      2px 2px 0 rgba(14, 14, 30, 0.8),
      4px 4px 0 rgba(14, 14, 30, 0.6);
    border-color: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  /* ── Expand/collapse wrapper ── */
  .group-expand-wrap {
    max-height: 0;
    overflow: hidden;
    transition: max-height 250ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .group-expand-wrap.expanded {
    /* Generous max to accommodate any realistic group size */
    max-height: 2000px;
    transition: max-height 400ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* ── Dashed divider between grouped memories ── */
  .group-divider {
    border-top: 1px dashed rgba(139, 92, 246, 0.08);
    margin: 4px 0;
  }

  /* ── Expand/collapse button ── */
  .group-expand-btn {
    display: block;
    width: 100%;
    margin-top: 4px;
    padding: 4px 8px;
    background: none;
    border: 1px dashed rgba(139, 92, 246, 0.08);
    border-radius: 6px;
    font-family: 'Raleway', -apple-system, sans-serif;
    font-size: 10px;
    font-weight: 500;
    color: #5a5a7a;
    cursor: pointer;
    text-align: center;
    text-shadow: 0 0 8px rgba(139, 92, 246, 0.2);
    transition: all 180ms ease;
  }

  .group-expand-btn:hover {
    color: #8b8ba7;
    border-color: rgba(139, 92, 246, 0.16);
    background: rgba(139, 92, 246, 0.03);
  }

  .group-expand-btn:active {
    transform: scale(0.98);
  }
</style>
