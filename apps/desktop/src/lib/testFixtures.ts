// SPDX-License-Identifier: GPL-3.0-or-later

//! Settings and results for tests (never imported by the app).

import type { MergedResult, Settings } from '$lib/api/client';

/** The server's default settings. */
export function defaultSettings(): Settings {
  return {
    search: { cache_ttl_secs: 600, max_concurrency: 16, provider_timeout_secs: 10, save_history: true },
    network: {
      doh: { custom_url: null, enabled: true, fallback_to_system: true, resolver: 'cloudflare' },
      per_host_rate: 1,
      proxy: null,
      tor: { enabled: false, mode: 'embedded', socks_url: 'socks5h://127.0.0.1:9050' },
    },
    magnets: { append_default_trackers: false, trackers_url: null },
    downloads: { torrent_dir: null },
    ui: { first_run_done: true, theme: 'system' },
    updates: { check_for_updates: true },
    torrents: {
      alt_limits: { down_kib: 1024, up_kib: 256 },
      ask_on_add: false,
      blocklist_url: null,
      close_to_tray: true,
      dht: true,
      download_dir: null,
      enabled: true,
      keep_awake: 'downloading',
      limits: { down_kib: null, up_kib: null },
      listen_port: 0,
      lsd: true,
      max_active_downloads: 3,
      max_active_seeds: 5,
      notice_accepted: true,
      on_battery: 'alt_speed',
      peer_limit_global: 1000,
      peer_limit_torrent: 200,
      proxy: null,
      schedule: null,
      seed_minutes: null,
      seed_ratio: 1,
      turtle: false,
      unmetered_only: true,
      upnp: true,
      utp: true,
      watch_dir: null,
    },
  } as Settings;
}

export function result(id: string, title: string, infoHash?: string): MergedResult {
  return {
    id,
    score: 1,
    sources: ['prov'],
    primary: { title, provider_id: 'prov', info_hash: infoHash ?? null },
  } as unknown as MergedResult;
}
