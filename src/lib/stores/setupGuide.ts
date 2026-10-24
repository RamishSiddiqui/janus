import { writable } from 'svelte/store';

/**
 * Set to true to open the setup guide on demand, for example from the
 * Providers page. The layout owns the dialog and resets this when it closes.
 * The first-run case is handled separately by the layout's own check.
 */
export const setupGuideRequested = writable(false);

export function openSetupGuide(): void {
  setupGuideRequested.set(true);
}
