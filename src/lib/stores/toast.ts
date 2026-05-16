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
}

export const toasts = writable<Toast[]>([]);

let counter = 0;

export function addToast(message: string, type: Toast['type'] = 'info', duration = 3000) {
  const id = `toast-${++counter}`;
  toasts.update(t => [...t, { id, message, type, duration }]);

  setTimeout(() => {
    toasts.update(t => t.filter(toast => toast.id !== id));
  }, duration);
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
