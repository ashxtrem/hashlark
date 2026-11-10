// SPDX-License-Identifier: GPL-3.0-or-later

//! LAN sharing: receiving links from paired phones (desktop only).

import { receivedToast } from '$lib/lan';
import {
  isTauri,
  lanApi,
  listenTauri,
  type LanDevice,
  type LanReceived,
  type LanStatus,
  type PairingCode,
} from '$lib/platform';
import { errorMessage, toasts } from '$lib/state/toasts.svelte';
import { ui } from '$lib/state/ui.svelte';

class LanState {
  status = $state<LanStatus | null>(null);
  devices = $state<LanDevice[]>([]);
  busy = $state(false);
  private listening = false;

  paired = $derived(this.devices.filter((d) => d.paired));

  async load(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.status = await lanApi.status();
      await this.refreshDevices();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async refreshDevices(): Promise<void> {
    this.devices = await lanApi.devices();
  }

  async setEnabled(enabled: boolean): Promise<void> {
    this.busy = true;
    try {
      this.status = await lanApi.setEnabled(enabled);
      await this.refreshDevices();
    } catch (e) {
      toasts.error(errorMessage(e));
      this.status = await lanApi.status().catch(() => this.status);
    } finally {
      this.busy = false;
    }
  }

  async openPairing(): Promise<void> {
    try {
      this.setPairing(await lanApi.openPairing());
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async closePairing(): Promise<void> {
    this.setPairing(null);
    try {
      await lanApi.closePairing();
    } catch {
      // Already closed.
    }
  }

  async forget(device: LanDevice): Promise<void> {
    if (!confirm(`Forget ${device.name}? It won't be able to send links here until you pair again.`)) return;
    try {
      await lanApi.forget(device.id);
      await this.refreshDevices();
      toasts.success(`Forgot ${device.name}`);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  setPairing(code: PairingCode | null): void {
    if (this.status) this.status.pairing = code;
  }

  /** Subscribes once to the engine's LAN events, for every page. */
  async listen(): Promise<void> {
    if (!isTauri() || this.listening) return;
    this.listening = true;
    await Promise.all([
      listenTauri<LanReceived>('lan-received', (event) => {
        const toast = receivedToast(event);
        if (toast) toasts.show(toast.message, toast.kind, 6000);
        else ui.noHandlerMagnet = event.url;
      }),
      listenTauri<null>('lan-devices', () => {
        void this.refreshDevices().catch(() => {});
      }),
      listenTauri<LanDevice>('lan-paired', (device) => {
        toasts.success(`Paired with ${device.name}`);
        void this.refreshDevices().catch(() => {});
      }),
      listenTauri<PairingCode | null>('lan-pairing', (code) => this.setPairing(code)),
    ]);
  }
}

export const lan = new LanState();
