// SPDX-License-Identifier: GPL-3.0-or-later

const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];

/** `1536` → `1.5 KiB`. */
export function bytes(n: number | null | undefined): string {
  if (n == null) return '–';
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${UNITS[unit]}`;
}

/** Seeder/leecher counts; unknown shows as `–`. */
export function count(n: number | null | undefined): string {
  return n == null ? '–' : n.toLocaleString();
}

const HOUR = 3600;
const DAY = 86_400;

/** Age since an RFC 3339 date: `5h`, `3d`, `4mo`, `2y`. */
export function age(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return '–';
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return '–';
  const secs = Math.max(0, Math.floor((now.getTime() - then.getTime()) / 1000));
  if (secs < DAY) return `${Math.floor(secs / HOUR)}h`;
  if (secs < 30 * DAY) return `${Math.floor(secs / DAY)}d`;
  if (secs < 365 * DAY) return `${Math.floor(secs / (30 * DAY))}mo`;
  return `${Math.floor(secs / (365 * DAY))}y`;
}

/** `1234` → `1.2 s`, `250` → `250 ms`. */
export function duration(ms: number | null | undefined): string {
  if (ms == null) return '–';
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

/** Absolute date for tooltips. */
export function dateTime(value: string | number | null | undefined): string {
  if (value == null) return '';
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleString();
}

export const CATEGORY_LABELS: Record<string, string> = {
  movies: 'Movies',
  tv: 'TV',
  music: 'Music',
  books: 'Books',
  software: 'Software',
  games: 'Games',
  anime: 'Anime',
  other: 'Other',
};

export const ERROR_LABELS: Record<string, string> = {
  timeout: 'Timed out',
  blocked: 'Blocked',
  challenge_required: 'Needs browser check',
  auth_failed: 'Login failed',
  rate_limited: 'Rate limited',
  parse_failed: 'Site changed',
  http: 'Server error',
  network: 'Network error',
};
