// SPDX-License-Identifier: GPL-3.0-or-later

//! What happens when the user downloads, copies or opens a result.

import type { MergedResult } from '$lib/api/client';
import { copyText, isTauri, openMagnet, openTorrent } from '$lib/platform';
import { app } from '$lib/state/app.svelte';
import { errorMessage, toasts } from '$lib/state/toasts.svelte';
import { providers } from '$lib/state/providers.svelte';
import { ui } from '$lib/state/ui.svelte';

/** Opens the result in the user's torrent client (ADR 0008). */
export async function download(result: MergedResult): Promise<void> {
  try {
    const target = await app.client.resolve(result.id);
    if (target.type === 'magnet') {
      const outcome = await openMagnet(target.url);
      if (outcome === 'no_handler') ui.noHandlerMagnet = target.url;
      else if (isTauri()) toasts.success('Sent to your torrent client');
    } else {
      const saved = await openTorrent(target.url, result.primary.title);
      if (saved) toasts.success(`Saved ${saved}`);
    }
  } catch (e) {
    toasts.error(`Could not open this result: ${errorMessage(e)}`);
  }
}

export async function copyMagnet(result: MergedResult): Promise<void> {
  try {
    const target = await app.client.resolve(result.id, 'magnet');
    if (target.type !== 'magnet') {
      toasts.error('This result has no magnet link; use the .torrent instead.');
      return;
    }
    await copyText(target.url);
    toasts.success('Magnet link copied');
  } catch (e) {
    toasts.error(`Could not copy the magnet link: ${errorMessage(e)}`);
  }
}

export async function saveTorrent(result: MergedResult): Promise<void> {
  try {
    const target = await app.client.resolve(result.id, 'torrent_file');
    if (target.type !== 'torrent_file') {
      toasts.error('This result has no .torrent file; use the magnet link instead.');
      return;
    }
    const saved = await openTorrent(target.url, result.primary.title);
    if (saved) toasts.success(`Saved ${saved}`);
  } catch (e) {
    toasts.error(`Could not get the .torrent: ${errorMessage(e)}`);
  }
}

/** Starts the browser-check flow for a provider that needs one. */
export function startChallenge(providerId: string): void {
  const provider = providers.list.find((p) => p.id === providerId);
  const url = provider?.source.type === 'definition' ? provider.source.links[0] : undefined;
  if (!provider || !url) {
    toasts.error('This provider has no web page to open.');
    return;
  }
  ui.challenge = { providerId, name: provider.name, url };
}

/** Whether the result can offer each kind of link without asking the provider. */
export function linkKinds(result: MergedResult): { magnet: boolean; torrent: boolean } {
  const p = result.primary;
  return {
    magnet: Boolean(p.magnet || p.info_hash || p.needs_resolve),
    torrent: Boolean(p.torrent_url && !p.needs_resolve),
  };
}
