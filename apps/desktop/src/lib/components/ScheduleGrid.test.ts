// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { filled } from '$lib/schedule';
import ScheduleGrid from './ScheduleGrid.svelte';
import Harness from './ScheduleGridHarness.svelte';

afterEach(cleanup);

const cell = (index: number) => document.querySelector<HTMLElement>(`[data-index="${index}"]`)!;
const value = () => screen.getByTestId('value').textContent!;

describe('ScheduleGrid', () => {
  it('shows 7 x 24 cells with the current hour marked', () => {
    render(ScheduleGrid, { value: filled() });
    expect(document.querySelectorAll('[role="gridcell"]')).toHaveLength(168);
    expect(document.querySelectorAll('[aria-current="time"]')).toHaveLength(1);
    expect(cell(0).getAttribute('aria-label')).toBe('Mon 00:00, Normal');
  });

  it('paints a dragged rectangle with the legend mode', async () => {
    render(Harness, { initial: filled() });
    await fireEvent.click(screen.getByRole('radio', { name: 'Off' }));
    await fireEvent.pointerDown(cell(24 + 8), { button: 0 });
    await fireEvent.pointerEnter(cell(24 + 9));
    await fireEvent.pointerEnter(cell(48 + 9));
    await fireEvent.pointerUp(window);
    // Entering cells after the button is up paints nothing more.
    await fireEvent.pointerEnter(cell(100));

    const grid = value();
    expect(grid).toHaveLength(168);
    const off = [...grid].flatMap((c, i) => (c === 'o' ? [i] : []));
    expect(off).toEqual([32, 33, 56, 57]);
  });

  it('follows the drag back when the pointer returns', async () => {
    render(Harness, { initial: filled() });
    await fireEvent.pointerDown(cell(10), { button: 0 });
    await fireEvent.pointerEnter(cell(14));
    await fireEvent.pointerEnter(cell(12));
    await fireEvent.pointerUp(window);
    // The default brush is Alternative.
    expect(value().slice(8, 16)).toBe('nnaaannn');
  });

  it('moves the cursor with the arrows and sets modes with 1-4', async () => {
    render(Harness, { initial: filled() });
    cell(0).focus();
    await fireEvent.keyDown(cell(0), { key: 'ArrowRight' });
    expect(document.activeElement).toBe(cell(1));
    await fireEvent.keyDown(cell(1), { key: 'ArrowDown' });
    expect(document.activeElement).toBe(cell(25));

    await fireEvent.keyDown(cell(25), { key: '4' });
    expect(value()[25]).toBe('o');
    await fireEvent.keyDown(cell(25), { key: '3' });
    expect(value()[25]).toBe('s');
    await fireEvent.keyDown(cell(25), { key: '2' });
    expect(value()[25]).toBe('a');
    await fireEvent.keyDown(cell(25), { key: '1' });
    expect(value()).toBe(filled());

    // Only the cursor cell is in the tab order.
    expect(document.querySelectorAll('[role="gridcell"][tabindex="0"]')).toHaveLength(1);
    expect(cell(25).getAttribute('tabindex')).toBe('0');
  });
});
