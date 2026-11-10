<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import {
    ArrowDown,
    ArrowDownToLine,
    ArrowUp,
    ArrowUpToLine,
    Ellipsis,
    GripVertical,
    Info,
    Pause,
    Play,
    Square,
    Trash2,
    Zap,
  } from '@lucide/svelte';
  import type { TorrentSummary } from '$lib/api/client';
  import { bytes } from '$lib/format';
  import type { RowAction } from '$lib/state/torrents.svelte';
  import { eta, percent, ratio, speed, stateTone, statusLabel, type QueueStep, type Tone } from '$lib/torrents';

  interface Props {
    rows: TorrentSummary[];
    /** The Queued tab: show queue numbers and allow dragging. */
    queued: boolean;
    selected: Set<string>;
    focused: string | null;
    /** Queue number (1-based) of each torrent in the whole queue. */
    positions: Map<string, number>;
    onselect: (id: string, e: MouseEvent) => void;
    ontoggle: (id: string) => void;
    onopen: (id: string) => void;
    onaction: (ids: string[], action: RowAction) => void;
    onmove: (ids: string[], step: QueueStep) => void;
    ondelete: (ids: string[]) => void;
    /** Drag and drop: put `id` in `targetId`'s place. */
    onreorder: (id: string, targetId: string) => void;
    onkeydown: (e: KeyboardEvent) => void;
  }

  let {
    rows,
    queued,
    selected,
    focused,
    positions,
    onselect,
    ontoggle,
    onopen,
    onaction,
    onmove,
    ondelete,
    onreorder,
    onkeydown,
  }: Props = $props();

  let dragId = $state<string | null>(null);
  let dropId = $state<string | null>(null);
  const DRAG_TYPE = 'application/x-hashlark-torrent';

  const TONES: Record<Tone, string> = {
    accent: 'text-accent',
    ok: 'text-ok',
    warn: 'text-warn',
    err: 'text-err',
    muted: 'text-muted',
  };
  const BARS: Record<Tone, string> = {
    accent: 'bg-accent',
    ok: 'bg-ok',
    warn: 'bg-warn',
    err: 'bg-err',
    muted: 'bg-muted',
  };

  // Columns: checkbox, [queue #], name, size, progress, status, ↓, [↑], ETA, [ratio, peers], actions.
  let cols = $derived(
    queued
      ? 'min-[720px]:grid-cols-[1.25rem_2rem_minmax(0,1fr)_4.5rem_6.5rem_8rem_5rem_4rem_5.5rem] xl:grid-cols-[1.25rem_2rem_minmax(0,1fr)_4.5rem_6.5rem_8rem_5rem_5rem_4rem_3rem_3.5rem_5.5rem]'
      : 'min-[720px]:grid-cols-[1.25rem_minmax(0,1fr)_4.5rem_6.5rem_8rem_5rem_4rem_5.5rem] xl:grid-cols-[1.25rem_minmax(0,1fr)_4.5rem_6.5rem_8rem_5rem_5rem_4rem_3rem_3.5rem_5.5rem]',
  );

  function pauseLabel(t: TorrentSummary) {
    return t.run_mode === 'paused' ? 'Resume' : 'Pause';
  }

  function stopLabel(t: TorrentSummary) {
    return t.run_mode === 'stopped' ? 'Start' : 'Stop';
  }

  /** Places a popover menu under its button. */
  function placeMenu(e: ToggleEvent, button: HTMLElement | null) {
    if (e.newState !== 'open' || !button) return;
    const menu = e.currentTarget as HTMLElement;
    const r = button.getBoundingClientRect();
    menu.style.top = `${Math.min(r.bottom + 4, window.innerHeight - menu.offsetHeight - 8)}px`;
    menu.style.left = `${Math.max(8, r.right - menu.offsetWidth)}px`;
  }

  function closeMenu(id: string) {
    document.getElementById(`menu-${id}`)?.hidePopover();
  }

  let list: HTMLElement | undefined = $state();

  /** Opens the focused row's menu from the keyboard (Menu key or Shift+F10). */
  function onListKey(e: KeyboardEvent) {
    if (e.target === e.currentTarget && focused && (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey))) {
      e.preventDefault();
      const menu = document.getElementById(`menu-${focused}`);
      menu?.showPopover();
      menu?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
      return;
    }
    onkeydown(e);
  }

  /** Arrow keys move between menu items; Escape and Tab return to the list. */
  function onMenuKey(e: KeyboardEvent, id: string) {
    const items = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="menuitem"]')];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const step = e.key === 'ArrowDown' ? 1 : -1;
      items[(at + step + items.length) % items.length]?.focus();
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      items[e.key === 'Home' ? 0 : items.length - 1]?.focus();
    } else if (e.key === 'Escape' || e.key === 'Tab') {
      e.preventDefault();
      e.stopPropagation();
      closeMenu(id);
      list?.focus();
    }
  }

  function stop(e: Event) {
    e.stopPropagation();
  }
</script>

{#snippet bar(t: TorrentSummary)}
  <span class="block h-1.5 overflow-hidden rounded-full bg-line" aria-hidden="true">
    <span class="block h-full rounded-full {BARS[stateTone(t.state)]}" style="width: {Math.min(100, t.progress * 100)}%"></span>
  </span>
{/snippet}

{#snippet menu(t: TorrentSummary)}
  {@const ids = selected.has(t.id) ? [...selected] : [t.id]}
  <div
    id="menu-{t.id}"
    popover
    role="menu"
    aria-label="Actions for {t.name}"
    tabindex="-1"
    class="fixed m-0 w-48 rounded-xl border border-line bg-surface p-1 text-sm text-fg shadow-card"
    ontoggle={(e) => placeMenu(e, document.getElementById(`menu-btn-${t.id}`))}
    onkeydown={(e) => onMenuKey(e, t.id)}
  >
    {#each [
      { label: 'Force start', icon: Zap, run: () => onaction(ids, 'force_start') },
      { label: 'Move to top', icon: ArrowUpToLine, run: () => onmove(ids, 'top') },
      { label: 'Move up', icon: ArrowUp, run: () => onmove(ids, 'up') },
      { label: 'Move down', icon: ArrowDown, run: () => onmove(ids, 'down') },
      { label: 'Move to bottom', icon: ArrowDownToLine, run: () => onmove(ids, 'bottom') },
      { label: 'Details', icon: Info, run: () => onopen(t.id) },
    ] as item (item.label)}
      <button
        role="menuitem"
        tabindex="-1"
        class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left hover:bg-surface-2 focus:bg-surface-2 focus:outline-none"
        onclick={() => {
          closeMenu(t.id);
          item.run();
        }}
      >
        <item.icon size={15} class="text-muted" />
        {item.label}
      </button>
    {/each}
    <button
      role="menuitem"
      tabindex="-1"
      class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-err hover:bg-err-soft focus:bg-err-soft focus:outline-none"
      onclick={() => {
        closeMenu(t.id);
        ondelete(ids);
      }}
    >
      <Trash2 size={15} /> Delete…
    </button>
  </div>
{/snippet}

<div class="card overflow-hidden">
  <div
    class="hidden gap-3 border-b border-line bg-surface-2 px-4 py-2 text-xs font-medium tracking-wide text-muted uppercase min-[720px]:grid {cols}"
    aria-hidden="true"
  >
    <span></span>
    {#if queued}<span class="text-right">#</span>{/if}
    <span>Name</span>
    <span class="text-right">Size</span>
    <span>Progress</span>
    <span>Status</span>
    <span class="text-right">↓</span>
    <span class="hidden text-right xl:block">↑</span>
    <span class="text-right">ETA</span>
    <span class="hidden text-right xl:block">Ratio</span>
    <span class="hidden text-right xl:block">Peers</span>
    <span class="sr-only">Actions</span>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <ul
    role="listbox"
    aria-label="Torrents"
    aria-multiselectable="true"
    aria-activedescendant={focused ? `torrent-${focused}` : undefined}
    tabindex="0"
    class="select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset"
    bind:this={list}
    onkeydown={onListKey}
  >
    {#each rows as t (t.id)}
      {@const isSel = selected.has(t.id)}
      <!-- Mouse shortcuts; the keyboard path is on the list (arrows, Space, Enter…). -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id="torrent-{t.id}"
        role="option"
        aria-selected={isSel}
        class="border-b border-line last:border-b-0 {isSel ? 'bg-accent-soft' : 'hover:bg-surface-2'} {focused === t.id
          ? 'outline-2 -outline-offset-2 outline-accent/60'
          : ''} {dropId === t.id && dragId !== t.id ? 'border-t-2 border-t-accent' : ''}"
        draggable={queued}
        onclick={(e) => {
          // Clicks in the row's menu (a popover inside the row) aren't row clicks.
          if (!(e.target as HTMLElement).closest('[popover]')) onselect(t.id, e);
        }}
        ondblclick={(e) => {
          if (!(e.target as HTMLElement).closest('[popover]')) onopen(t.id);
        }}
        ondragstart={(e) => {
          dragId = t.id;
          e.dataTransfer?.setData(DRAG_TYPE, t.id);
          if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
        }}
        ondragover={(e) => {
          if (!dragId || !e.dataTransfer?.types.includes(DRAG_TYPE)) return;
          e.preventDefault();
          e.stopPropagation();
          dropId = t.id;
        }}
        ondrop={(e) => {
          if (!dragId || !e.dataTransfer?.types.includes(DRAG_TYPE)) return;
          e.preventDefault();
          e.stopPropagation();
          if (dragId !== t.id) onreorder(dragId, t.id);
          dragId = null;
          dropId = null;
        }}
        ondragend={() => {
          dragId = null;
          dropId = null;
        }}
      >
        <!-- Wide: a table row. -->
        <div class="hidden items-center gap-3 px-4 py-2 text-sm min-[720px]:grid {cols}">
          <input
            type="checkbox"
            class="size-4 accent-[var(--hl-accent)]"
            aria-label="Select {t.name}"
            checked={isSel}
            tabindex="-1"
            onclick={stop}
            onchange={() => ontoggle(t.id)}
          />
          {#if queued}
            <span class="flex items-center justify-end gap-0.5 text-xs text-muted tabular-nums">
              <GripVertical size={13} class="cursor-grab" aria-hidden="true" />{positions.get(t.id)}
            </span>
          {/if}
          <button
            class="min-w-0 truncate text-left font-medium focus:outline-none focus-visible:underline"
            title={t.name}
            tabindex="-1"
            onclick={(e) => {
              e.stopPropagation();
              onopen(t.id);
            }}
          >
            {t.name}
          </button>
          <span class="text-right text-muted tabular-nums">{t.size ? bytes(t.size) : '–'}</span>
          <span class="flex items-center gap-1.5">
            <span class="flex-1">{@render bar(t)}</span>
            <span class="w-11 text-right text-xs text-muted tabular-nums">{percent(t.progress)}</span>
          </span>
          <span class="truncate text-xs font-medium {TONES[stateTone(t.state)]}" title={t.error ?? statusLabel(t)}>
            {statusLabel(t)}
          </span>
          <span class="text-right tabular-nums">{speed(t.down_bps)}</span>
          <span class="hidden text-right tabular-nums xl:block">{speed(t.up_bps)}</span>
          <span class="text-right text-muted tabular-nums">{eta(t.eta_secs)}</span>
          <span class="hidden text-right text-muted tabular-nums xl:block">{ratio(t.ratio)}</span>
          <span class="hidden text-right text-muted tabular-nums xl:block" title="{t.seeds} seeds">{t.peers}</span>
          <span class="flex justify-end gap-0.5">
            <button
              class="icon-btn"
              tabindex="-1"
              title={pauseLabel(t)}
              aria-label="{pauseLabel(t)} {t.name}"
              onclick={(e) => {
                e.stopPropagation();
                onaction([t.id], t.run_mode === 'paused' ? 'resume' : 'pause');
              }}
            >
              {#if t.run_mode === 'paused'}<Play size={16} />{:else}<Pause size={16} />{/if}
            </button>
            <button
              class="icon-btn"
              tabindex="-1"
              title={stopLabel(t)}
              aria-label="{stopLabel(t)} {t.name}"
              onclick={(e) => {
                e.stopPropagation();
                onaction([t.id], t.run_mode === 'stopped' ? 'start' : 'stop');
              }}
            >
              {#if t.run_mode === 'stopped'}<Play size={16} class="text-ok" />{:else}<Square size={15} />{/if}
            </button>
            <button
              id="menu-btn-{t.id}"
              class="icon-btn"
              tabindex="-1"
              title="More actions"
              aria-label="More actions for {t.name}"
              popovertarget="menu-{t.id}"
              onclick={stop}
            >
              <Ellipsis size={16} />
            </button>
          </span>
        </div>

        <!-- Narrow: a card. -->
        <div class="space-y-2 px-3 py-3 min-[720px]:hidden">
          <div class="flex items-start gap-2.5">
            <input
              type="checkbox"
              class="mt-0.5 size-4 shrink-0 accent-[var(--hl-accent)]"
              aria-label="Select {t.name}"
              checked={isSel}
              onclick={stop}
              onchange={() => ontoggle(t.id)}
            />
            <button
              class="min-w-0 flex-1 text-left text-sm font-medium break-words"
              onclick={(e) => {
                e.stopPropagation();
                onopen(t.id);
              }}
            >
              {#if queued}<span class="mr-1 text-muted tabular-nums">#{positions.get(t.id)}</span>{/if}{t.name}
            </button>
          </div>
          {@render bar(t)}
          <p class="flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-muted tabular-nums">
            <span class="font-medium {TONES[stateTone(t.state)]}">{statusLabel(t)}</span>
            <span>{percent(t.progress)} of {t.size ? bytes(t.size) : '–'}</span>
            <span>↓ {speed(t.down_bps)}</span>
            <span>↑ {speed(t.up_bps)}</span>
            {#if t.eta_secs != null}<span>ETA {eta(t.eta_secs)}</span>{/if}
            <span>Ratio {ratio(t.ratio)}</span>
            <span>{t.peers} peers</span>
          </p>
          <div class="flex flex-wrap gap-1">
            <button
              class="btn-outline px-2 py-1 text-xs"
              onclick={(e) => {
                e.stopPropagation();
                onaction([t.id], t.run_mode === 'paused' ? 'resume' : 'pause');
              }}
            >
              {#if t.run_mode === 'paused'}<Play size={14} />{:else}<Pause size={14} />{/if}{pauseLabel(t)}
            </button>
            <button
              class="btn-outline px-2 py-1 text-xs"
              onclick={(e) => {
                e.stopPropagation();
                onaction([t.id], t.run_mode === 'stopped' ? 'start' : 'stop');
              }}
            >
              {#if t.run_mode === 'stopped'}<Play size={14} />{:else}<Square size={13} />{/if}{stopLabel(t)}
            </button>
            <button
              class="btn-outline px-2 py-1 text-xs"
              onclick={(e) => {
                e.stopPropagation();
                onaction([t.id], 'force_start');
              }}
            >
              <Zap size={14} /> Force
            </button>
            {#if queued}
              <button class="icon-btn" aria-label="Move {t.name} to top" onclick={(e) => { e.stopPropagation(); onmove([t.id], 'top'); }}>
                <ArrowUpToLine size={15} />
              </button>
              <button class="icon-btn" aria-label="Move {t.name} up" onclick={(e) => { e.stopPropagation(); onmove([t.id], 'up'); }}>
                <ArrowUp size={15} />
              </button>
              <button class="icon-btn" aria-label="Move {t.name} down" onclick={(e) => { e.stopPropagation(); onmove([t.id], 'down'); }}>
                <ArrowDown size={15} />
              </button>
              <button class="icon-btn" aria-label="Move {t.name} to bottom" onclick={(e) => { e.stopPropagation(); onmove([t.id], 'bottom'); }}>
                <ArrowDownToLine size={15} />
              </button>
            {/if}
            <button
              class="icon-btn ml-auto text-err hover:text-err"
              aria-label="Delete {t.name}"
              onclick={(e) => {
                e.stopPropagation();
                ondelete([t.id]);
              }}
            >
              <Trash2 size={15} />
            </button>
          </div>
        </div>
        {@render menu(t)}
      </li>
    {/each}
  </ul>
</div>
