// ============================================================
//   Mythic — Toast Notification Store
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

export const toasts = writable<Toast[]>([]);

let counter = 0;

export function addToast(message: string, type: Toast['type'] = 'info', duration = 3000, action?: Toast['action']) {
  const id = `toast-${++counter}`;
  toasts.update(t => [...t, { id, message, type, duration, action }]);

  setTimeout(() => {
    toasts.update(t => t.filter(toast => toast.id !== id));
  }, duration);

  return id;
}

export function success(message: string) {
  addToast(message, 'success');
}

export function error(message: string) {
  addToast(message, 'error', 4000);
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
 */
export function undoableDelete(
  message: string,
  commit: () => void | Promise<void>,
  onUndo: () => void,
  delayMs = 5500,
) {
  let undone = false;
  const timer = setTimeout(() => {
    if (!undone) commit();
  }, delayMs);

  addToast(message, 'info', delayMs, {
    label: 'Undo',
    onClick: () => {
      undone = true;
      clearTimeout(timer);
      onUndo();
    },
  });
}
