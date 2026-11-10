// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import {
  cellForKey,
  currentIndex,
  filled,
  fromRows,
  indexOf,
  isValidGrid,
  moveCursor,
  paintRect,
  rectIndices,
  setCell,
  toRows,
  WEEK_LEN,
} from './schedule';

describe('grid <-> string', () => {
  it('round-trips 7 rows of 24 cells', () => {
    const grid = 'nasoasno'.repeat(21);
    expect(grid).toHaveLength(WEEK_LEN);
    const rows = toRows(grid);
    expect(rows).toHaveLength(7);
    expect(rows.every((r) => r.length === 24)).toBe(true);
    expect(fromRows(rows)).toBe(grid);
    // Monday 00:00 is the first character, Sunday 23:00 the last.
    expect(rows[0]![0]).toBe(grid[0]);
    expect(rows[6]![23]).toBe(grid[167]);
    expect(indexOf(6, 23)).toBe(167);
  });

  it('reads a short or bad string as Normal and validates like the server', () => {
    expect(toRows('a')[0]!.slice(0, 2)).toEqual(['a', 'n']);
    expect(toRows('x'.repeat(168))[3]![5]).toBe('n');
    expect(isValidGrid(filled('o'))).toBe(true);
    expect(isValidGrid('n'.repeat(167))).toBe(false);
    expect(isValidGrid('x'.repeat(168))).toBe(false);
  });
});

describe('painting', () => {
  it('sets one cell', () => {
    const grid = setCell(filled(), 25, 'o');
    expect(grid).toHaveLength(WEEK_LEN);
    expect(grid[25]).toBe('o');
    expect(grid.replace('o', '')).toBe('n'.repeat(167));
    expect(setCell(filled(), 168, 'o')).toBe(filled());
  });

  it('paints the rectangle of a drag, whichever way it goes', () => {
    // Tuesday 08:00 to Wednesday 10:00.
    const a = indexOf(1, 8);
    const b = indexOf(2, 10);
    expect(rectIndices(a, b)).toHaveLength(2 * 3);
    const grid = paintRect(filled(), a, b, 'a');
    expect([...grid].filter((c) => c === 'a')).toHaveLength(6);
    for (const day of [1, 2]) for (const h of [8, 9, 10]) expect(grid[indexOf(day, h)]).toBe('a');
    expect(paintRect(filled(), b, a, 'a')).toBe(grid);
    // A drag up and to the left of the start covers the same box.
    expect(paintRect(filled(), indexOf(2, 8), indexOf(1, 10), 'a')).toBe(grid);
  });

  it('a click paints one cell', () => {
    expect(paintRect(filled(), 30, 30, 's')).toBe(setCell(filled(), 30, 's'));
  });
});

describe('keyboard', () => {
  it('moves by cell and stops at the edges', () => {
    expect(moveCursor(indexOf(2, 5), 'ArrowRight')).toBe(indexOf(2, 6));
    expect(moveCursor(indexOf(2, 5), 'ArrowLeft')).toBe(indexOf(2, 4));
    expect(moveCursor(indexOf(2, 5), 'ArrowDown')).toBe(indexOf(3, 5));
    expect(moveCursor(indexOf(2, 5), 'ArrowUp')).toBe(indexOf(1, 5));
    expect(moveCursor(0, 'ArrowLeft')).toBe(0);
    expect(moveCursor(0, 'ArrowUp')).toBe(0);
    expect(moveCursor(167, 'ArrowRight')).toBe(167);
    expect(moveCursor(167, 'ArrowDown')).toBe(167);
    expect(moveCursor(indexOf(2, 5), 'Home')).toBe(indexOf(2, 0));
    expect(moveCursor(indexOf(2, 5), 'End')).toBe(indexOf(2, 23));
    expect(moveCursor(10, 'x')).toBe(10);
  });

  it('maps 1-4 to the four modes', () => {
    expect(['1', '2', '3', '4', '5'].map(cellForKey)).toEqual(['n', 'a', 's', 'o', null]);
  });
});

describe('current hour', () => {
  it('counts from Monday 00:00 in local time', () => {
    // 2026-11-09 is a Monday, 2026-11-15 a Sunday.
    expect(currentIndex(new Date(2026, 10, 9, 0, 30))).toBe(0);
    expect(currentIndex(new Date(2026, 10, 10, 13, 0))).toBe(24 + 13);
    expect(currentIndex(new Date(2026, 10, 15, 23, 59))).toBe(167);
  });
});
