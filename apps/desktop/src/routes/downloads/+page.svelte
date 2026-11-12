<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import {
    ArrowDown,
    ArrowDownToLine,
    ArrowUp,
    ArrowUpToLine,
    FilePlus2,
    FileUp,
    Pause,
    Play,
    Plus,
    Search,
    Square,
    Trash2,
    X,
    Zap,
  } from '@lucide/svelte';
  import DeleteTorrentDialog from '$components/DeleteTorrentDialog.svelte';
  import RecheckTorrentDialog from '$components/RecheckTorrentDialog.svelte';
  import TorrentDetails from '$components/TorrentDetails.svelte';
  import TorrentList from '$components/TorrentList.svelte';
  import { isTauri, type WhenDoneAction } from '$lib/platform';
  import { toasts } from '$lib/state/toasts.svelte';
  import { torrents, type RowAction } from '$lib/state/torrents.svelte';
  import {
    byQueue,
    FILTER_LABELS,
    FILTERS,
    filterCounts,
    isMagnet,
    pauseToggle,
    queueTarget,
    dropIndex,
    rangeIds,
    stepSelection,
    stopToggle,
    visibleTorrents,
    WHEN_DONE_LABELS,
    type Filter,
    type QueueStep,
  } from '$lib/torrents';

  let filter = $state<Filter>('all');
  let nameFilter = $state('');
  let magnet = $state('');
  let selected = $state(new Set<string>());
  let focused = $state<string | null>(null);
  let anchor = $state<string | null>(null);
  let detailsId = $state<string | null>(null);
  /** Torrents in the Delete dialog, named when it opened (not live). */
  let deleteTargets = $state<{ id: string; name: string }[]>([]);
  /** Torrents in the Force recheck dialog. */
  let recheckIds = $state<string[]>([]);
  let fileInput: HTMLInputElement | undefined = $state();
  let pickShift = false;
  let shiftHeld = false;

  let counts = $derived(filterCounts(torrents.list));
  let rows = $derived(visibleTorrents(torrents.list, filter, nameFilter));
  let rowIds = $derived(rows.map((t) => t.id));
  let positions = $derived(new Map(byQueue(torrents.list).map((t, i) => [t.id, i + 1])));
  let selectedRows = $derived(torrents.list.filter((t) => selected.has(t.id)));
  let desktop = isTauri();

  // Drop selected ids that left the list or the current tab.
  $effect(() => {
    const shown = new Set(rowIds);
    if ([...selected].some((id) => !shown.has(id))) {
      selected = new Set([...selected].filter((id) => shown.has(id)));
    }
    if (focused && !shown.has(focused)) focused = null;
  });

  function select(id: string, e: MouseEvent) {
    if (e.shiftKey) {
      selected = new Set(rangeIds(rowIds, anchor ?? focused, id));
    } else if (e.ctrlKey || e.metaKey) {
      toggle(id);
      anchor = id;
    } else {
      selected = new Set([id]);
      anchor = id;
    }
    focused = id;
    (document.querySelector('[role="listbox"][aria-label="Torrents"]') as HTMLElement | null)?.focus();
  }

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
    focused = id;
  }

  function act(ids: string[], action: RowAction) {
    void torrents.act(ids, action);
  }

  function move(ids: string[], step: QueueStep) {
    const [only] = ids;
    const index = ids.length === 1 && only ? queueTarget(torrents.list, rows, only, step) : null;
    if (ids.length === 1 && index === null) return;
    void torrents.move(ids, step, index);
  }

  function reorder(id: string, targetId: string) {
    const index = dropIndex(torrents.list, targetId);
    if (index !== null) void torrents.reorder(id, index);
  }

  function askDelete(ids: string[]) {
    deleteTargets = ids.map((id) => ({ id, name: torrents.list.find((t) => t.id === id)?.name ?? id }));
  }

  async function confirmDelete(ids: string[], deleteFiles: boolean) {
    deleteTargets = [];
    if (detailsId && ids.includes(detailsId)) detailsId = null;
    selected = new Set([...selected].filter((id) => !ids.includes(id)));
    await torrents.remove(ids, deleteFiles);
  }

  function askRecheck(ids: string[]) {
    recheckIds = [...ids];
  }

  async function confirmRecheck(ids: string[]) {
    recheckIds = [];
    await torrents.act(ids, 'recheck');
  }

  /** The selection, or the focused row when nothing is selected. */
  function targets(): string[] {
    if (selected.size > 0) return [...selected];
    return focused ? [focused] : [];
  }

  function onListKey(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    const mod = e.ctrlKey || e.metaKey;
    const ids = targets();
    const list = torrents.list.filter((t) => ids.includes(t.id));
    if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && mod) {
      e.preventDefault();
      if (ids.length) move(ids, e.key === 'ArrowUp' ? 'up' : 'down');
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      const next =
        e.key === 'Home'
          ? (rowIds[0] ?? null)
          : e.key === 'End'
            ? (rowIds[rowIds.length - 1] ?? null)
            : stepSelection(rowIds, focused, e.key === 'ArrowDown' ? 1 : -1);
      if (!next) return;
      focused = next;
      if (e.shiftKey) selected = new Set(rangeIds(rowIds, anchor ?? next, next));
      else {
        selected = new Set([next]);
        anchor = next;
      }
      document.getElementById(`torrent-${next}`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === ' ') {
      e.preventDefault();
      if (ids.length) act(ids, pauseToggle(list));
    } else if ((e.key === 'S' || e.key === 's') && e.shiftKey && !mod) {
      e.preventDefault();
      if (ids.length) act(ids, stopToggle(list));
    } else if ((e.key === 'F' || e.key === 'f') && e.shiftKey && !mod) {
      e.preventDefault();
      if (ids.length) act(ids, 'force_start');
    } else if (e.key === 'Delete') {
      e.preventDefault();
      if (ids.length) askDelete(ids);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      detailsId = focused ?? ids[0] ?? null;
    } else if (e.key === 'a' && mod) {
      e.preventDefault();
      selected = new Set(rowIds);
    } else if (e.key === 'Escape' && selected.size) {
      selected = new Set();
    }
  }

  async function addMagnet(e: MouseEvent | SubmitEvent) {
    e.preventDefault();
    const shift = 'shiftKey' in e ? e.shiftKey : shiftHeld;
    if (!magnet.trim()) return;
    if (!isMagnet(magnet)) {
      // A wrong link stays in the field so it can be fixed.
      toasts.error('That is not a magnet link. It starts with “magnet:?”.');
      return;
    }
    const link = magnet;
    // Cleared only once added; a declined notice or a failure keeps it.
    if ((await torrents.add({ source: { magnet: link.trim() } }, { dialog: shift })) && magnet === link) magnet = '';
  }

  function openPicker(e: MouseEvent) {
    pickShift = e.shiftKey;
    fileInput?.click();
  }

  async function picked() {
    const files = fileInput?.files ? [...fileInput.files] : [];
    if (fileInput) fileInput.value = '';
    await torrents.addFiles(files, pickShift);
  }

  /** Ctrl/Cmd+V of a magnet anywhere on the page but a text field. */
  function onPaste(e: ClipboardEvent) {
    const target = e.target as HTMLElement | null;
    if (target?.closest('input, textarea, [contenteditable="true"]') || document.querySelector('dialog[open]')) return;
    const text = e.clipboardData?.getData('text') ?? '';
    if (!text.trim().toLowerCase().startsWith('magnet:')) return;
    e.preventDefault();
    void torrents.addText(text, shiftHeld);
  }

  function setWhenDone(e: Event) {
    void torrents.setWhenDone((e.currentTarget as HTMLSelectElement).value as WhenDoneAction);
  }
</script>

<svelte:head><title>Downloads – Hashlark</title></svelte:head>

<svelte:window
  onpaste={onPaste}
  onkeydown={(e) => (shiftHeld = e.shiftKey)}
  onkeyup={(e) => (shiftHeld = e.shiftKey)}
/>

<div class="mx-auto max-w-[96rem] px-3 py-4 sm:px-6 sm:py-6">
  <header class="mb-4 flex flex-wrap items-end justify-between gap-3">
    <div>
      <h1 class="text-2xl font-semibold">Downloads</h1>
      <p class="mt-1 text-sm text-muted">
        {#if !torrents.connected}
          Connecting to the torrent client…
        {:else if torrents.session && !torrents.session.running}
          The built-in torrent client isn't running.
        {:else}
          Torrent traffic connects directly, never through Tor or the search proxy.
        {/if}
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <button class="btn-outline" onclick={() => (torrents.createOpen = true)}><FilePlus2 size={15} /> Create</button>
      <button class="btn-outline" onclick={() => torrents.act(null, 'pause')}><Pause size={15} /> Pause all</button>
      <button class="btn-outline" onclick={() => torrents.act(null, 'resume')}><Play size={15} /> Resume all</button>
      {#if desktop && torrents.whenDone}
        <label class="flex items-center gap-2 text-sm">
          <span class="text-muted">When all done</span>
          <select
            class="rounded-lg border border-line bg-surface px-2 py-1.5 text-sm {torrents.whenDone.action !== 'nothing'
              ? 'border-accent text-accent'
              : ''}"
            value={torrents.whenDone.action}
            onchange={setWhenDone}
          >
            {#each torrents.whenDone.actions as action (action)}
              <option value={action}>{WHEN_DONE_LABELS[action]}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>
  </header>

  {#if !torrents.enabled}
    <div class="card mb-4 p-4 text-sm">
      The built-in torrent client is off, so results open in your own torrent client.
      <a href="/settings" class="font-medium text-accent hover:underline">Turn it on in Settings</a>.
    </div>
  {/if}

  <form class="mb-4 flex flex-wrap gap-2" onsubmit={addMagnet}>
    <input
      class="input min-w-0 flex-1 basis-64"
      placeholder="Paste a magnet link"
      aria-label="Magnet link"
      bind:value={magnet}
    />
    <button class="btn-primary" type="submit" disabled={!magnet.trim()} onclick={addMagnet} title="Shift+click to choose where to save">
      <Plus size={15} /> Add
    </button>
    <button class="btn-outline" type="button" onclick={openPicker} title="Shift+click to choose where to save">
      <FileUp size={15} /> Open .torrent
    </button>
    <input
      bind:this={fileInput}
      type="file"
      accept=".torrent,application/x-bittorrent"
      multiple
      class="hidden"
      onchange={picked}
    />
  </form>

  <div class="mb-3 flex flex-wrap items-center gap-2">
    <div class="flex flex-wrap gap-1" role="group" aria-label="Filter">
      {#each FILTERS as f (f)}
        <button
          class="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-sm font-medium transition-colors {filter === f
            ? 'bg-accent text-accent-fg'
            : 'text-muted hover:bg-surface-2 hover:text-fg'}"
          aria-pressed={filter === f}
          onclick={() => (filter = f)}
        >
          {FILTER_LABELS[f]}
          <span class="text-xs tabular-nums {filter === f ? 'opacity-80' : 'opacity-70'}">{counts[f]}</span>
        </button>
      {/each}
    </div>
    <label class="relative ml-auto w-full sm:w-56">
      <Search size={15} class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted" />
      <input class="input py-1.5 pl-8" placeholder="Filter by name" aria-label="Filter by name" bind:value={nameFilter} />
    </label>
  </div>

  {#if selected.size > 0}
    {@const pauseAction = pauseToggle(selectedRows)}
    {@const stopAction = stopToggle(selectedRows)}
    <div class="card mb-3 flex flex-wrap items-center gap-1 px-3 py-2 text-sm" role="toolbar" aria-label="Selected torrents">
      <span class="mr-2 font-medium">{selected.size} selected</span>
      <button class="btn-ghost px-2 py-1" onclick={() => act([...selected], pauseAction)}>
        {#if pauseAction === 'resume'}<Play size={15} /> Resume{:else}<Pause size={15} /> Pause{/if}
      </button>
      <button class="btn-ghost px-2 py-1" onclick={() => act([...selected], stopAction)}>
        {#if stopAction === 'start'}<Play size={15} /> Start{:else}<Square size={14} /> Stop{/if}
      </button>
      <button class="btn-ghost px-2 py-1" onclick={() => act([...selected], 'force_start')}><Zap size={15} /> Force start</button>
      <span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
      <button class="icon-btn" title="Move to top" aria-label="Move to top" onclick={() => move([...selected], 'top')}><ArrowUpToLine size={15} /></button>
      <button class="icon-btn" title="Move up" aria-label="Move up" onclick={() => move([...selected], 'up')}><ArrowUp size={15} /></button>
      <button class="icon-btn" title="Move down" aria-label="Move down" onclick={() => move([...selected], 'down')}><ArrowDown size={15} /></button>
      <button class="icon-btn" title="Move to bottom" aria-label="Move to bottom" onclick={() => move([...selected], 'bottom')}><ArrowDownToLine size={15} /></button>
      <span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
      <button class="btn-ghost px-2 py-1 text-err" onclick={() => askDelete([...selected])}><Trash2 size={15} /> Delete</button>
      <button class="icon-btn ml-auto" aria-label="Clear selection" onclick={() => (selected = new Set())}><X size={16} /></button>
    </div>
  {/if}

  {#if torrents.list.length === 0}
    <div class="card px-6 py-12 text-center text-sm text-muted">
      <p class="text-base font-medium text-fg">No downloads yet</p>
      <p class="mt-1">Download a search result, paste a magnet link, or drop a .torrent file anywhere on this window.</p>
    </div>
  {:else if rows.length === 0}
    <p class="py-12 text-center text-sm text-muted">Nothing here.</p>
  {:else}
    <TorrentList
      {rows}
      queued={filter === 'queued'}
      {selected}
      {focused}
      {positions}
      onselect={select}
      ontoggle={toggle}
      onopen={(id) => (detailsId = id)}
      onaction={act}
      onmove={move}
      ondelete={askDelete}
      onrecheck={askRecheck}
      onreorder={reorder}
      onkeydown={onListKey}
    />
    <p class="mt-2 hidden text-xs text-muted min-[720px]:block">
      Keys: ↑/↓ select · Space pause/resume · Shift+S stop/start · Shift+F force start · Ctrl+↑/↓ move in queue · Delete · Enter details · Shift+F10 or Menu key more actions
      {#if filter === 'queued'}· Drag rows to reorder{/if}
    </p>
  {/if}
</div>

<TorrentDetails id={detailsId} onclose={() => (detailsId = null)} ondelete={askDelete} />
<DeleteTorrentDialog targets={deleteTargets} onclose={() => (deleteTargets = [])} onconfirm={confirmDelete} />
<RecheckTorrentDialog ids={recheckIds} onclose={() => (recheckIds = [])} onconfirm={confirmRecheck} />
