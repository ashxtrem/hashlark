// SPDX-License-Identifier: GPL-3.0-or-later

export type ToastKind = 'info' | 'success' | 'error';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** A link-style button, e.g. "Show". */
  action?: ToastAction;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

class Toasts {
  items = $state<Toast[]>([]);
  private next = 1;

  show(message: string, kind: ToastKind = 'info', ms = 4000, action?: ToastAction): void {
    const id = this.next++;
    this.items.push({ id, kind, message, action });
    setTimeout(() => this.dismiss(id), ms);
  }

  success(message: string, action?: ToastAction): void {
    // Toasts with an action stay longer so keyboard users can reach it.
    this.show(message, 'success', action ? 10000 : 4000, action);
  }

  error(message: string): void {
    this.show(message, 'error', 7000);
  }

  dismiss(id: number): void {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new Toasts();

/** Message for a caught error. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  // Desktop commands can fail with `{ code, message }`.
  if (typeof e === 'object' && e !== null && 'message' in e) return String(e.message);
  return typeof e === 'string' ? e : 'Something went wrong';
}
