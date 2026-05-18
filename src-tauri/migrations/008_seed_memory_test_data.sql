-- Seed data for Memory Management testing.
-- Creates conversations, memories, and memory_links that exercise every feature:
--   1. Canon memories (is_canon=1) — character-level roots
--   2. Conversation-scoped memories — branched from canon via parent_id
--   3. Auto-extracted memories (source='auto') vs user-pinned (source='user')
--   4. Multi-version memories (version > 1) — edited memories
--   5. Memory links: copy + one_way + manual
--   6. Memory links: copy + two_way + auto
--   7. Memory links: sync + one_way + auto
--   8. Memory links: sync + two_way + manual

-- ============================================================
-- Conversations for Aria Silverleaf (3 divergent timelines)
-- ============================================================

INSERT OR IGNORE INTO conversations (id, title, character_id) VALUES
('conv-aria-main',    'Aria — College Arrival',       'char-aria-silverleaf'),
('conv-aria-branch1', 'Aria — Dark Forest Encounter', 'char-aria-silverleaf'),
('conv-aria-branch2', 'Aria — Tournament Arc',        'char-aria-silverleaf');

-- ============================================================
-- Conversations for Roran Ironfist (2 timelines)
-- ============================================================

INSERT OR IGNORE INTO conversations (id, title, character_id) VALUES
('conv-roran-main',   'Roran — Forge Apprenticeship', 'char-roran-ironfist'),
('conv-roran-branch', 'Roran — Dragon Slayer Route',  'char-roran-ironfist');

-- ============================================================
-- CANON MEMORIES — character-level roots (is_canon = 1)
-- ============================================================

-- Aria canon memories (3)
INSERT OR IGNORE INTO memories (id, character_id, content, source, parent_id, version, is_canon) VALUES
('mem-aria-canon-1', 'char-aria-silverleaf',
 '[trait] Half-elf with green eyes, pointed ears, untamed elemental magic affinity',
 'user', NULL, 1, 1),

('mem-aria-canon-2', 'char-aria-silverleaf',
 '[relationship] Has a complicated relationship with her elven mother — admires but resents her absence',
 'user', NULL, 2, 1),

('mem-aria-canon-3', 'char-aria-silverleaf',
 '[event] Accidentally destroyed a classroom with uncontrolled fire magic during orientation week',
 'auto', NULL, 1, 1);

-- Roran canon memories (2)
INSERT OR IGNORE INTO memories (id, character_id, content, source, parent_id, version, is_canon) VALUES
('mem-roran-canon-1', 'char-roran-ironfist',
 '[trait] Son of a royal blacksmith, broad-shouldered, calloused hands, practical and stubborn',
 'user', NULL, 1, 1),

('mem-roran-canon-2', 'char-roran-ironfist',
 '[goal] Wants to forge an unbreakable sword using runic enchantment — his lifelong ambition',
 'user', NULL, 1, 1);

-- ============================================================
-- CONVERSATION-SCOPED MEMORIES — branched from canon (parent_id set)
-- ============================================================

-- Aria Main conversation memories (forked from canon)
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-aria-main-1', 'char-aria-silverleaf', 'conv-aria-main',
 '[event] Met the user at the College courtyard while carrying spell tomes',
 'auto', 'mem-aria-canon-1', 1, 0),

('mem-aria-main-2', 'char-aria-silverleaf', 'conv-aria-main',
 '[relationship] User helped Aria find the Elemental Studies hall — she feels grateful',
 'auto', NULL, 1, 0),

('mem-aria-main-3', 'char-aria-silverleaf', 'conv-aria-main',
 '[preference] User prefers to be called by their first name, not title',
 'user', NULL, 1, 0);

-- Aria Branch 1: Dark Forest (divergent timeline)
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-aria-b1-1', 'char-aria-silverleaf', 'conv-aria-branch1',
 '[event] Aria and the user ventured into the Forbidden Forest on a dare from Finn',
 'auto', 'mem-aria-canon-1', 1, 0),

('mem-aria-b1-2', 'char-aria-silverleaf', 'conv-aria-branch1',
 '[event] Encountered a shadow wraith — Aria discovered she can sense dark magic entities',
 'auto', NULL, 1, 0),

('mem-aria-b1-3', 'char-aria-silverleaf', 'conv-aria-branch1',
 '[relationship] User saved Aria from the shadow wraith — deep bond formed, trust level high',
 'user', NULL, 3, 0);

-- Aria Branch 2: Tournament (another divergent timeline)
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-aria-b2-1', 'char-aria-silverleaf', 'conv-aria-branch2',
 '[event] Aria entered the College tournament to prove half-elves can compete',
 'auto', 'mem-aria-canon-3', 1, 0),

('mem-aria-b2-2', 'char-aria-silverleaf', 'conv-aria-branch2',
 '[event] Defeated a pure-blood elf student using a creative fire-and-wind combo spell',
 'auto', NULL, 1, 0);

-- Roran Main conversation memories
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-roran-main-1', 'char-roran-ironfist', 'conv-roran-main',
 '[event] User visited Roran at the College forge and discussed enchantment techniques',
 'auto', 'mem-roran-canon-1', 1, 0),

('mem-roran-main-2', 'char-roran-ironfist', 'conv-roran-main',
 '[discovery] Found a rare runestone that may hold the key to unbreakable enchantments',
 'user', NULL, 2, 0);

-- Roran Branch: Dragon Slayer
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-roran-br-1', 'char-roran-ironfist', 'conv-roran-branch',
 '[event] Roran took a dangerous quest to slay a dragon threatening the eastern villages',
 'auto', 'mem-roran-canon-2', 1, 0),

('mem-roran-br-2', 'char-roran-ironfist', 'conv-roran-branch',
 '[trait] Roran gained a scar across his right cheek from the dragon fight — wears it proudly',
 'auto', NULL, 1, 0);

-- ============================================================
-- MEMORY LINKS — all 4 combinations of (link_type × direction × sync_mode)
-- ============================================================

-- Link 1: COPY + ONE_WAY + MANUAL
-- Canon memory shared to Branch 1 as a frozen copy
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-link-copy-1', 'char-aria-silverleaf', 'conv-aria-branch1',
 '[trait] Half-elf with green eyes, pointed ears, untamed elemental magic affinity',
 'auto', 'mem-aria-canon-1', 1, 0);

INSERT OR IGNORE INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id) VALUES
('link-1', 'mem-aria-canon-1', 'conv-aria-branch1', 'copy', 'one_way', 'manual', 'mem-link-copy-1');

-- Link 2: COPY + TWO_WAY + AUTO
-- Canon relationship memory shared to Tournament arc
INSERT OR IGNORE INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
('mem-link-copy-2', 'char-aria-silverleaf', 'conv-aria-branch2',
 '[relationship] Has a complicated relationship with her elven mother — admires but resents her absence',
 'auto', 'mem-aria-canon-2', 1, 0);

INSERT OR IGNORE INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id) VALUES
('link-2', 'mem-aria-canon-2', 'conv-aria-branch2', 'copy', 'two_way', 'auto', 'mem-link-copy-2');

-- Link 3: SYNC + ONE_WAY + AUTO
-- Main conversation memory live-synced to Branch 2 (one-way, auto-push)
INSERT OR IGNORE INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id) VALUES
('link-3', 'mem-aria-main-2', 'conv-aria-branch2', 'sync', 'one_way', 'auto', NULL);

-- Link 4: SYNC + TWO_WAY + MANUAL
-- Roran's runestone discovery synced between his two timelines (bidirectional, manual)
INSERT OR IGNORE INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id) VALUES
('link-4', 'mem-roran-main-2', 'conv-roran-branch', 'sync', 'two_way', 'manual', NULL);
