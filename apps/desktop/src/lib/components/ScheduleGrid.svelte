<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import {
    cellForKey,
    currentIndex,
    DAYS,
    hourLabel,
    indexOf,
    modeLabel,
    MODES,
    moveCursor,
    paintRect,
    setCell,
    toRows,
    type Cell,
  } from '$lib/schedule';

  interface Props {
    /** 168 characters of `n`, `a`, `s` or `o`, Monday 00:00 first. */
    value: string;
    disabled?: boolean;
  }

  let { value = $bindable(), disabled = false }: Props = $props();

  /** The mode the mouse paints with. */
  let brush = $state<Cell>('a');
  /** The cell the arrow keys move; the only one in the tab order. */
  let cursor = $state(0);
  let rows = $derived(toRows(value));
  const now = currentIndex();

  let drag: { anchor: number; base: string } | null = null;
  let root: HTMLDivElement | undefined = $state();

  function focusCell(index: number) {
    cursor = index;
    root?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.focus();
  }

  function down(e: PointerEvent, index: number) {
    if (disabled || e.button !== 0) return;
    // Touch would keep sending events to this cell; let the others see them.
    (e.target as Element | null)?.releasePointerCapture?.(e.pointerId);
    drag = { anchor: index, base: value };
    cursor = index;
    value = paintRect(value, index, index, brush);
  }

  function enter(index: number) {
    if (drag) value = paintRect(drag.base, drag.anchor, index, brush);
  }

  function up() {
    drag = null;
  }

  function key(e: KeyboardEvent) {
    if (disabled) return;
    const cell = cellForKey(e.key);
    if (cell) {
      e.preventDefault();
      value = setCell(value, cursor, cell);
      return;
    }
    const next = moveCursor(cursor, e.key);
    if (next !== cursor || e.key.startsWith('Arrow')) {
      e.preventDefault();
      focusCell(next);
    }
  }
</script>

<svelte:window onpointerup={up} onpointercancel={up} />

<div class="space-y-3">
  <div role="radiogroup" aria-label="Mode to paint" class="flex flex-wrap items-center gap-2 text-sm">
    {#each MODES as m (m.cell)}
      <button
        type="button"
        role="radio"
        aria-checked={brush === m.cell}
        {disabled}
        class="btn-outline gap-2 px-2.5 py-1 {brush === m.cell ? 'border-accent text-accent' : ''}"
        onclick={() => (brush = m.cell)}
      >
        <span class="inline-block size-3.5 rounded-sm border border-line {m.tone}"></span>
        {m.label}
      </button>
    {/each}
  </div>

  <!-- Focus lives on one cell at a time (roving tabindex), not on the grid. -->
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    bind:this={root}
    role="grid"
    aria-label="Weekly schedule. Arrow keys move, 1 to 4 set Normal, Alternative, Seed only or Off."
    aria-disabled={disabled}
    class="select-none"
    onkeydown={key}
  >
    <div role="row" class="mb-1 grid grid-cols-[2.5rem_repeat(24,minmax(0,1fr))] text-[10px] text-muted">
      <span></span>
      {#each { length: 24 } as _, h (h)}
        <span role="columnheader" class="text-center" aria-label={hourLabel(h)}>{h % 3 === 0 ? h : ''}</span>
      {/each}
    </div>
    {#each rows as row, day (day)}
      <div role="row" class="mb-px grid grid-cols-[2.5rem_repeat(24,minmax(0,1fr))] gap-px">
        <span role="rowheader" class="self-center text-xs text-muted">{DAYS[day]}</span>
        {#each row as cell, hour (hour)}
          {@const index = indexOf(day, hour)}
          {@const mode = MODES.find((m) => m.cell === cell)!}
          <div
            role="gridcell"
            tabindex={index === cursor ? 0 : -1}
            data-index={index}
            data-cell={cell}
            aria-label="{DAYS[day]} {hourLabel(hour)}, {modeLabel(cell)}"
            aria-current={index === now ? 'time' : undefined}
            class="flex h-6 items-center justify-center rounded-[3px] text-[9px] leading-none text-fg/60 focus-visible:outline-2 focus-visible:outline-fg {mode.tone} {index ===
            now
              ? 'outline-2 outline-offset-1 outline-fg'
              : ''} {disabled ? 'opacity-50' : 'cursor-pointer'}"
            onpointerdown={(e) => down(e, index)}
            onpointerenter={() => enter(index)}
            onfocus={() => (cursor = index)}
          >
            {mode.letter}
          </div>
        {/each}
      </div>
    {/each}
  </div>
  <p class="hint">
    Drag across hours to paint them with the chosen mode. With the keyboard, move with the arrow keys and press
    1 (Normal), 2 (Alternative), 3 (Seed only) or 4 (Off). The outlined hour is now. Hours are local time on the
    computer running Hashlark.
  </p>
</div>
