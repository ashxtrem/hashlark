// SPDX-License-Identifier: GPL-3.0-or-later

import type { ProviderView } from '$lib/api/client';
import { app } from './app.svelte';
import { errorMessage } from './toasts.svelte';

class ProvidersState {
  list = $state<ProviderView[]>([]);
  loaded = $state(false);
  error = $state<string | null>(null);

  /** Display name for each provider id. */
  names = $derived(Object.fromEntries(this.list.map((p) => [p.id, p.name])));
  enabledCount = $derived(this.list.filter((p) => p.enabled && !p.error).length);

  async load(): Promise<void> {
    try {
      this.list = await app.client.providers();
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.loaded = true;
    }
  }

  /** Replaces one provider after an update. */
  put(view: ProviderView): void {
    const i = this.list.findIndex((p) => p.id === view.id);
    if (i === -1) this.list.push(view);
    else this.list[i] = view;
  }

  remove(id: string): void {
    this.list = this.list.filter((p) => p.id !== id);
  }
}

export const providers = new ProvidersState();
