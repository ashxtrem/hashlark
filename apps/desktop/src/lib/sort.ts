// SPDX-License-Identifier: GPL-3.0-or-later

import type { MergedResult, SortOrder } from '$lib/api/client';

export type SortDir = 'asc' | 'desc';

/** Numeric order with null/undefined after every value, in either direction. */
function cmpNullable(
  a: number | null | undefined,
  b: number | null | undefined,
  dir: SortDir,
): number {
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  return dir === 'desc' ? b - a : a - b;
}

function time(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const t = new Date(iso).getTime();
  return Number.isNaN(t) ? null : t;
}

/**
 * Sorts merged results the same way the core does (`ranking::sort`):
 * the chosen key first, unknown values last, then relevance.
 * `dir` defaults to descending (newest, largest, most seeders, Z–A).
 */
export function sortResults(
  results: MergedResult[],
  order: SortOrder,
  dir: SortDir = 'desc',
): MergedResult[] {
  return [...results].sort((a, b) => {
    let primary = 0;
    if (order === 'title') {
      const cmp = a.primary.title.localeCompare(b.primary.title, undefined, { sensitivity: 'base' });
      primary = dir === 'desc' ? -cmp : cmp;
    } else if (order === 'seeders') primary = cmpNullable(a.seeders, b.seeders, dir);
    else if (order === 'peers') primary = cmpNullable(a.leechers, b.leechers, dir);
    else if (order === 'size') primary = cmpNullable(a.primary.size_bytes, b.primary.size_bytes, dir);
    else if (order === 'date') primary = cmpNullable(time(a.primary.published), time(b.primary.published), dir);
    return primary || b.score - a.score || cmpNullable(a.seeders, b.seeders, 'desc');
  });
}
