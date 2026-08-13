// ============================================================
//   Janus — Toast Notification Store
//   Lightweight global toast system for user feedback
// ============================================================

import { writable } from 'svelte/store';

export interface Toast {
  id: string;
  message: string;
  type: 'success' | 'error' | 'info';
  duration: number;
  action?: { label: string; onClick: () => void };
}

interface TimerState {
  timeoutId: ReturnType<typeof setTimeout> | null;
  /** Milliseconds left whenever the timer isn't currently running (paused,
   *  or not yet started). Updated on pause so resume can pick up where it
   *  left off instead of restarting the full duration. */
  remaining: number;
  startedAt: number;
  onExpire: () => void;
}

export const toasts = writable<Toast[]>([]);

let counter = 0;
const timers = new Map<string, TimerState>();

function removeToastFromStore(id: string) {
  toasts.update(t => t.filter(toast => toast.id !== id));
}

function startTimer(id: string, duration: number, onExpire: () => void) {
  const timeoutId = duration > 0 ? setTimeout(() => {
    timers.delete(id);
    onExpire();
  }, duration) : null;
  timers.set(id, { timeoutId, remaining: duration, startedAt: Date.now(), onExpire });
}

/** Pauses a toast's auto-dismiss/commit timer — used on hover, so the user
 *  can read (or copy) a message without it vanishing mid-read. */
export function pauseToast(id: string) {
  const timer = timers.get(id);
  if (!timer || timer.timeoutId === null) return;
  clearTimeout(timer.timeoutId);
  timer.remaining = Math.max(0, timer.remaining - (Date.now() - timer.startedAt));
  timer.timeoutId = null;
}

/** Resumes a paused toast's timer with whatever time was left when it was
 *  paused (not the full original duration). */
export function resumeToast(id: string) {
  const timer = timers.get(id);
  if (!timer || timer.timeoutId !== null || timer.remaining <= 0) return;
  timer.startedAt = Date.now();
  timer.timeoutId = setTimeout(() => {
    timers.delete(id);
    timer.onExpire();
  }, timer.remaining);
}

/** Immediately dismisses a toast (e.g. its close button) without running
 *  whatever `onExpire` would otherwise have done. */
export function dismissToast(id: string) {
  const timer = timers.get(id);
  if (timer?.timeoutId) clearTimeout(timer.timeoutId);
  timers.delete(id);
  removeToastFromStore(id);
}

export function addToast(message: string, type: Toast['type'] = 'info', duration = 3000, action?: Toast['action']): string {
  const id = `toast-${++counter}`;
  toasts.update(t => [...t, { id, message, type, duration, action }]);
  startTimer(id, duration, () => removeToastFromStore(id));
  return id;
}

export function success(message: string) {
  addToast(message, 'success');
}

export function error(message: string) {
  // Longer than the other toast types, and now pausable-on-hover + closable
  // (see ToastContainer.svelte) — error messages carry real diagnostic
  // detail worth actually reading, not just a glance.
  addToast(message, 'error', 8000);
}

export function info(message: string) {
  addToast(message, 'info');
}

/**
 * Shows a toast with an "Undo" action and defers `commit` for `delayMs`.
 * The caller is expected to have already optimistically removed the item
 * from the UI before calling this — `onUndo` should restore it. If the
 * window elapses without Undo being clicked, `commit` runs (typically the
 * actual backend delete call). Used for delete confirmations that don't
 * warrant a full soft-delete/trash system: the item never actually leaves
 * the database until the undo window has passed.
 *
 * The commit timer is the SAME pausable timer that drives the toast's own
 * on-screen lifetime (via `onExpire`) — hovering the toast (see
 * ToastContainer.svelte) pauses the commit exactly as long as the toast
 * stays visible, so a delete can never silently commit while the user is
 * still looking at (or about to click) the Undo button.
 */
export function undoableDelete(
  message: string,
  commit: () => void | Promise<void>,
  onUndo: () => void,
  delayMs = 5500,
) {
  let undone = false;
  const id = addToast(message, 'info', delayMs, {
    label: 'Undo',
    onClick: () => {
      undone = true;
      dismissToast(id);
      onUndo();
    },
  });
  const timer = timers.get(id);
  if (timer) {
    timer.onExpire = () => {
      removeToastFromStore(id);
      if (!undone) commit();
    };
  }
}
