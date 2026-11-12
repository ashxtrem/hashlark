// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { SessionStats, TorrentSummary } from './api/client';
import {
  countdownText,
  displayPath,
  eta,
  fileTree,
  filterCounts,
  moveOrder,
  nodeCheck,
  patch,
  pauseToggle,
  percent,
  queueTarget,
  ratio,
  readTorrent,
  reduce,
  removeRequest,
  maintainRequest,
  canReannounce,
  canRecheck,
  speed,
  statusLabel,
  stepSelection,
  stopToggle,
  toBase64,
  torrentFileError,
  MAX_TORRENT_FILE,
  visibleTorrents,
  backoff,
  rangeIds,
} from './torrents';

function torrent(id: string, over: Partial<TorrentSummary> = {}): TorrentSummary {
  return {
    id,
    name: `Torrent ${id}`,
    added_at: 0,
    done: false,
    down_bps: 0,
    downloaded_total: 0,
    have: 0,
    peers: 0,
    progress: 0,
    queue_pos: 0,
    ratio: 0,
    run_mode: 'auto',
    save_dir: '/dl',
    seeds: 0,
    size: 100,
    source: 'magnet',
    state: 'downloading',
    up_bps: 0,
    uploaded_total: 0,
    ...over,
  };
}

const session = { running: true, turtle: false } as SessionStats;

describe('formatters', () => {
  it('formats speeds, with a dash when idle', () => {
    expect(speed(0)).toBe('–');
    expect(speed(null)).toBe('–');
    expect(speed(512)).toBe('512 B/s');
    expect(speed(1536)).toBe('1.5 KiB/s');
    expect(speed(5 * 1024 * 1024)).toBe('5.0 MiB/s');
  });

  it('formats ETAs', () => {
    expect(eta(null)).toBe('–');
    expect(eta(45)).toBe('45s');
    expect(eta(200)).toBe('3m 20s');
    expect(eta(2 * 3600 + 5 * 60 + 9)).toBe('2h 5m');
    expect(eta(3 * 86_400 + 4 * 3600)).toBe('3d 4h');
    expect(eta(200 * 86_400)).toBe('∞');
  });

  it('formats ratios', () => {
    expect(ratio(0)).toBe('0.00');
    expect(ratio(1.234)).toBe('1.23');
    expect(ratio(Infinity)).toBe('∞');
    expect(ratio(NaN)).toBe('–');
  });

  it('only shows 100% when complete', () => {
    expect(percent(0.9999)).toBe('99.9%');
    expect(percent(1)).toBe('100.0%');
    expect(percent(0.4567)).toBe('45.6%');
  });

  it('labels why a torrent is not moving', () => {
    expect(statusLabel({ state: 'scheduled_off', run_mode: 'auto' })).toBe('Scheduled: off');
    expect(statusLabel({ state: 'waiting_for_network', run_mode: 'auto' })).toBe('Waiting for Wi-Fi');
    expect(statusLabel({ state: 'done', run_mode: 'auto' })).toBe('Done (seed limit)');
    expect(statusLabel({ state: 'downloading', run_mode: 'forced' })).toBe('Downloading (forced)');
  });

  it('words the when-done countdown', () => {
    expect(countdownText('shutdown', 41.2)).toBe('Shutting down in 42 s');
    expect(countdownText('quit', -1)).toBe('Quitting Hashlark in 0 s');
  });
});

describe('filterCounts', () => {
  const list = [
    torrent('a', { state: 'downloading', progress: 0.5 }),
    torrent('b', { state: 'metadata', size: 0 }),
    torrent('c', { state: 'seeding', progress: 1 }),
    torrent('d', { state: 'done', progress: 1, done: true }),
    torrent('e', { state: 'finished', progress: 1, run_mode: 'stopped' }),
    torrent('f', { state: 'stopped', progress: 0.3, run_mode: 'stopped' }),
    torrent('g', { state: 'queued' }),
    torrent('h', { state: 'scheduled_off' }),
    torrent('i', { state: 'paused', run_mode: 'paused' }),
    torrent('j', { state: 'error', error: 'boom' }),
  ];

  it('counts every tab', () => {
    expect(filterCounts(list)).toEqual({
      all: 10,
      downloading: 2,
      seeding: 1,
      queued: 2,
      paused: 1,
      stopped: 2,
      finished: 3,
      error: 1,
    });
  });

  it('puts stopped and done finished torrents under Finished', () => {
    const finished = visibleTorrents(list, 'finished', '').map((t) => t.id);
    expect(finished).toEqual(['c', 'd', 'e']);
  });

  it('counts a stopped torrent at 100% as finished', () => {
    expect(filterCounts([torrent('x', { state: 'stopped', run_mode: 'stopped', progress: 1 })]).finished).toBe(1);
  });

  it('filters by every word of the name, in queue order', () => {
    const named = [
      torrent('1', { name: 'Ubuntu 24.04 Desktop', queue_pos: 2 }),
      torrent('2', { name: 'Debian netinst', queue_pos: 0 }),
      torrent('3', { name: 'ubuntu server', queue_pos: 1 }),
    ];
    expect(visibleTorrents(named, 'all', 'UBUNTU').map((t) => t.id)).toEqual(['3', '1']);
    expect(visibleTorrents(named, 'all', 'ubuntu desk').map((t) => t.id)).toEqual(['1']);
  });
});

describe('reduce', () => {
  it('replaces the list and session on a snapshot', () => {
    const list = [torrent('a')];
    const { data, notice } = reduce({ list: [], session: null }, { type: 'snapshot', session, torrents: list });
    expect(data).toEqual({ list, session });
    expect(notice).toBeNull();
  });

  it('keeps the data and announces completion', () => {
    const before = { list: [torrent('a')], session };
    const { data, notice } = reduce(before, { type: 'completed', id: 'a', name: 'ISO' });
    expect(data).toBe(before);
    expect(notice).toEqual({ kind: 'success', message: 'Download finished: ISO' });
  });

  it('names the torrent of an error', () => {
    const { notice } = reduce({ list: [torrent('a')], session }, { type: 'error', id: 'a', message: 'disk full' });
    expect(notice).toEqual({ kind: 'error', message: 'Torrent a: disk full' });
  });

  it('backs off up to 30 s', () => {
    expect([0, 1, 2, 10].map(backoff)).toEqual([1000, 2000, 4000, 30_000]);
  });
});

describe('actions', () => {
  it('sends delete_files only when ticked', () => {
    expect(removeRequest(['a'], false)).toEqual({ ids: ['a'], action: { remove: {} } });
    expect(removeRequest(['a', 'b'], true)).toEqual({ ids: ['a', 'b'], action: { remove: { delete_files: true } } });
  });

  it('toggles pause and stop for a selection', () => {
    expect(pauseToggle([{ run_mode: 'paused' }, { run_mode: 'paused' }])).toBe('resume');
    expect(pauseToggle([{ run_mode: 'paused' }, { run_mode: 'auto' }])).toBe('pause');
    expect(stopToggle([{ run_mode: 'stopped' }])).toBe('start');
    expect(stopToggle([{ run_mode: 'forced' }])).toBe('stop');
  });

  it('fills a patch with nulls', () => {
    expect(patch({ queue_pos: 3 })).toEqual({
      action: null,
      files: null,
      limits: null,
      peer_limit: null,
      queue_pos: 3,
      seed: null,
      trackers: null,
    });
  });

  it('sends a tracker list in a patch', () => {
    expect(patch({ trackers: ['udp://a.example:1337/announce'] }).trackers).toEqual(['udp://a.example:1337/announce']);
    expect(patch({ trackers: [] }).trackers).toEqual([]);
  });

  it('builds reannounce and recheck bulk requests', () => {
    expect(maintainRequest(['a', 'b'], 'recheck')).toEqual({ ids: ['a', 'b'], action: 'recheck' });
    expect(maintainRequest(['a'], 'reannounce')).toEqual({ ids: ['a'], action: 'reannounce' });
  });

  it('enables reannounce and recheck by state', () => {
    const t = (state: TorrentSummary['state']) => ({ state });
    expect(canRecheck([t('metadata'), t('metadata')])).toBe(false);
    expect(canRecheck([t('metadata'), t('paused')])).toBe(true);
    expect(canRecheck([])).toBe(false);
    expect(canReannounce([t('paused'), t('queued')])).toBe(false);
    expect(canReannounce([t('paused'), t('seeding')])).toBe(true);
    expect(canReannounce([t('downloading')])).toBe(true);
    expect(canReannounce([t('metadata')])).toBe(false);
  });
});

describe('queue', () => {
  const all = [
    torrent('s', { queue_pos: 0, state: 'seeding' }),
    torrent('q1', { queue_pos: 1, state: 'queued' }),
    torrent('d', { queue_pos: 2, state: 'downloading' }),
    torrent('q2', { queue_pos: 3, state: 'queued' }),
    torrent('q3', { queue_pos: 4, state: 'queued' }),
  ];
  const queued = visibleTorrents(all, 'queued', '');

  it('moves past the shown neighbour, skipping hidden rows', () => {
    expect(queueTarget(all, queued, 'q2', 'up')).toBe(1);
    expect(queueTarget(all, queued, 'q1', 'down')).toBe(3);
    expect(queueTarget(all, queued, 'q3', 'top')).toBe(1);
    expect(queueTarget(all, queued, 'q1', 'bottom')).toBe(4);
  });

  it("can't move past the ends", () => {
    expect(queueTarget(all, queued, 'q1', 'up')).toBeNull();
    expect(queueTarget(all, queued, 'q3', 'bottom')).toBeNull();
  });

  it('keeps a multi-selection in order', () => {
    expect(moveOrder(all, ['q2', 'q1'], 'top')).toEqual(['q2', 'q1']);
    expect(moveOrder(all, ['q2', 'q1'], 'up')).toEqual(['q1', 'q2']);
  });
});

describe('selection', () => {
  const ids = ['a', 'b', 'c'];
  it('steps and clamps', () => {
    expect(stepSelection(ids, null, 1)).toBe('a');
    expect(stepSelection(ids, null, -1)).toBe('c');
    expect(stepSelection(ids, 'b', 1)).toBe('c');
    expect(stepSelection(ids, 'c', 1)).toBe('c');
    expect(stepSelection([], null, 1)).toBeNull();
  });

  it('selects ranges either way', () => {
    expect(rangeIds(ids, 'c', 'a')).toEqual(['a', 'b', 'c']);
    expect(rangeIds(ids, null, 'b')).toEqual(['b']);
  });
});

describe('files', () => {
  it('builds a tree with folder totals', () => {
    const tree = fileTree([
      { index: 0, path: 'b.txt', size: 10, have: 5 },
      { index: 1, path: 'dir/a.bin', size: 30, have: 30 },
      { index: 2, path: 'dir/sub/c.bin', size: 60, have: 0 },
    ]);
    expect(tree.map((n) => n.name)).toEqual(['dir', 'b.txt']);
    expect(tree[0]).toMatchObject({ size: 90, have: 30 });
    expect(nodeCheck(tree[0]!, new Set([1]))).toBe('some');
    expect(nodeCheck(tree[0]!, new Set([1, 2]))).toBe('all');
    expect(nodeCheck(tree[0]!, new Set([0]))).toBe('none');
  });

  const enc = new TextEncoder();
  const bstr = (s: string) => `${enc.encode(s).length}:${s}`;

  it('reads a multi-file .torrent, keeping indices past padding files', () => {
    const text =
      'd4:infod5:filesl' +
      `d6:lengthi10e4:pathl${bstr('a.txt')}ee` +
      `d4:attr1:p6:lengthi6e4:pathl${bstr('.pad')}${bstr('6')}ee` +
      `d6:lengthi20e4:pathl${bstr('sub')}${bstr('b.txt')}ee` +
      `e4:name${bstr('Pack')}12:piece lengthi16384eee`;
    expect(readTorrent(enc.encode(text))).toEqual({
      name: 'Pack',
      files: [
        { index: 0, path: 'a.txt', size: 10 },
        { index: 2, path: 'sub/b.txt', size: 20 },
      ],
    });
  });

  it('reads a single-file .torrent and rejects junk', () => {
    const text = `d4:infod6:lengthi42e4:name${bstr('one.iso')}ee`;
    expect(readTorrent(enc.encode(text))).toEqual({ name: 'one.iso', files: [{ index: 0, path: 'one.iso', size: 42 }] });
    expect(readTorrent(enc.encode('not bencode'))).toBeNull();
    expect(readTorrent(enc.encode('d4:infoi1ee'))).toBeNull();
  });

  it('drops the Windows long-path prefix for display', () => {
    expect(displayPath('\\\\?\\C:\\Users\\me\\Downloads')).toBe('C:\\Users\\me\\Downloads');
    expect(displayPath('\\\\?\\UNC\\nas\\share')).toBe('\\\\nas\\share');
    expect(displayPath('/data/downloads')).toBe('/data/downloads');
  });

  it('base64-encodes bytes', () => {
    expect(toBase64(new Uint8Array([0, 255, 104, 105]))).toBe('AP9oaQ==');
  });

  it('refuses non-.torrent and over-10 MiB files before reading them', () => {
    expect(torrentFileError({ name: 'a.TORRENT', size: 1000 })).toBeNull();
    expect(torrentFileError({ name: 'a.torrent', size: MAX_TORRENT_FILE })).toBeNull();
    expect(torrentFileError({ name: 'a.torrent', size: MAX_TORRENT_FILE + 1 })).toMatch(/too big/);
    expect(torrentFileError({ name: 'a.txt', size: 10 })).toMatch(/not a \.torrent/);
    expect(MAX_TORRENT_FILE).toBe(10 * 1024 * 1024);
  });
});
