// ============================================================
//   Mythic — Persisted Settings Store
// ============================================================

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

type Theme = 'dark' | 'light' | 'system';

export interface AppSettings {
  theme: Theme;
  fontSize: string;
  streamingEnabled: boolean;
  autoGenerateImages: boolean;
  autoSaveMemories: boolean;
  localStorageOnly: boolean;
  systemPrompt: string;
}

const STORAGE_KEY = 'mythic-settings';

const defaultSettings: AppSettings = {
  theme: 'dark',
  fontSize: 'Medium',
  streamingEnabled: true,
  autoGenerateImages: true,
  autoSaveMemories: false,
  localStorageOnly: true,
  systemPrompt: `You are {{char}}, a character in an immersive roleplay. Stay in character at all times. Use vivid, descriptive prose with *actions* in asterisks. Never break the fourth wall. Respond naturally to the user's actions and advance the narrative.`,
};

function loadSettings(): AppSettings {
  if (!browser) return defaultSettings;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...defaultSettings, ...JSON.parse(raw) };
  } catch {
    // Corrupted storage — fall back to defaults
  }
  return defaultSettings;
}

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(loadSettings());

  // Persist on every change
  subscribe((value) => {
    if (browser) {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
      } catch {
        // Storage full or unavailable
      }
    }
  });

  return {
    subscribe,
    set,
    update,
    reset: () => set(defaultSettings),
  };
}

export const settings = createSettingsStore();

/**
 * Privacy guard — returns true when the user has opted into local-only mode.
 * Other modules should call this before making any external requests
 * beyond the core LLM provider (e.g. analytics, telemetry, cloud sync).
 */
export function isLocalOnly(): boolean {
  let value = true;
  settings.subscribe(s => { value = s.localStorageOnly; })();
  return value;
}
