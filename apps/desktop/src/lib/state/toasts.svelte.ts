// SPDX-License-Identifier: GPL-3.0-or-later

export type ToastKind = 'info' | 'success' | 'error';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

class Toasts {
  items = $state<Toast[]>([]);
  private next = 1;

  show(message: string, kind: ToastKind = 'info', ms = 4000): void {
    const id = this.next++;
    this.items.push({ id, kind, message });
    setTimeout(() => this.dismiss(id), ms);
  }

  success(message: string): void {
    this.show(message, 'success');
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
  return typeof e === 'string' ? e : 'Something went wrong';
}
