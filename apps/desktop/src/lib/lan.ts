// SPDX-License-Identifier: GPL-3.0-or-later

//! Wording for LAN sharing, kept apart from the state so it can be tested.

import type { LanReceived } from '$lib/platform';
import type { ToastKind } from '$lib/state/toasts.svelte';

/** "1:05" for 65 000 ms; never negative. */
export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}

/** "123 456": easier to read out and type. */
export function formatPin(pin: string): string {
  return pin.length === 6 ? `${pin.slice(0, 3)} ${pin.slice(3)}` : pin;
}

/**
 * The toast for a received link, or `null` when the no-torrent-client
 * dialog should explain instead.
 */
export function receivedToast(event: LanReceived): { kind: ToastKind; message: string } | null {
  const what = event.title ? `“${event.title}”` : 'a link';
  switch (event.outcome.status) {
    case 'opened':
      return { kind: 'success', message: `Opened ${what} from ${event.from}` };
    case 'no_handler':
      return null;
    case 'failed':
      return {
        kind: 'error',
        message: `Couldn't open ${what} from ${event.from}: ${event.outcome.message}`,
      };
  }
}
