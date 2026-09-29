// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { SseParser } from './api/sse';
import { age, bytes, count, duration } from './format';
import { sortResults } from './sort';
import type { MergedResult } from './api/client';

describe('SseParser', () => {
  it('parses events split across chunks', () => {
    const p = new SseParser();
    expect(p.push('event: results\nda')).toEqual([]);
    expect(p.push('ta: {"a":1}\n')).toEqual([]);
    expect(p.push('\n')).toEqual([{ event: 'results', data: '{"a":1}' }]);
  });

  it('handles CRLF, comments, multi-line data and default event name', () => {
    const p = new SseParser();
    const out = p.push(':keep-alive\r\ndata: one\r\ndata: two\r\n\r\nevent: done\ndata:{}\n\n');
    expect(out).toEqual([
      { event: 'message', data: 'one\ntwo' },
      { event: 'done', data: '{}' },
    ]);
  });

  it('ignores blank dispatches without data', () => {
    expect(new SseParser().push('event: x\n\n')).toEqual([]);
  });
});

describe('format', () => {
  it('formats bytes', () => {
    expect(bytes(512)).toBe('512 B');
    expect(bytes(1536)).toBe('1.5 KiB');
    expect(bytes(6_114_770_944)).toBe('5.7 GiB');
    expect(bytes(null)).toBe('–');
  });

  it('formats age and durations', () => {
    const now = new Date('2026-09-28T12:00:00Z');
    expect(age('2026-09-28T07:00:00Z', now)).toBe('5h');
    expect(age('2026-09-25T12:00:00Z', now)).toBe('3d');
    expect(age('2025-08-01T00:00:00Z', now)).toBe('1y');
    expect(age('2027-01-01T00:00:00Z', now)).toBe('0h');
    expect(age(null, now)).toBe('–');
    expect(duration(250)).toBe('250 ms');
    expect(duration(1234)).toBe('1.2 s');
    expect(count(undefined)).toBe('–');
  });
});

function result(id: string, seeders: number | null, size: number | null, score = 0): MergedResult {
  return {
    id,
    score,
    seeders,
    leechers: null,
    sources: ['p'],
    primary: {
      title: id,
      provider_id: 'p',
      size_bytes: size,
      needs_resolve: false,
    } as MergedResult['primary'],
  };
}

describe('sortResults', () => {
  it('matches the core ordering rules', () => {
    const rs = [result('a', null, 10, 0.9), result('b', 5, null), result('c', 50, 5)];
    expect(sortResults(rs, 'seeders').map((r) => r.id)).toEqual(['c', 'b', 'a']);
    expect(sortResults(rs, 'size').map((r) => r.id)).toEqual(['a', 'c', 'b']);
    expect(sortResults(rs, 'relevance')[0]!.id).toBe('a');
    expect(sortResults(rs, 'title').map((r) => r.id)).toEqual(['c', 'b', 'a']);
    expect(sortResults(rs, 'title', 'asc').map((r) => r.id)).toEqual(['a', 'b', 'c']);
    expect(sortResults(rs, 'seeders', 'asc').map((r) => r.id)).toEqual(['b', 'c', 'a']);
    expect(rs.map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });

  it('sorts by peers descending with unknowns last', () => {
    const rs = [
      { ...result('a', 1, 1), leechers: 1 },
      { ...result('b', 1, 1), leechers: 9 },
      { ...result('c', 1, 1), leechers: null },
    ];
    expect(sortResults(rs, 'peers').map((r) => r.id)).toEqual(['b', 'a', 'c']);
  });
});
