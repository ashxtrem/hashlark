// SPDX-License-Identifier: GPL-3.0-or-later

import { SvelteSet } from 'svelte/reactivity';
import type { Favorite, MergedResult } from '$lib/api/client';
import { app } from './app.svelte';
import { errorMessage, toasts } from './toasts.svelte';

/** Saved results. */
class Favorites {
  list = $state<Favorite[]>([]);
  ids = new SvelteSet<string>();
  loaded = $state(false);

  async load(): Promise<void> {
    try {
      this.list = await app.client.favorites();
      this.ids.clear();
      for (const f of this.list) this.ids.add(f.result.id);
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      this.loaded = true;
    }
  }

  has(id: string): boolean {
    return this.ids.has(id);
  }

  async toggle(result: MergedResult): Promise<void> {
    try {
      if (this.ids.has(result.id)) {
        await app.client.removeFavorite(result.id);
        this.ids.delete(result.id);
        this.list = this.list.filter((f) => f.result.id !== result.id);
      } else {
        const saved = await app.client.addFavorite(result.id);
        this.ids.add(result.id);
        this.list = [saved, ...this.list];
        toasts.success('Saved to favourites');
      }
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }
}

export const favorites = new Favorites();
