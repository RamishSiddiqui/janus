// ============================================================
//   Janus — Selected Persona (for new-chat pre-selection)
// ============================================================
//
// Remembers which persona was last picked so it's pre-filled the next time
// the user starts a new chat, without needing a bespoke modal at every
// "start chat" call site. Per-conversation persona selection (changing it
// after the conversation exists) goes through `set_conversation_persona`
// via ContextPersonaPanel.svelte instead — this store only seeds the
// initial choice at creation time.

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'mythic-selected-persona-id';

function loadSelectedPersonaId(): string | null {
  if (!browser) return null;
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function createSelectedPersonaStore() {
  const { subscribe, set } = writable<string | null>(loadSelectedPersonaId());

  subscribe((value) => {
    if (!browser) return;
    try {
      if (value) {
        localStorage.setItem(STORAGE_KEY, value);
      } else {
        localStorage.removeItem(STORAGE_KEY);
      }
    } catch {
      // Storage full or unavailable
    }
  });

  return { subscribe, set };
}

export const selectedPersonaId = createSelectedPersonaStore();
