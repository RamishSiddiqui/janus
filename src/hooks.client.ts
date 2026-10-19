import type { ClientInit } from '@sveltejs/kit';
import { initWebMode } from '$lib/services/webBridge';

// Runs before the first page renders. In a browser served by Janus this puts
// the Tauri stand-in in place, so components that check for the backend (and
// every invoke/listen call) work the same as in the desktop app.
export const init: ClientInit = async () => {
  await initWebMode();
};
