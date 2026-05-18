-- ================================================================
--  Mythic — Branch Navigator Test Data Seed
--  Inserts a 3-turn conversation for Aria Silverleaf with:
--    - Turn 1 AI reply: 3 alternate branches (siblingIds)
--    - Turn 2 AI reply: 2 alternate branches
--    - active_message_id points to branch A (index 0) chain
-- ================================================================

-- Wipe any old messages in this conversation first
DELETE FROM messages WHERE conversation_id = 'conv-aria-main';

-- ── Turn 1: User opens the conversation ──────────────────────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-u1',
  'conv-aria-main',
  'user',
  'Are you a first-year too? I keep getting lost in the East Wing corridors.',
  NULL,
  datetime('now', '-10 minutes')
);

-- ── Turn 1 — AI Branch A (index 0) — friendly & curious ─────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a1-branch-a',
  'conv-aria-main',
  'assistant',
  '*She looks up from a hand-drawn map, silver hair catching the candlelight.*
"First year, yes — and equally lost." A wry smile curves her lips. "Though I suspect this castle *wants* us to wander. Have you found the moving staircases on Level 4 yet? They rearranged themselves last Tuesday."

She tilts the parchment toward you. The ink is still damp in places.',
  'msg-u1',
  datetime('now', '-9 minutes', '-40 seconds')
);

-- ── Turn 1 — AI Branch B (index 1) — reserved & mysterious ──────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a1-branch-b',
  'conv-aria-main',
  'assistant',
  '*She doesn''t look up immediately, finishing the annotation she''s making before lowering her quill.*
"Mm. First year." Her voice is measured, difficult to place. "The East Wing is— intentional. They built the disorientation into the architecture. Students who find their way through it unaided tend to..." She finally meets your eyes. "...stay."

A beat of silence. "You may follow me as far as the crossroads, if you wish."',
  'msg-u1',
  datetime('now', '-9 minutes', '-30 seconds')
);

-- ── Turn 1 — AI Branch C (index 2) — warm & immediately helpful ─
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a1-branch-c',
  'conv-aria-main',
  'assistant',
  '*She laughs — a genuine, surprised sound — and turns her map around so you can see it.*
"Oh, thank the stars, I thought it was just me." She gestures at a messy tangle of arrows and crossed-out routes. "I''ve been mapping the East Wing for a week. It changes, I swear. But—" she points to a circled junction "—this corridor. *This* one stays consistent. If you always come back to here, you can''t get permanently lost."

She offers her hand. "Aria. Also terminally directionally challenged."',
  'msg-u1',
  datetime('now', '-9 minutes', '-20 seconds')
);

-- ── Turn 2: User follows up (child of Branch A) ──────────────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-u2',
  'conv-aria-main',
  'user',
  'Moving staircases? That sounds terrifying. How do you even navigate?',
  'msg-a1-branch-a',
  datetime('now', '-8 minutes')
);

-- ── Turn 2 — AI Branch A (index 0) — practical ───────────────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a2-branch-a',
  'conv-aria-main',
  'assistant',
  '*She pulls a small leather-bound notebook from her satchel and flips to a hand-drawn grid — tiny numbered squares connected by dotted lines.*
"Old-fashioned cartography." She taps a row of squares near the bottom. "I log every route I walk and mark which stairs *moved* and at what time of day. The castle has patterns — it isn''t truly random." Her gaze flicks up from the page. "Want to compare notes?"',
  'msg-u2',
  datetime('now', '-7 minutes', '-50 seconds')
);

-- ── Turn 2 — AI Branch B (index 1) — playful & teasing ──────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a2-branch-b',
  'conv-aria-main',
  'assistant',
  '*She grins, snapping her notebook shut with theatrical flair.*
"Terrifying? I prefer ''character-building''." She starts walking without warning. "Come on, I''ll show you. Rule one: always have a backup staircase. Rule two: never trust a staircase that *looks* reliable — those are the worst ones." She glances over her shoulder. "Rule three: running is perfectly acceptable and no one will judge you."',
  'msg-u2',
  datetime('now', '-7 minutes', '-40 seconds')
);

-- ── Turn 3: User replies (child of Turn 2 Branch A) ─────────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-u3',
  'conv-aria-main',
  'user',
  'I would love that actually. Two cartographers are better than one.',
  'msg-a2-branch-a',
  datetime('now', '-6 minutes')
);

-- ── Turn 3 — AI response (single, no branches) ───────────────────
INSERT INTO messages (id, conversation_id, role, content, parent_id, created_at)
VALUES (
  'msg-a3',
  'conv-aria-main',
  'assistant',
  '*Something shifts in her expression — a warmth she hadn''t quite shown before.*
"Two cartographers." She seems to taste the phrase. "Yes. Alright." She opens the notebook again and smooths a fresh page flat with her palm.

"Tell me — which entrance did you come from? I haven''t triangulated the western vestibule yet."

She holds out the pen without looking up, already drawing.',
  'msg-u3',
  datetime('now', '-5 minutes')
);

-- ── Set active branch chain ───────────────────────────────────────
-- Active tip = last message in the A→A chain (msg-a3)
UPDATE conversations
SET active_message_id = 'msg-a3',
    updated_at        = datetime('now')
WHERE id = 'conv-aria-main';

-- ── Verify ────────────────────────────────────────────────────────
SELECT
  m.id,
  m.role,
  substr(m.content, 1, 55) AS preview,
  m.parent_id,
  COUNT(sib.id) AS siblings_at_this_level
FROM messages m
LEFT JOIN messages sib ON sib.parent_id = m.parent_id AND sib.conversation_id = m.conversation_id
WHERE m.conversation_id = 'conv-aria-main'
GROUP BY m.id
ORDER BY m.created_at;
