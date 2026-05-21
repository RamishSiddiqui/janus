/**
 * groupMemories.ts
 * 
 * Groups consecutive memories within each lane that don't have
 * link events (copy/sync) between them into single logical groups.
 * 
 * Used by both MemoryTimeline and MemoryGraph to reduce visual clutter.
 */

import type { Memory, MemoryLink } from '$lib/services/ipc';

export interface MemoryItem {
  memory: Memory;
  category: string;
  categoryIcon: string;
  content: string;  // category-prefix stripped
}

export interface MemoryGroup {
  id: string;             // first memory's id
  laneId: string;         // 'canon' or conversation_id
  memories: MemoryItem[];
  isGroup: boolean;       // true if 2+ memories
}

const CATEGORY_ICONS: Record<string, string> = {
  trait: '🧬',
  event: '⚡',
  relationship: '💫',
  goal: '🎯',
  discovery: '🔮',
  preference: '💭',
  fact: '📋',
};

function parseMemory(m: Memory): MemoryItem {
  const catMatch = m.content.match(/^\[(\w+)\]\s*/);
  const category = catMatch ? catMatch[1].toLowerCase() : 'fact';
  const content = catMatch ? m.content.slice(catMatch[0].length) : m.content;
  return {
    memory: m,
    category,
    categoryIcon: CATEGORY_ICONS[category] ?? '📋',
    content,
  };
}

/**
 * Groups memories by lane, splitting at link boundaries.
 * 
 * Algorithm:
 * 1. Build a set of memory IDs that are sources of links
 * 2. Sort memories by lane, then chronologically
 * 3. Walk through each lane's memories sequentially
 * 4. If the current memory is a link source, it becomes the LAST item
 *    in the current group, and the next memory starts a new group
 * 5. Result: array of MemoryGroup, one per contiguous run
 */
export function groupMemories(
  memories: Memory[],
  links: MemoryLink[],
): MemoryGroup[] {
  // Set of memory IDs that are sources of links (copy/sync)
  const linkSourceIds = new Set(links.map(l => l.source_memory_id));

  // Group raw memories by lane (canon vs conversation_id)
  const byLane = new Map<string, Memory[]>();
  for (const m of memories) {
    const laneId = m.is_canon ? 'canon' : (m.conversation_id ?? 'canon');
    if (!byLane.has(laneId)) byLane.set(laneId, []);
    byLane.get(laneId)!.push(m);
  }

  const result: MemoryGroup[] = [];

  for (const [laneId, laneMems] of byLane) {
    // Sort chronologically within lane
    laneMems.sort((a, b) => a.created_at.localeCompare(b.created_at));

    let currentGroup: MemoryItem[] = [];

    for (const m of laneMems) {
      const item = parseMemory(m);
      currentGroup.push(item);

      // If this memory is a link source, close the current group
      // (this memory is the tail of the group, link appears after it)
      if (linkSourceIds.has(m.id)) {
        result.push({
          id: currentGroup[0].memory.id,
          laneId,
          memories: currentGroup,
          isGroup: currentGroup.length > 1,
        });
        currentGroup = [];
      }
    }

    // Flush remaining memories as a group
    if (currentGroup.length > 0) {
      result.push({
        id: currentGroup[0].memory.id,
        laneId,
        memories: currentGroup,
        isGroup: currentGroup.length > 1,
      });
    }
  }

  return result;
}

/**
 * Variant for Timeline: returns a flat list of "renderable rows"
 * that interleave groups and link rows in the correct order.
 */
export interface TimelineGroup {
  type: 'group';
  id: string;
  laneId: string;
  memories: MemoryItem[];
  isGroup: boolean;
  time: string;          // first memory's created_at
  sortKey: string;
}

export interface TimelineLinkRow {
  type: 'link';
  time: string;
  sortKey: string;
  linkLabel: string;
  fromLaneId: string;
  toLaneId: string;
  linkType: string;
}

export type TimelineEntry = TimelineGroup | TimelineLinkRow;

export function buildTimelineEntries(
  memories: Memory[],
  links: MemoryLink[],
): TimelineEntry[] {
  const linkSourceIds = new Set(links.map(l => l.source_memory_id));
  const entries: TimelineEntry[] = [];

  // Build lane map: memory_id → lane_id
  const memLaneMap = new Map<string, string>();
  memories.forEach(m => {
    memLaneMap.set(m.id, m.is_canon ? 'canon' : (m.conversation_id ?? 'canon'));
  });

  // Group raw memories by lane
  const byLane = new Map<string, Memory[]>();
  for (const m of memories) {
    const laneId = m.is_canon ? 'canon' : (m.conversation_id ?? 'canon');
    if (!byLane.has(laneId)) byLane.set(laneId, []);
    byLane.get(laneId)!.push(m);
  }

  // Track which links we've placed (so we can interleave them)
  const linksBySource = new Map<string, MemoryLink[]>();
  for (const l of links) {
    if (!linksBySource.has(l.source_memory_id)) linksBySource.set(l.source_memory_id, []);
    linksBySource.get(l.source_memory_id)!.push(l);
  }

  let globalIdx = 0;

  for (const [laneId, laneMems] of byLane) {
    laneMems.sort((a, b) => a.created_at.localeCompare(b.created_at));

    let currentGroup: MemoryItem[] = [];
    let groupStartIdx = globalIdx;

    for (const m of laneMems) {
      const item = parseMemory(m);
      currentGroup.push(item);

      if (linkSourceIds.has(m.id)) {
        // Emit the group
        const firstMem = currentGroup[0].memory;
        entries.push({
          type: 'group',
          id: firstMem.id,
          laneId,
          memories: currentGroup,
          isGroup: currentGroup.length > 1,
          time: firstMem.created_at,
          sortKey: `${firstMem.created_at}_${String(groupStartIdx).padStart(4, '0')}_a`,
        });

        // Emit link rows after this group
        const memLinks = linksBySource.get(m.id) ?? [];
        memLinks.forEach((link, li) => {
          const isSync = link.link_type === 'sync';
          const isTwoWay = link.direction === 'two_way';
          const label = isSync
            ? (isTwoWay ? '⇄ sync' : '→ sync')
            : (isTwoWay ? '⇄ copy' : '→ copy');

          entries.push({
            type: 'link',
            time: m.created_at,
            sortKey: `${m.created_at}_${String(groupStartIdx).padStart(4, '0')}_b${String(li).padStart(3, '0')}`,
            fromLaneId: laneId,
            toLaneId: link.target_conversation_id,
            linkLabel: label,
            linkType: link.link_type,
          });
        });

        globalIdx++;
        groupStartIdx = globalIdx;
        currentGroup = [];
      }
    }

    // Flush remaining
    if (currentGroup.length > 0) {
      const firstMem = currentGroup[0].memory;
      entries.push({
        type: 'group',
        id: firstMem.id,
        laneId,
        memories: currentGroup,
        isGroup: currentGroup.length > 1,
        time: firstMem.created_at,
        sortKey: `${firstMem.created_at}_${String(groupStartIdx).padStart(4, '0')}_a`,
      });
      globalIdx++;
    }
  }

  // Sort everything by sortKey
  entries.sort((a, b) => a.sortKey.localeCompare(b.sortKey));
  return entries;
}
