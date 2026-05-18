-- ╔══════════════════════════════════════════════════════════════╗
-- ║  Comprehensive Memory Graph Seed Data                       ║
-- ║  Covers all multi-character cross-conversation scenarios    ║
-- ╚══════════════════════════════════════════════════════════════╝

-- Clear existing test data (memories + links only — keep characters & conversations)
DELETE FROM memory_links;
DELETE FROM memories;
DELETE FROM conversations WHERE id LIKE 'conv-%';

-- ═══════════════════════════════════════════════════════════════
-- CONVERSATIONS
-- ═══════════════════════════════════════════════════════════════

-- Aria: 4 conversation timelines
INSERT INTO conversations (id, title, character_id) VALUES
  ('conv-aria-main',     'Aria — College Arrival',         'char-aria-silverleaf'),
  ('conv-aria-branch1',  'Aria — Dark Forest Encounter',   'char-aria-silverleaf'),
  ('conv-aria-branch2',  'Aria — Tournament Arc',          'char-aria-silverleaf'),
  ('conv-aria-branch3',  'Aria — Crystal Caverns',         'char-aria-silverleaf');

-- Roran: 3 conversation timelines
INSERT INTO conversations (id, title, character_id) VALUES
  ('conv-roran-main',    'Roran — Forge Apprenticeship',   'char-roran-ironfist'),
  ('conv-roran-branch',  'Roran — Dragon Slayer Route',    'char-roran-ironfist'),
  ('conv-roran-branch2', 'Roran — Runic Mastery',          'char-roran-ironfist');

-- Finn: 2 conversation timelines  
INSERT INTO conversations (id, title, character_id) VALUES
  ('conv-finn-main',     'Finn — Shadow Academy',          'char-finn-shadowcloak'),
  ('conv-finn-branch',   'Finn — The Heist',               'char-finn-shadowcloak');

-- Saffron: 4 conversations (wide graph scenario)
INSERT INTO conversations (id, title, character_id) VALUES
  ('conv-saff-main',     'Saffron — Library of Echoes',    'char-saffron-emberheart'),
  ('conv-saff-b1',       'Saffron — Desert Expedition',    'char-saffron-emberheart'),
  ('conv-saff-b2',       'Saffron — Astral Projection',    'char-saffron-emberheart'),
  ('conv-saff-b3',       'Saffron — The Lost Archive',     'char-saffron-emberheart');

-- Cross-character shared conversations
INSERT INTO conversations (id, title, character_id) VALUES
  ('conv-shared-forge',  'The Forge Alliance',             'char-aria-silverleaf'),
  ('conv-shared-heist',  'Midnight Heist',                 'char-aria-silverleaf');


-- ═══════════════════════════════════════════════════════════════
-- ARIA SILVERLEAF — Rich branching + category variety
-- ═══════════════════════════════════════════════════════════════

-- Canon memories (character-level, no conversation_id)
INSERT INTO memories (id, character_id, content, source, version, is_canon) VALUES
  ('mem-aria-c1', 'char-aria-silverleaf', '[trait] Half-elf with green eyes, pointed ears, untamed elemental magic. She feels the pulse of mana like a second heartbeat.', 'user', 1, 1),
  ('mem-aria-c2', 'char-aria-silverleaf', '[relationship] Has a complicated relationship with her elven mother who abandoned her at the College gates at age seven.', 'user', 2, 1),
  ('mem-aria-c3', 'char-aria-silverleaf', '[event] Accidentally destroyed a classroom with uncontrolled fire magic during orientation — earned the nickname "Cinder".', 'auto', 1, 1),
  ('mem-aria-c4', 'char-aria-silverleaf', '[goal] Prove that half-elves can master elemental convergence — a feat no mixed-blood has achieved in three centuries.', 'user', 1, 1);

-- conv-aria-main (College Arrival) — 3 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-m1', 'char-aria-silverleaf', 'conv-aria-main', '[event] Met the user at the College courtyard while carrying a stack of elemental theory textbooks.', 'auto', 'mem-aria-c1', 1, 0),
  ('mem-aria-m2', 'char-aria-silverleaf', 'conv-aria-main', '[relationship] User helped Aria find the Elemental Studies hall — she felt grateful and opened up about her past.', 'auto', NULL, 1, 0),
  ('mem-aria-m3', 'char-aria-silverleaf', 'conv-aria-main', '[preference] User prefers to be called by their first name, not title.', 'user', NULL, 1, 0);

-- conv-aria-branch1 (Dark Forest) — 4 memories with inheritance chain
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-b1-1', 'char-aria-silverleaf', 'conv-aria-branch1', '[event] Aria and the user ventured into the Forbidden Forest to gather moonpetal herbs for a potion exam.', 'auto', 'mem-aria-c1', 1, 0),
  ('mem-aria-b1-2', 'char-aria-silverleaf', 'conv-aria-branch1', '[event] Encountered a shadow wraith — Aria discovered she can channel raw emotion into elemental bursts.', 'auto', NULL, 1, 0),
  ('mem-aria-b1-3', 'char-aria-silverleaf', 'conv-aria-branch1', '[relationship] User saved Aria from the shadow wraith, creating a deep bond of trust.', 'user', NULL, 3, 0),
  ('mem-aria-b1-4', 'char-aria-silverleaf', 'conv-aria-branch1', '[discovery] Found an ancient Elven waystone in the forest that reacted to Aria''s half-blood magic.', 'auto', 'mem-aria-b1-2', 1, 0);

-- conv-aria-branch2 (Tournament Arc) — 3 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-b2-1', 'char-aria-silverleaf', 'conv-aria-branch2', '[event] Aria entered the College tournament to prove half-elves can compete at the highest level.', 'auto', 'mem-aria-c3', 1, 0),
  ('mem-aria-b2-2', 'char-aria-silverleaf', 'conv-aria-branch2', '[event] Defeated a pure-blood elf student using a creative fusion of fire and ice — shocking the judges.', 'auto', NULL, 1, 0),
  ('mem-aria-b2-3', 'char-aria-silverleaf', 'conv-aria-branch2', '[goal] Wants to reach the tournament finals to earn a direct audience with the Archmage.', 'user', 'mem-aria-c4', 1, 0);

-- conv-aria-branch3 (Crystal Caverns) — 3 memories  
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-b3-1', 'char-aria-silverleaf', 'conv-aria-branch3', '[event] Explored the Crystal Caverns beneath the College, where mana crystallizes into physical form.', 'auto', NULL, 1, 0),
  ('mem-aria-b3-2', 'char-aria-silverleaf', 'conv-aria-branch3', '[discovery] Aria''s half-elf blood causes mana crystals to resonate at unique frequencies — potentially a new school of magic.', 'auto', 'mem-aria-b3-1', 1, 0),
  ('mem-aria-b3-3', 'char-aria-silverleaf', 'conv-aria-branch3', '[fact] The Crystal Caverns are forbidden to students, but Aria found a secret entrance through the old library.', 'user', NULL, 1, 0);


-- ═══════════════════════════════════════════════════════════════
-- RORAN IRONFIST — Moderate depth + versioned memories
-- ═══════════════════════════════════════════════════════════════

INSERT INTO memories (id, character_id, content, source, version, is_canon) VALUES
  ('mem-roran-c1', 'char-roran-ironfist', '[trait] Son of a royal blacksmith, broad-shouldered, calloused hands. Speaks with quiet intensity.', 'user', 1, 1),
  ('mem-roran-c2', 'char-roran-ironfist', '[goal] Wants to forge an unbreakable sword using runic enchantment — a technique lost for centuries.', 'user', 1, 1),
  ('mem-roran-c3', 'char-roran-ironfist', '[relationship] Respects Aria for her determination but worries her magic is too unstable for combat.', 'user', 1, 1);

-- conv-roran-main (Forge Apprenticeship) — 3 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-roran-m1', 'char-roran-ironfist', 'conv-roran-main', '[event] User visited Roran at the College forge and discussed ancient metallurgy techniques.', 'auto', 'mem-roran-c1', 1, 0),
  ('mem-roran-m2', 'char-roran-ironfist', 'conv-roran-main', '[discovery] Found a rare runestone that may hold the key to Aetherium alloy — a metal that bonds with magic.', 'user', NULL, 3, 0),
  ('mem-roran-m3', 'char-roran-ironfist', 'conv-roran-main', '[preference] Roran prefers working in silence; background noise disrupts his attunement to the metal.', 'auto', NULL, 1, 0);

-- conv-roran-branch (Dragon Slayer Route) — 3 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-roran-br1', 'char-roran-ironfist', 'conv-roran-branch', '[event] Roran took a dangerous quest to slay a dragon threatening the village near the College.', 'auto', 'mem-roran-c2', 1, 0),
  ('mem-roran-br2', 'char-roran-ironfist', 'conv-roran-branch', '[trait] Roran gained a scar across his right cheek from the dragon''s claw — wears it with pride.', 'auto', NULL, 1, 0),
  ('mem-roran-br3', 'char-roran-ironfist', 'conv-roran-branch', '[event] Used the dragon''s heartfire to temper his first runic blade — it glows faintly blue.', 'auto', 'mem-roran-br1', 1, 0);

-- conv-roran-branch2 (Runic Mastery) — 2 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-roran-b2-1', 'char-roran-ironfist', 'conv-roran-branch2', '[discovery] Deciphered an ancient runic formula that allows metal to absorb elemental energy without shattering.', 'auto', 'mem-roran-m2', 1, 0),
  ('mem-roran-b2-2', 'char-roran-ironfist', 'conv-roran-branch2', '[event] Successfully enchanted a practice dagger — but the rune destabilized after three uses.', 'auto', 'mem-roran-b2-1', 1, 0);


-- ═══════════════════════════════════════════════════════════════
-- LILA STORMWHISPER — Canon-only (no conversations, tests leaf render)
-- ═══════════════════════════════════════════════════════════════

INSERT INTO memories (id, character_id, content, source, version, is_canon) VALUES
  ('mem-lila-c1', 'char-lila-stormwhisper', '[trait] Farm girl who once commanded lightning. Freckled, red-haired, fiercely stubborn.', 'user', 1, 1),
  ('mem-lila-c2', 'char-lila-stormwhisper', '[event] Was struck by lightning at age twelve — instead of dying, she absorbed the bolt.', 'auto', 1, 1),
  ('mem-lila-c3', 'char-lila-stormwhisper', '[goal] Prove she belongs at the College despite having no formal magical education.', 'user', 1, 1),
  ('mem-lila-c4', 'char-lila-stormwhisper', '[relationship] Looks up to Saffron as a mentor figure — the first person who took her seriously.', 'user', 1, 1);


-- ═══════════════════════════════════════════════════════════════
-- FINN SHADOWCLOAK — Deep parent chain (vertical graph)
-- ═══════════════════════════════════════════════════════════════

INSERT INTO memories (id, character_id, content, source, version, is_canon) VALUES
  ('mem-finn-c1', 'char-finn-shadowcloak', '[trait] Charming rogue from a line of thieves. Mastering illusion magic to enhance his natural stealth.', 'user', 1, 1),
  ('mem-finn-c2', 'char-finn-shadowcloak', '[fact] Finn''s family sigil is a crescent moon — each member earns it by completing their first solo heist.', 'user', 1, 1);

-- conv-finn-main (Shadow Academy) — 5 chained memories (3 levels deep)
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-finn-m1', 'char-finn-shadowcloak', 'conv-finn-main', '[event] Enrolled in the Shadow Academy''s covert ops program under a false identity.', 'auto', 'mem-finn-c1', 1, 0),
  ('mem-finn-m2', 'char-finn-shadowcloak', 'conv-finn-main', '[event] Passed the first trial by pickpocketing the headmaster''s seal without detection.', 'auto', 'mem-finn-m1', 1, 0),
  ('mem-finn-m3', 'char-finn-shadowcloak', 'conv-finn-main', '[discovery] Learned that the Academy is a front for an underground resistance movement.', 'auto', 'mem-finn-m2', 4, 0),
  ('mem-finn-m4', 'char-finn-shadowcloak', 'conv-finn-main', '[relationship] Befriended a fellow student named Mira who is secretly a royal spy.', 'user', 'mem-finn-m3', 1, 0),
  ('mem-finn-m5', 'char-finn-shadowcloak', 'conv-finn-main', '[goal] Must decide whether to expose the resistance or join their cause.', 'user', 'mem-finn-m3', 1, 0);

-- conv-finn-branch (The Heist) — 2 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-finn-h1', 'char-finn-shadowcloak', 'conv-finn-branch', '[event] Finn planned a heist on the College treasury to steal a shadow crystal.', 'auto', 'mem-finn-c2', 1, 0),
  ('mem-finn-h2', 'char-finn-shadowcloak', 'conv-finn-branch', '[event] The heist went sideways — Aria Silverleaf caught him, but chose not to report him.', 'auto', 'mem-finn-h1', 1, 0);


-- ═══════════════════════════════════════════════════════════════
-- SAFFRON EMBERHEART — Wide graph (many convos, few mems each)
-- ═══════════════════════════════════════════════════════════════

INSERT INTO memories (id, character_id, content, source, version, is_canon) VALUES
  ('mem-saff-c1', 'char-saffron-emberheart', '[trait] Brilliant scholar who has read more books than half the faculty. Obsessed with recovering lost magic.', 'user', 1, 1),
  ('mem-saff-c2', 'char-saffron-emberheart', '[goal] Find the Codex Ignis — a legendary text believed to contain the secret of elemental fusion.', 'user', 1, 1);

-- conv-saff-main (Library of Echoes) — 2 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-saff-m1', 'char-saffron-emberheart', 'conv-saff-main', '[event] Discovered a hidden section in the College library that only reveals itself at midnight.', 'auto', 'mem-saff-c1', 1, 0),
  ('mem-saff-m2', 'char-saffron-emberheart', 'conv-saff-main', '[discovery] Found a fragment of the Codex Ignis — it mentions a key hidden in the Crystal Caverns.', 'user', 'mem-saff-m1', 1, 0);

-- conv-saff-b1 (Desert Expedition) — 2 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-saff-d1', 'char-saffron-emberheart', 'conv-saff-b1', '[event] Led an expedition to the Scorched Wastes, following a map from the Codex fragment.', 'auto', 'mem-saff-c2', 1, 0),
  ('mem-saff-d2', 'char-saffron-emberheart', 'conv-saff-b1', '[fact] The desert ruins contain inscriptions in pre-Elven script that only Saffron can partially read.', 'auto', NULL, 1, 0);

-- conv-saff-b2 (Astral Projection) — 1 memory
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-saff-a1', 'char-saffron-emberheart', 'conv-saff-b2', '[event] Attempted astral projection to commune with the original authors of the Codex — partially succeeded.', 'auto', 'mem-saff-m2', 1, 0);

-- conv-saff-b3 (The Lost Archive) — 2 memories
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-saff-l1', 'char-saffron-emberheart', 'conv-saff-b3', '[discovery] Located the Lost Archive beneath the desert — a vast underground library sealed for millennia.', 'auto', 'mem-saff-d1', 1, 0),
  ('mem-saff-l2', 'char-saffron-emberheart', 'conv-saff-b3', '[relationship] Met the Archive''s guardian — an ancient golem that tests visitors with riddles.', 'user', NULL, 1, 0);


-- ═══════════════════════════════════════════════════════════════
-- CROSS-CHARACTER SHARED CONVERSATIONS
-- ═══════════════════════════════════════════════════════════════

-- "The Forge Alliance" — Aria + Roran collaborate
-- Memories for Aria in this shared convo
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-forge1', 'char-aria-silverleaf', 'conv-shared-forge', '[event] Aria asked Roran to forge a focus crystal amplifier for her elemental convergence experiments.', 'auto', 'mem-aria-c4', 1, 0),
  ('mem-aria-forge2', 'char-aria-silverleaf', 'conv-shared-forge', '[relationship] Aria and Roran developed mutual respect — he tempers her recklessness, she inspires his ambition.', 'user', NULL, 1, 0);

-- Memories for Roran in this shared convo
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-roran-forge1', 'char-roran-ironfist', 'conv-shared-forge', '[event] Roran agreed to help Aria, realizing her elemental magic could be the key to stable runic enchantment.', 'auto', 'mem-roran-c2', 1, 0),
  ('mem-roran-forge2', 'char-roran-ironfist', 'conv-shared-forge', '[discovery] The fusion of Aria''s fire magic and Roran''s runecraft created a prototype that held for ten minutes.', 'auto', 'mem-roran-forge1', 1, 0);

-- "Midnight Heist" — Aria + Finn cross paths
-- Memories for Aria
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-aria-heist1', 'char-aria-silverleaf', 'conv-shared-heist', '[event] Caught Finn attempting to steal from the restricted section. Chose to help him instead of reporting.', 'auto', NULL, 1, 0),
  ('mem-aria-heist2', 'char-aria-silverleaf', 'conv-shared-heist', '[relationship] Finn owes Aria a favor — an uneasy alliance between a mage and a rogue.', 'user', 'mem-aria-heist1', 1, 0);

-- Memories for Finn in this shared convo
INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon) VALUES
  ('mem-finn-heist1', 'char-finn-shadowcloak', 'conv-shared-heist', '[event] Aria caught Finn during the heist but offered a deal — she keeps quiet if he teaches her shadow step.', 'auto', 'mem-finn-c1', 1, 0),
  ('mem-finn-heist2', 'char-finn-shadowcloak', 'conv-shared-heist', '[fact] Aria''s elemental aura makes her impossible to sneak up on — Finn finds this both annoying and impressive.', 'auto', NULL, 1, 0);


-- ═══════════════════════════════════════════════════════════════
-- MEMORY LINKS — All link type/direction combinations
-- ═══════════════════════════════════════════════════════════════

INSERT INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id) VALUES
  -- Copy one-way: Aria's canon trait copied to Dark Forest branch
  ('link-01', 'mem-aria-c1', 'conv-aria-branch1', 'copy', 'one_way', 'manual', 'mem-aria-b1-1'),
  
  -- Copy one-way: Aria's relationship canon copied to Tournament
  ('link-02', 'mem-aria-c2', 'conv-aria-branch2', 'copy', 'one_way', 'manual', 'mem-aria-b2-1'),
  
  -- Sync one-way: Aria's College friendship auto-pushes to Tournament
  ('link-03', 'mem-aria-m2', 'conv-aria-branch2', 'sync', 'one_way', 'auto', NULL),
  
  -- Sync two-way: Cross-branch sync between Dark Forest discovery and Crystal Caverns
  ('link-04', 'mem-aria-b1-4', 'conv-aria-branch3', 'sync', 'two_way', 'auto', 'mem-aria-b3-2'),
  
  -- Sync two-way: Roran's runestone discovery synced to Dragon route
  ('link-05', 'mem-roran-m2', 'conv-roran-branch', 'sync', 'two_way', 'auto', NULL),
  
  -- Copy one-way: Roran's runestone discovery → Runic Mastery branch
  ('link-06', 'mem-roran-m2', 'conv-roran-branch2', 'copy', 'one_way', 'manual', 'mem-roran-b2-1'),
  
  -- Sync one-way: Saffron's Codex fragment syncs to Desert expedition
  ('link-07', 'mem-saff-m2', 'conv-saff-b1', 'sync', 'one_way', 'auto', NULL),
  
  -- Copy one-way: Saffron's desert finding copied to Lost Archive
  ('link-08', 'mem-saff-d1', 'conv-saff-b3', 'copy', 'one_way', 'manual', 'mem-saff-l1'),
  
  -- Sync two-way: Saffron's Codex fragment synced to Astral branch
  ('link-09', 'mem-saff-m2', 'conv-saff-b2', 'sync', 'two_way', 'auto', 'mem-saff-a1'),

  -- Cross-character: Aria's main relationship memory synced into the shared Forge conversation
  ('link-10', 'mem-aria-m2', 'conv-shared-forge', 'sync', 'two_way', 'auto', 'mem-roran-forge2'),
  
  -- Cross-character: Finn's heist memory copied from his Heist conv context  
  ('link-11', 'mem-finn-h2', 'conv-shared-heist', 'copy', 'one_way', 'manual', 'mem-finn-heist1'),

  -- Sync one-way: Finn's Academy discovery pushes to The Heist context
  ('link-12', 'mem-finn-m3', 'conv-finn-branch', 'sync', 'one_way', 'auto', NULL);
