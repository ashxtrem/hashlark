// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure helpers of the Downloads page (plan `docs/torrent-client-plan.md`
//! §7.2): wording, filters, the live-update reducer, queue moves and
//! reading `.torrent` files. Kept apart from the state so they can be tested.

import type {
  BulkRequest,
  Schemas,
  SessionStats,
  TorrentEvent,
  TorrentFile,
  TorrentPatch,
  TorrentSummary,
} from '$lib/api/client';
import { bytes } from '$lib/format';
import type { WhenDoneAction } from '$lib/platform';

export type TorrentState = Schemas['TorrentState'];
export type QueueStep = Schemas['QueueStep'];

// ----- Formatters ------------------------------------------------------------

/** `1536` → `1.5 KiB/s`; nothing moving shows as `–`. */
export function speed(bps: number | null | undefined): string {
  if (!bps || bps < 0) return '–';
  return `${bytes(bps)}/s`;
}

/** Like {@link speed} but `0 B/s` when idle (the sidebar totals). */
export function totalSpeed(bps: number | null | undefined): string {
  return bps && bps > 0 ? `${bytes(bps)}/s` : '0 B/s';
}

const MINUTE = 60;
const HOUR = 3600;
const DAY = 86_400;

/** Seconds left: `45s`, `3m 20s`, `2h 5m`, `3d 4h`; unknown is `–`, over 100 days `∞`. */
export function eta(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs) || secs < 0) return '–';
  const s = Math.round(secs);
  if (s < MINUTE) return `${s}s`;
  if (s < HOUR) return `${Math.floor(s / MINUTE)}m ${s % MINUTE}s`;
  if (s < DAY) return `${Math.floor(s / HOUR)}h ${Math.floor((s % HOUR) / MINUTE)}m`;
  if (s < 100 * DAY) return `${Math.floor(s / DAY)}d ${Math.floor((s % DAY) / HOUR)}h`;
  return '∞';
}

/** Share ratio with two decimals; `∞` when it can't be computed. */
export function ratio(r: number | null | undefined): string {
  if (r == null || Number.isNaN(r)) return '–';
  if (!Number.isFinite(r)) return '∞';
  return r.toFixed(2);
}

/** `0.4567` → `45.6%`. Rounds down so only a complete torrent shows 100%. */
export function percent(progress: number): string {
  const p = Math.max(0, Math.min(1, progress || 0));
  return `${(Math.floor(p * 1000) / 10).toFixed(1)}%`;
}

// ----- Status ----------------------------------------------------------------

export const STATE_LABELS: Record<TorrentState, string> = {
  metadata: 'Fetching metadata',
  checking: 'Checking',
  downloading: 'Downloading',
  seeding: 'Seeding',
  queued: 'Queued',
  paused: 'Paused',
  stopped: 'Stopped',
  finished: 'Finished',
  done: 'Done (seed limit)',
  scheduled_off: 'Scheduled: off',
  scheduled_seed_only: 'Scheduled: seed only',
  on_battery: 'On battery',
  waiting_for_network: 'Waiting for Wi-Fi',
  error: 'Error',
};

/** The status column: why a torrent is (not) moving. */
export function statusLabel(t: Pick<TorrentSummary, 'state' | 'run_mode'>): string {
  const label = STATE_LABELS[t.state];
  const running = t.state === 'downloading' || t.state === 'seeding' || t.state === 'metadata';
  return t.run_mode === 'forced' && running ? `${label} (forced)` : label;
}

export type Tone = 'accent' | 'ok' | 'warn' | 'err' | 'muted';

export function stateTone(state: TorrentState): Tone {
  switch (state) {
    case 'downloading':
    case 'metadata':
    case 'checking':
      return 'accent';
    case 'seeding':
    case 'finished':
    case 'done':
      return 'ok';
    case 'scheduled_off':
    case 'scheduled_seed_only':
    case 'on_battery':
    case 'waiting_for_network':
      return 'warn';
    case 'error':
      return 'err';
    default:
      return 'muted';
  }
}

/** "When all done" choices (desktop). */
export const WHEN_DONE_LABELS: Record<WhenDoneAction, string> = {
  nothing: 'Do nothing',
  quit: 'Quit Hashlark',
  sleep: 'Sleep',
  hibernate: 'Hibernate',
  shutdown: 'Shut down',
};

/** The countdown sentence: "Shutting down in 42 s". */
export function countdownText(action: WhenDoneAction, seconds: number): string {
  const s = Math.max(0, Math.ceil(seconds));
  const verb: Record<WhenDoneAction, string> = {
    nothing: 'Finishing',
    quit: 'Quitting Hashlark',
    sleep: 'Going to sleep',
    hibernate: 'Hibernating',
    shutdown: 'Shutting down',
  };
  return `${verb[action]} in ${s} s`;
}

// ----- Filters ---------------------------------------------------------------

export const FILTERS = ['all', 'downloading', 'seeding', 'queued', 'paused', 'stopped', 'finished', 'error'] as const;
export type Filter = (typeof FILTERS)[number];

export const FILTER_LABELS: Record<Filter, string> = {
  all: 'All',
  downloading: 'Downloading',
  seeding: 'Seeding',
  queued: 'Queued',
  paused: 'Paused',
  stopped: 'Stopped',
  finished: 'Finished',
  error: 'Error',
};

type Row = Pick<TorrentSummary, 'state' | 'run_mode' | 'progress'>;

/** 100% of the selected files, whether seeding, done (seed limit) or stopped. */
export function isComplete(t: Row): boolean {
  if (t.state === 'seeding' || t.state === 'finished' || t.state === 'done') return true;
  if (t.state === 'metadata' || t.state === 'error') return false;
  return t.progress >= 1;
}

/**
 * Whether a torrent shows under a filter tab. The buckets follow the
 * server's `StateCounts` (held-back states count as queued), except that
 * the tabs overlap: Finished also holds seeding and stopped torrents that
 * are complete, and Stopped holds stopped ones that are complete.
 */
export function matchesFilter(t: Row, filter: Filter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'downloading':
      return t.state === 'metadata' || t.state === 'checking' || t.state === 'downloading';
    case 'seeding':
      return t.state === 'seeding';
    case 'queued':
      return (
        t.state === 'queued' ||
        t.state === 'scheduled_off' ||
        t.state === 'scheduled_seed_only' ||
        t.state === 'on_battery' ||
        t.state === 'waiting_for_network'
      );
    case 'paused':
      return t.state === 'paused';
    case 'stopped':
      return t.state === 'stopped' || t.state === 'finished' || (t.run_mode === 'stopped' && t.state !== 'error');
    case 'finished':
      return isComplete(t);
    case 'error':
      return t.state === 'error';
  }
}

/** Number of torrents under each tab. */
export function filterCounts(list: Row[]): Record<Filter, number> {
  const counts = Object.fromEntries(FILTERS.map((f) => [f, 0])) as Record<Filter, number>;
  for (const t of list) {
    for (const f of FILTERS) if (matchesFilter(t, f)) counts[f] += 1;
  }
  return counts;
}

/** Queue order: `queue_pos`, then the time added (as the server sorts). */
export function byQueue<T extends Pick<TorrentSummary, 'queue_pos' | 'added_at'>>(list: T[]): T[] {
  return [...list].sort((a, b) => a.queue_pos - b.queue_pos || a.added_at - b.added_at);
}

/** The rows a tab shows, in queue order, narrowed by a name filter. */
export function visibleTorrents(list: TorrentSummary[], filter: Filter, name: string): TorrentSummary[] {
  const words = name.toLowerCase().split(/\s+/).filter(Boolean);
  return byQueue(list).filter(
    (t) => matchesFilter(t, filter) && words.every((w) => t.name.toLowerCase().includes(w)),
  );
}

/** Torrents fetching or downloading (the sidebar badge). */
export function downloadingCount(list: Pick<TorrentSummary, 'state'>[]): number {
  return list.filter((t) => t.state === 'metadata' || t.state === 'checking' || t.state === 'downloading').length;
}

// ----- Live updates ----------------------------------------------------------

export interface TorrentsData {
  list: TorrentSummary[];
  session: SessionStats | null;
}

export interface TorrentNotice {
  kind: 'success' | 'error';
  message: string;
}

/** Applies one event of `GET /torrents/events`. */
export function reduce(data: TorrentsData, event: TorrentEvent): { data: TorrentsData; notice: TorrentNotice | null } {
  switch (event.type) {
    case 'snapshot':
      return { data: { list: event.torrents, session: event.session }, notice: null };
    case 'completed':
      return { data, notice: { kind: 'success', message: `Download finished: ${event.name}` } };
    case 'error': {
      const name = data.list.find((t) => t.id === event.id)?.name ?? event.id;
      return { data, notice: { kind: 'error', message: `${name}: ${event.message}` } };
    }
  }
}

/** Reconnect delay after `attempt` failures in a row: 1 s doubling up to 30 s. */
export function backoff(attempt: number): number {
  return Math.min(30_000, 1000 * 2 ** Math.max(0, attempt));
}

// ----- Actions ---------------------------------------------------------------

type Mode = Pick<TorrentSummary, 'run_mode'>;

/** Space: resume when every selected torrent is paused, else pause. */
export function pauseToggle(list: Mode[]): 'pause' | 'resume' {
  return list.length > 0 && list.every((t) => t.run_mode === 'paused') ? 'resume' : 'pause';
}

/** Shift+S: start when every selected torrent is stopped, else stop. */
export function stopToggle(list: Mode[]): 'stop' | 'start' {
  return list.length > 0 && list.every((t) => t.run_mode === 'stopped') ? 'start' : 'stop';
}

/** The bulk request of the delete dialog: `delete_files` only when ticked. */
export function removeRequest(ids: string[], deleteFiles: boolean): BulkRequest {
  return { ids, action: { remove: deleteFiles ? { delete_files: true } : {} } };
}

/** A `TorrentPatch` that only sets the given fields. */
export function patch(fields: Partial<TorrentPatch>): TorrentPatch {
  return {
    action: null,
    files: null,
    limits: null,
    peer_limit: null,
    queue_pos: null,
    seed: null,
    ...fields,
  };
}

// ----- Queue -----------------------------------------------------------------

/**
 * The `queue_pos` index that moves `id` by `step` among the `shown` rows
 * (so in the Queued tab "up" passes the previous queued torrent, not a
 * hidden seeding one). The server takes the torrent out and puts it back at
 * that index of the whole queue. `null` when it can't move.
 */
export function queueTarget(
  all: TorrentSummary[],
  shown: TorrentSummary[],
  id: string,
  step: QueueStep,
): number | null {
  const rows = byQueue(shown);
  const i = rows.findIndex((t) => t.id === id);
  if (i < 0) return null;
  const j = step === 'top' ? 0 : step === 'up' ? i - 1 : step === 'down' ? i + 1 : rows.length - 1;
  const target = rows[j];
  if (!target || j === i) return null;
  return dropIndex(all, target.id);
}

/** The `queue_pos` index that puts a dragged torrent in `targetId`'s place. */
export function dropIndex(all: TorrentSummary[], targetId: string): number | null {
  const i = byQueue(all).findIndex((t) => t.id === targetId);
  return i < 0 ? null : i;
}

/**
 * Order to send one queue step per torrent so a multi-selection keeps its
 * own order: top and down go from the last, up and bottom from the first.
 */
export function moveOrder(all: TorrentSummary[], ids: string[], step: QueueStep): string[] {
  const set = new Set(ids);
  const ordered = byQueue(all)
    .filter((t) => set.has(t.id))
    .map((t) => t.id);
  return step === 'top' || step === 'down' ? ordered.reverse() : ordered;
}

// ----- Selection -------------------------------------------------------------

/** The id `delta` rows away from `current` (the first row when none). */
export function stepSelection(ids: string[], current: string | null, delta: number): string | null {
  if (ids.length === 0) return null;
  const i = current ? ids.indexOf(current) : -1;
  if (i < 0) return (delta < 0 ? ids[ids.length - 1] : ids[0]) ?? null;
  return ids[Math.max(0, Math.min(ids.length - 1, i + delta))] ?? null;
}

/** Ids from `anchor` to `id` inclusive, in list order (Shift+click). */
export function rangeIds(ids: string[], anchor: string | null, id: string): string[] {
  const a = anchor ? ids.indexOf(anchor) : -1;
  const b = ids.indexOf(id);
  if (a < 0 || b < 0) return [id];
  return ids.slice(Math.min(a, b), Math.max(a, b) + 1);
}

// ----- Files -----------------------------------------------------------------

export interface FileEntry {
  index: number;
  path: string;
  size: number;
  have?: number;
  selected?: boolean;
}

export interface FileNode {
  name: string;
  /** Path from the torrent root, `/`-separated. */
  path: string;
  /** Set for files, absent for folders. */
  file?: FileEntry;
  children: FileNode[];
  size: number;
  have: number;
}

/** Folders and files of a torrent, folders first, by name. */
export function fileTree(files: FileEntry[]): FileNode[] {
  const root: FileNode = { name: '', path: '', children: [], size: 0, have: 0 };
  for (const f of files) {
    const parts = f.path.split('/').filter(Boolean);
    let node = root;
    for (let i = 0; i < parts.length - 1; i += 1) {
      const part = parts[i] ?? '';
      const path = parts.slice(0, i + 1).join('/');
      let child = node.children.find((c) => !c.file && c.name === part);
      if (!child) {
        child = { name: part, path, children: [], size: 0, have: 0 };
        node.children.push(child);
      }
      node = child;
    }
    node.children.push({
      name: parts[parts.length - 1] ?? f.path,
      path: parts.join('/'),
      file: f,
      children: [],
      size: f.size,
      have: f.have ?? 0,
    });
  }
  const finish = (n: FileNode): void => {
    for (const c of n.children) finish(c);
    if (!n.file) {
      n.size = n.children.reduce((s, c) => s + c.size, 0);
      n.have = n.children.reduce((s, c) => s + c.have, 0);
    }
    n.children.sort((a, b) => Number(Boolean(a.file)) - Number(Boolean(b.file)) || a.name.localeCompare(b.name));
  };
  finish(root);
  return root.children;
}

/** File indices under a node. */
export function nodeIndices(node: FileNode): number[] {
  return node.file ? [node.file.index] : node.children.flatMap(nodeIndices);
}

/** `all`, `none` or `some` of a node's files are in `selected`. */
export function nodeCheck(node: FileNode, selected: Set<number>): 'all' | 'none' | 'some' {
  const indices = nodeIndices(node);
  const n = indices.filter((i) => selected.has(i)).length;
  return n === 0 ? 'none' : n === indices.length ? 'all' : 'some';
}

/** Selected indices of a details response. */
export function selectedIndices(files: TorrentFile[]): Set<number> {
  return new Set(files.filter((f) => f.selected).map((f) => f.index));
}

/** A folder as people write it: without Windows' `\\?\` long-path prefix. */
export function displayPath(path: string): string {
  if (path.startsWith('\\\\?\\UNC\\')) return `\\\\${path.slice(8)}`;
  return path.startsWith('\\\\?\\') ? path.slice(4) : path;
}

// ----- Adding ----------------------------------------------------------------

export function isMagnet(text: string): boolean {
  return /^magnet:\?/i.test(text.trim());
}

/** Base64 of a file's bytes, for `{ torrent }` sources. */
export function toBase64(data: Uint8Array): string {
  let binary = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < data.length; i += CHUNK) {
    binary += String.fromCharCode(...data.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

type Bencoded = number | Uint8Array | Bencoded[] | Map<string, Bencoded>;

/** Byte strings (not `instanceof`, which fails across realms). */
const isBytes = (v: unknown): v is Uint8Array => ArrayBuffer.isView(v);

function bdecode(data: Uint8Array): Bencoded {
  let pos = 0;
  const latin1 = (from: number, to: number) => String.fromCharCode(...data.subarray(from, to));
  const read = (depth: number): Bencoded => {
    if (depth > 64 || pos >= data.length) throw new Error('bad bencode');
    const c = data[pos];
    if (c === 0x69 /* i */) {
      const end = data.indexOf(0x65, pos);
      if (end < 0) throw new Error('bad bencode');
      const n = Number(latin1(pos + 1, end));
      pos = end + 1;
      return n;
    }
    if (c === 0x6c /* l */) {
      pos += 1;
      const list: Bencoded[] = [];
      while (data[pos] !== 0x65) list.push(read(depth + 1));
      pos += 1;
      return list;
    }
    if (c === 0x64 /* d */) {
      pos += 1;
      const dict = new Map<string, Bencoded>();
      while (data[pos] !== 0x65) {
        const key = read(depth + 1);
        if (!isBytes(key)) throw new Error('bad bencode');
        dict.set(new TextDecoder().decode(key), read(depth + 1));
      }
      pos += 1;
      return dict;
    }
    const colon = data.indexOf(0x3a, pos);
    if (colon < 0) throw new Error('bad bencode');
    const len = Number(latin1(pos, colon));
    if (!Number.isInteger(len) || len < 0 || colon + 1 + len > data.length) throw new Error('bad bencode');
    pos = colon + 1 + len;
    return data.subarray(colon + 1, pos);
  };
  return read(0);
}

const text = (v: Bencoded | undefined): string | null => (isBytes(v) ? new TextDecoder().decode(v) : null);

/**
 * Name and files of a `.torrent` (v1 layout), for the Add dialog before the
 * torrent is added. Indices count every file, as `AddRequest.files` does;
 * padding files are left out of the list. `null` when it can't be read.
 */
export function readTorrent(data: Uint8Array): { name: string; files: FileEntry[] } | null {
  try {
    const root = bdecode(data);
    if (!(root instanceof Map)) return null;
    const info = root.get('info');
    if (!(info instanceof Map)) return null;
    const name = text(info.get('name.utf-8')) ?? text(info.get('name')) ?? 'torrent';
    const list = info.get('files');
    if (list === undefined) {
      const length = info.get('length');
      return typeof length === 'number' ? { name, files: [{ index: 0, path: name, size: length }] } : null;
    }
    if (!Array.isArray(list)) return null;
    const files: FileEntry[] = [];
    list.forEach((entry, index) => {
      if (!(entry instanceof Map)) return;
      const attr = text(entry.get('attr')) ?? '';
      if (attr.includes('p')) return;
      const parts = entry.get('path.utf-8') ?? entry.get('path');
      const length = entry.get('length');
      if (!Array.isArray(parts) || typeof length !== 'number') return;
      files.push({ index, path: parts.map((p) => text(p) ?? '').join('/'), size: length });
    });
    return { name, files };
  } catch {
    return null;
  }
}
