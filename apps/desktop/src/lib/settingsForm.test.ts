// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { normalizeTorrents, withTextFields, type TextFields } from './settingsForm';
import { defaultSettings } from './testFixtures';

const noText: TextFields = { proxy: '', trackersUrl: '', customDoh: '', torUrl: 'socks5h://127.0.0.1:9050' };

/** What the page sends: the draft with the text inputs merged in. */
const save = (draft: ReturnType<typeof defaultSettings>, text = noText) => withTextFields(draft, text);

describe('withTextFields', () => {
  it('leaves untouched settings as they were (no false "unsaved changes")', () => {
    const s = defaultSettings();
    expect(JSON.stringify(save(s))).toBe(JSON.stringify(s));
  });

  it('keeps the page-level text inputs', () => {
    const s = save(defaultSettings(), {
      proxy: ' socks5h://127.0.0.1:9050 ',
      trackersUrl: 'https://example.org/t.txt',
      customDoh: '',
      torUrl: '',
    });
    expect(s.network.proxy).toBe('socks5h://127.0.0.1:9050');
    expect(s.magnets.trackers_url).toBe('https://example.org/t.txt');
    expect(s.network.doh.custom_url).toBeNull();
    expect(s.network.tor.socks_url).toBeNull();
  });

  it('saves every text input of the Torrent client panel', () => {
    const draft = defaultSettings();
    draft.torrents.download_dir = ' D:\\Torrents ';
    draft.torrents.proxy = 'socks5://127.0.0.1:1080';
    draft.torrents.blocklist_url = 'https://example.org/list.gz';
    draft.torrents.watch_dir = '/srv/watch';
    const t = save(draft).torrents;
    expect(t.download_dir).toBe('D:\\Torrents');
    expect(t.proxy).toBe('socks5://127.0.0.1:1080');
    expect(t.blocklist_url).toBe('https://example.org/list.gz');
    expect(t.watch_dir).toBe('/srv/watch');
  });

  it('turns emptied text inputs into null', () => {
    const draft = defaultSettings();
    draft.torrents.download_dir = '';
    draft.torrents.proxy = '   ';
    draft.torrents.blocklist_url = '';
    draft.torrents.watch_dir = '';
    const t = save(draft).torrents;
    expect([t.download_dir, t.proxy, t.blocklist_url, t.watch_dir]).toEqual([null, null, null, null]);
  });
});

describe('numeric fields', () => {
  it('an emptied limit, seed ratio or seed time means none', () => {
    const t = defaultSettings().torrents;
    // Number inputs give null (or undefined/NaN) when cleared.
    t.limits.down_kib = undefined as unknown as null;
    t.alt_limits.up_kib = Number.NaN;
    t.seed_ratio = null;
    t.seed_minutes = undefined as unknown as null;
    const n = normalizeTorrents(t);
    expect(n.limits).toEqual({ down_kib: null, up_kib: null });
    expect(n.alt_limits).toEqual({ down_kib: 1024, up_kib: null });
    expect(n.seed_ratio).toBeNull();
    expect(n.seed_minutes).toBeNull();
    expect(JSON.parse(JSON.stringify(n)).seed_ratio).toBeNull();
  });

  it('keeps a ratio of 0 and numbers as typed', () => {
    const t = defaultSettings().torrents;
    t.seed_ratio = 0;
    t.limits.up_kib = 0;
    const n = normalizeTorrents(t);
    expect(n.seed_ratio).toBe(0);
    expect(n.limits.up_kib).toBe(0);
  });

  it('an empty schedule string is no schedule', () => {
    const t = defaultSettings().torrents;
    t.schedule = '';
    expect(normalizeTorrents(t).schedule).toBeNull();
    t.schedule = 'n'.repeat(168);
    expect(normalizeTorrents(t).schedule).toBe('n'.repeat(168));
  });
});

describe('save and reload', () => {
  it('peer limits and power settings survive a round trip', () => {
    const draft = defaultSettings();
    draft.torrents.peer_limit_global = 2500;
    draft.torrents.peer_limit_torrent = 300;
    draft.torrents.keep_awake = 'active';
    draft.torrents.on_battery = 'pause';
    draft.torrents.close_to_tray = false;
    draft.torrents.schedule = 'o'.repeat(24) + 'n'.repeat(144);

    // What the server stores and returns on the next load.
    const sent = JSON.stringify(save(draft));
    const reloaded = JSON.parse(sent) as ReturnType<typeof defaultSettings>;
    expect(reloaded.torrents).toMatchObject({
      peer_limit_global: 2500,
      peer_limit_torrent: 300,
      keep_awake: 'active',
      on_battery: 'pause',
      close_to_tray: false,
    });
    expect(reloaded.torrents.schedule).toHaveLength(168);

    // Reopening the page: the draft is the saved copy, so nothing is "dirty".
    expect(JSON.stringify(save(reloaded))).toBe(JSON.stringify(reloaded));
    // Changing one value makes the page dirty again.
    reloaded.torrents.peer_limit_global = 2501;
    expect(JSON.stringify(save(reloaded))).not.toBe(sent);
  });
});
