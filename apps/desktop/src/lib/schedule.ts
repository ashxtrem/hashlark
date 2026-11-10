// SPDX-License-Identifier: GPL-3.0-or-later

//! The weekly schedule grid: 7 days (Monday first) by 24 hours, stored as a
//! 168-character string of `n`, `a`, `s` or `o` (plan §4.1).

export type Cell = 'n' | 'a' | 's' | 'o';

export const WEEK_LEN = 168;

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const;

export const MODES: { cell: Cell; key: string; label: string; letter: string; tone: string }[] = [
  { cell: 'n', key: '1', label: 'Normal', letter: 'N', tone: 'bg-ok/25' },
  { cell: 'a', key: '2', label: 'Alternative', letter: 'A', tone: 'bg-accent/40' },
  { cell: 's', key: '3', label: 'Seed only', letter: 'S', tone: 'bg-warn/40' },
  { cell: 'o', key: '4', label: 'Off', letter: 'O', tone: 'bg-err/35' },
];

export function isCell(c: string): c is Cell {
  return c === 'n' || c === 'a' || c === 's' || c === 'o';
}

/** Exactly 168 valid cells (what the server accepts). */
export function isValidGrid(grid: string): boolean {
  return grid.length === WEEK_LEN && [...grid].every(isCell);
}

/** A week where every hour is `cell`. */
export function filled(cell: Cell = 'n'): string {
  return cell.repeat(WEEK_LEN);
}

/** The grid as 7 rows of 24 cells; a bad cell reads as Normal. */
export function toRows(grid: string): Cell[][] {
  const cells = Array.from({ length: WEEK_LEN }, (_, i) => {
    const c = grid[i] ?? 'n';
    return isCell(c) ? c : 'n';
  });
  return DAYS.map((_, day) => cells.slice(day * 24, day * 24 + 24));
}

/** Rows back to the 168-character string. */
export function fromRows(rows: Cell[][]): string {
  return rows.map((r) => r.join('')).join('');
}

/** Index of an hour of a day (0 = Monday 00:00). */
export const indexOf = (day: number, hour: number): number => day * 24 + hour;

/** `grid` with cell `index` set to `cell`. */
export function setCell(grid: string, index: number, cell: Cell): string {
  if (index < 0 || index >= WEEK_LEN) return grid;
  const padded = grid.padEnd(WEEK_LEN, 'n');
  return padded.slice(0, index) + cell + padded.slice(index + 1);
}

/** Every index in the rectangle between two cells (a drag from `a` to `b`). */
export function rectIndices(a: number, b: number): number[] {
  const [d1, d2] = [Math.floor(a / 24), Math.floor(b / 24)].sort((x, y) => x - y) as [number, number];
  const [h1, h2] = [a % 24, b % 24].sort((x, y) => x - y) as [number, number];
  const out: number[] = [];
  for (let d = d1; d <= d2; d++) for (let h = h1; h <= h2; h++) out.push(indexOf(d, h));
  return out;
}

/** Paints the rectangle between two cells (a drag) with `cell`. */
export function paintRect(grid: string, from: number, to: number, cell: Cell): string {
  const cells = [...grid.padEnd(WEEK_LEN, 'n')];
  for (const i of rectIndices(from, to)) cells[i] = cell;
  return cells.join('');
}

/** Where an arrow key moves the cursor; it stops at the edges of the grid. */
export function moveCursor(index: number, key: string): number {
  const day = Math.floor(index / 24);
  const hour = index % 24;
  switch (key) {
    case 'ArrowLeft':
      return indexOf(day, Math.max(0, hour - 1));
    case 'ArrowRight':
      return indexOf(day, Math.min(23, hour + 1));
    case 'ArrowUp':
      return indexOf(Math.max(0, day - 1), hour);
    case 'ArrowDown':
      return indexOf(Math.min(6, day + 1), hour);
    case 'Home':
      return indexOf(day, 0);
    case 'End':
      return indexOf(day, 23);
    default:
      return index;
  }
}

/** The cell a number key (1–4) sets, if it is one. */
export function cellForKey(key: string): Cell | null {
  return MODES.find((m) => m.key === key)?.cell ?? null;
}

/** Index of the local hour containing `now` (the grid starts on Monday). */
export function currentIndex(now: Date = new Date()): number {
  return indexOf((now.getDay() + 6) % 7, now.getHours());
}

export function modeLabel(cell: Cell): string {
  return MODES.find((m) => m.cell === cell)?.label ?? 'Normal';
}

export const hourLabel = (hour: number): string => `${String(hour).padStart(2, '0')}:00`;
