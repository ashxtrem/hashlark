// SPDX-License-Identifier: GPL-3.0-or-later

//! How the settings page turns its form into `Settings`: text inputs hold
//! strings (`''` for none), the API wants `null`.

import type { Settings } from '$lib/api/client';

type TorrentSettings = Settings['torrents'];

/** The page's own text inputs, kept outside the `draft` copy. */
export interface TextFields {
  proxy: string;
  trackersUrl: string;
  customDoh: string;
  torUrl: string;
}

const text = (v: string | null | undefined): string | null => v?.trim() || null;

/** A number from a number input; an empty or invalid one becomes `null`. */
const num = (v: number | null | undefined): number | null =>
  typeof v === 'number' && Number.isFinite(v) ? v : null;

/**
 * Torrent client settings as the API expects them: folders, proxy and
 * blocklist trimmed (empty = none), and an emptied limit, seed ratio or seed
 * time as `null` (no limit). A text field missing here is silently dropped
 * on save, so every text input of the Torrent client panel is listed.
 */
export function normalizeTorrents(t: TorrentSettings): TorrentSettings {
  return {
    ...t,
    download_dir: text(t.download_dir),
    proxy: text(t.proxy),
    blocklist_url: text(t.blocklist_url),
    watch_dir: text(t.watch_dir),
    limits: { down_kib: num(t.limits.down_kib), up_kib: num(t.limits.up_kib) },
    alt_limits: { down_kib: num(t.alt_limits.down_kib), up_kib: num(t.alt_limits.up_kib) },
    seed_ratio: num(t.seed_ratio),
    seed_minutes: num(t.seed_minutes),
    schedule: t.schedule || null,
  };
}

/** `s` with every text input of the page merged in; used for the dirty check and for saving. */
export function withTextFields(s: Settings, f: TextFields): Settings {
  return {
    ...s,
    network: {
      ...s.network,
      proxy: text(f.proxy),
      doh: { ...s.network.doh, custom_url: text(f.customDoh) },
      tor: { ...s.network.tor, socks_url: text(f.torUrl) },
    },
    magnets: { ...s.magnets, trackers_url: text(f.trackersUrl) },
    torrents: normalizeTorrents(s.torrents),
  };
}
