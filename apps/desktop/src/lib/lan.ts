// SPDX-License-Identifier: GPL-3.0-or-later

//! Wording for LAN sharing, kept apart from the state so it can be tested.

import { isLanError, type LanDevice, type LanReceived } from '$lib/platform';
import type { ToastKind } from '$lib/state/toasts.svelte';

/** "1:05" for 65 000 ms; never negative. */
export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}

/** Paired devices (those on the network first), then devices that could be paired. */
export function pickerSections(devices: LanDevice[]): { paired: LanDevice[]; others: LanDevice[] } {
  const byName = (a: LanDevice, b: LanDevice) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  const paired = devices.filter((d) => d.paired).sort((a, b) => Number(b.online) - Number(a.online) || byName(a, b));
  const others = devices.filter((d) => !d.paired).sort(byName);
  return { paired, others };
}

export function isPin(text: string): boolean {
  return /^\d{6}$/.test(text);
}

/** Where the other device shows its PIN. */
export function pinHint(device: LanDevice): string {
  return device.platform === 'android'
    ? `On ${device.name}, open Hashlark → Settings → Devices on this network → Show pairing PIN, then enter the PIN it shows.`
    : `On ${device.name}, open Hashlark → Settings → Devices on this network → Pair a phone, then enter the PIN it shows.`;
}

/** What to tell the user when talking to `deviceName` failed. */
export function lanErrorMessage(e: unknown, deviceName: string): string {
  if (!isLanError(e)) return e instanceof Error ? e.message : String(e);
  switch (e.code) {
    case 'unreachable':
      return `Couldn't reach ${deviceName}. Check that it's on the same Wi-Fi and Hashlark is open there.`;
    case 'rejected':
      return `${deviceName} no longer recognises this computer. Pair again.`;
    case 'pairing_closed':
      return `${deviceName} isn't ready to pair yet, or its PIN expired. Show a new PIN there and try again.`;
    case 'wrong_pin':
      return `That PIN isn't right. Check the PIN on ${deviceName}.`;
    case 'rate_limited':
      return 'Too many attempts. Wait a minute and try again.';
    default:
      return e.message;
  }
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
    case 'added':
      return { kind: 'success', message: `Downloading ${what} from ${event.from}` };
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
