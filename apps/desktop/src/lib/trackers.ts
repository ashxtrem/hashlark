// SPDX-License-Identifier: GPL-3.0-or-later

//! The staged tracker list of the Trackers tab (plan
//! `docs/torrent-trackers-plan.md` §4): edits stay here until Save sends the
//! whole list. The engine checks again; this only catches obvious typos early.

/** URL schemes the engine accepts for trackers. */
export const TRACKER_SCHEMES = ['http', 'https', 'udp', 'ws', 'wss'] as const;

/** Why `url` isn't a tracker URL, or null when it looks fine. */
export function trackerError(url: string): string | null {
  const text = url.trim();
  if (text === '') return 'Enter a tracker URL.';
  if (/\s/.test(text)) return `Not a tracker URL (it contains spaces): ${text}`;
  let parsed: URL;
  try {
    parsed = new URL(text);
  } catch {
    return `Not a tracker URL: ${text}`;
  }
  const scheme = parsed.protocol.replace(/:$/, '').toLowerCase();
  if (!(TRACKER_SCHEMES as readonly string[]).includes(scheme)) {
    return `Trackers start with http, https, udp, ws or wss: ${text}`;
  }
  if (!parsed.host) return `Not a tracker URL (no host): ${text}`;
  return null;
}

/** Trimmed, non-blank lines of pasted text. */
export function trackerLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l !== '');
}

/** Drops repeated URLs (after trimming), keeping the first of each. */
export function dedupe(list: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of list) {
    const url = raw.trim();
    if (url === '' || seen.has(url)) continue;
    seen.add(url);
    out.push(url);
  }
  return out;
}

/** The result of {@link addTrackers}. */
export interface Added {
  list: string[];
  /** Lines that failed the check, left for the user to fix. */
  rejected: string[];
  /** One message per rejected line. */
  errors: string[];
}

/** Appends the valid lines of `text` to `list` (no repeats, order kept). */
export function addTrackers(list: string[], text: string): Added {
  const rejected: string[] = [];
  const errors: string[] = [];
  const good: string[] = [];
  for (const line of trackerLines(text)) {
    const problem = trackerError(line);
    if (problem) {
      rejected.push(line);
      errors.push(problem);
    } else {
      good.push(line);
    }
  }
  return { list: dedupe([...list, ...good]), rejected, errors };
}

/**
 * Replaces the URL at `index`. A URL already elsewhere in the list merges
 * into the first copy. Check it with {@link trackerError} first.
 */
export function editTracker(list: string[], index: number, url: string): string[] {
  if (index < 0 || index >= list.length) return list;
  const next = [...list];
  next[index] = url.trim();
  return dedupe(next);
}

export function removeTracker(list: string[], index: number): string[] {
  if (index < 0 || index >= list.length) return list;
  return list.filter((_, i) => i !== index);
}

/** Row to focus after removing row `index`: the next one, else the last; null when empty. */
export function focusAfterRemove(length: number, index: number): number | null {
  if (length === 0) return null;
  return Math.min(index, length - 1);
}

/** The staged list differs from the torrent's (order counts). */
export function trackersChanged(staged: string[], original: string[]): boolean {
  return staged.length !== original.length || staged.some((url, i) => url !== original[i]);
}
