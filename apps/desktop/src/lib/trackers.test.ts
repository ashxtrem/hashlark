// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest';
import {
  addTrackers,
  dedupe,
  editTracker,
  focusAfterRemove,
  removeTracker,
  trackerError,
  trackerLines,
  trackersChanged,
} from './trackers';

const A = 'udp://a.example:1337/announce';
const B = 'https://b.example/announce';
const C = 'wss://c.example/announce';

describe('tracker list', () => {
  it('accepts the engine schemes', () => {
    for (const url of [A, B, C, 'http://d.example/a', 'ws://e.example:80/']) {
      expect(trackerError(url)).toBeNull();
    }
  });

  it('flags typos', () => {
    expect(trackerError('udb://a.example/announce')).toMatch(/http, https, udp, ws or wss/);
    expect(trackerError('ftp://a.example/')).toMatch(/http, https, udp, ws or wss/);
    expect(trackerError('a.example/announce')).toMatch(/Not a tracker URL/);
    expect(trackerError('udp://a.example/ announce')).toMatch(/spaces/);
    expect(trackerError('   ')).toBe('Enter a tracker URL.');
  });

  it('ignores blank lines', () => {
    expect(trackerLines(`\n ${A} \r\n\n${B}\n  \n`)).toEqual([A, B]);
  });

  it('dedupes keeping the first of each', () => {
    expect(dedupe([B, A, ` ${B}`, C, A])).toEqual([B, A, C]);
  });

  it('adds many, keeps order, drops repeats and returns the bad lines', () => {
    const res = addTrackers([A], `${B}\n\n${A}\nudb://typo/announce\n${C}`);
    expect(res.list).toEqual([A, B, C]);
    expect(res.rejected).toEqual(['udb://typo/announce']);
    expect(res.errors).toHaveLength(1);
  });

  it('edits in place and merges into an existing copy', () => {
    expect(editTracker([A, B, C], 1, ' http://x.example/a ')).toEqual([A, 'http://x.example/a', C]);
    expect(editTracker([A, B, C], 2, A)).toEqual([A, B]);
    expect(editTracker([A], 5, B)).toEqual([A]);
  });

  it('removes a row and picks the row to focus next', () => {
    const list = removeTracker([A, B, C], 1);
    expect(list).toEqual([A, C]);
    expect(focusAfterRemove(list.length, 1)).toBe(1);
    expect(focusAfterRemove(1, 1)).toBe(0);
    expect(focusAfterRemove(0, 0)).toBeNull();
    expect(removeTracker([A], 3)).toEqual([A]);
  });

  it('diffs against the original, order included', () => {
    expect(trackersChanged([A, B], [A, B])).toBe(false);
    expect(trackersChanged([B, A], [A, B])).toBe(true);
    expect(trackersChanged([A], [A, B])).toBe(true);
    expect(trackersChanged([], [])).toBe(false);
  });
});
