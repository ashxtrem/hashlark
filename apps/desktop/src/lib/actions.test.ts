// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Api } from '$lib/api/client';
import { defaultSettings, result } from './testFixtures';

const { goto, openMagnet, openTorrent } = vi.hoisted(() => ({
  goto: vi.fn(),
  openMagnet: vi.fn(),
  openTorrent: vi.fn(),
}));

vi.mock('$app/navigation', () => ({ goto }));
vi.mock('$lib/platform', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/platform')>()),
  isTauri: () => true,
  openMagnet,
  openTorrent,
}));

import { download, inDownloads, openInClient } from './actions';
import { app } from './state/app.svelte';
import { toasts } from './state/toasts.svelte';
import { torrents } from './state/torrents.svelte';
import { ui } from './state/ui.svelte';

const MAGNET = 'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567';
const HASH = '0123456789abcdef0123456789abcdef01234567';
const r = result('r1', 'Ubuntu ISO', HASH);

function setEnabled(enabled: boolean) {
  const s = defaultSettings();
  s.torrents.enabled = enabled;
  app.settings = s;
}

let add: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  setEnabled(true);
  app.api = { resolve: vi.fn(async () => ({ type: 'magnet', url: MAGNET })) } as unknown as Api;
  openMagnet.mockResolvedValue('opened');
  add = vi.spyOn(torrents, 'add').mockResolvedValue({
    already: false,
    torrent: { id: HASH, name: 'Ubuntu ISO' },
  } as never);
  toasts.items = [];
  ui.noHandlerMagnet = null;
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.clearAllMocks();
  torrents.list = [];
});

describe('download', () => {
  it('with the built-in client on, adds the result and offers Show', async () => {
    await download(r);
    expect(add).toHaveBeenCalledTimes(1);
    expect(add.mock.calls[0]![0]).toEqual({ source: { result_id: 'r1' } });
    expect((add.mock.calls[0]![1] as { dialog?: boolean }).dialog).toBeFalsy();
    expect(openMagnet).not.toHaveBeenCalled();

    const [toast] = toasts.items;
    expect(toast?.message).toBe('Added to Downloads');
    expect(toast?.action?.label).toBe('Show');
    toast!.action!.run();
    expect(goto).toHaveBeenCalledWith('/downloads');
  });

  it('with Shift held, asks first through the Add dialog', async () => {
    await download(r, { dialog: true });
    expect((add.mock.calls[0]![1] as { dialog?: boolean }).dialog).toBe(true);
  });

  it('says nothing when the torrent was already there or the add was cancelled', async () => {
    add.mockResolvedValueOnce({ already: true, torrent: { id: HASH } } as never);
    await download(r);
    add.mockResolvedValueOnce(null as never);
    await download(r, { dialog: true });
    expect(toasts.items).toHaveLength(0);
  });

  it('with the built-in client off, hands the magnet to the other client', async () => {
    setEnabled(false);
    await download(r);
    expect(add).not.toHaveBeenCalled();
    expect(openMagnet).toHaveBeenCalledWith(MAGNET);
    expect(toasts.items[0]?.message).toBe('Sent to your torrent client');
  });

  it('with it off, Shift does not open the Add dialog', async () => {
    setEnabled(false);
    await download(r, { dialog: true });
    expect(add).not.toHaveBeenCalled();
    expect(openMagnet).toHaveBeenCalledTimes(1);
  });

  it('with it off and no other client, remembers the magnet for the dialog', async () => {
    setEnabled(false);
    openMagnet.mockResolvedValue('no_handler');
    await download(r);
    expect(ui.noHandlerMagnet).toBe(MAGNET);
  });
});

describe('openInClient', () => {
  it('hands off even while the built-in client is on', async () => {
    await openInClient(r);
    expect(add).not.toHaveBeenCalled();
    expect(openMagnet).toHaveBeenCalledWith(MAGNET);
  });
});

describe('inDownloads', () => {
  it('matches the result info hash against the client', () => {
    expect(inDownloads(r)).toBe(false);
    torrents.list = [{ id: HASH }] as never;
    expect(inDownloads(r)).toBe(true);
    expect(inDownloads(result('r2', 'No hash'))).toBe(false);
    expect(inDownloads(result('r3', 'Upper', HASH.toUpperCase()))).toBe(true);
  });
});
