<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { ArrowDown, ArrowUp, Copy, Download, FileDown, Star } from '@lucide/svelte';
  import type { MergedResult, SortOrder } from '$lib/api/client';
  import { copyMagnet, download, linkKinds, saveTorrent } from '$lib/actions';
  import { age, bytes, CATEGORY_LABELS, count, dateTime } from '$lib/format';
  import { sortResults } from '$lib/sort';
  import { favorites } from '$lib/state/favorites.svelte';
  import { search } from '$lib/state/search.svelte';
  import { ui } from '$lib/state/ui.svelte';

  interface Props {
    results: MergedResult[];
    names: Record<string, string>;
  }

  let { results, names }: Props = $props();

  /** Rendering is capped and grown on demand to keep big result sets fast. */
  const PAGE = 100;
  let limit = $state(PAGE);
  let ordered = $derived(sortResults(results, search.sort, search.sortDir));
  let shown = $derived(ordered.slice(0, limit));

  const COLUMNS: { order: SortOrder; label: string; align: 'left' | 'right' }[] = [
    { order: 'title', label: 'Title', align: 'left' },
    { order: 'size', label: 'Size', align: 'right' },
    { order: 'seeders', label: 'Seeds', align: 'right' },
    { order: 'peers', label: 'Peers', align: 'right' },
    { order: 'date', label: 'Age', align: 'right' },
  ];

  function setSort(order: SortOrder): void {
    search.toggleSort(order);
    limit = PAGE;
  }
</script>

<div class="card overflow-hidden">
  <div
    class="grid grid-cols-[minmax(0,1fr)_6rem_4.5rem_4.5rem_3.5rem_9.5rem] gap-3 border-b border-line bg-surface-2 px-4 py-2 text-xs font-medium tracking-wide text-muted uppercase"
    role="row"
  >
    {#each COLUMNS as col (col.order)}
      {@const active = search.sort === col.order}
      <button
        type="button"
        class="inline-flex items-center gap-1 {col.align === 'right'
          ? 'justify-end'
          : ''} {active ? 'text-fg' : 'hover:text-fg'}"
        aria-sort={active ? (search.sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
        onclick={() => setSort(col.order)}
      >
        {col.label}
        {#if active}
          {#if search.sortDir === 'asc'}
            <ArrowUp size={12} aria-hidden="true" />
          {:else}
            <ArrowDown size={12} aria-hidden="true" />
          {/if}
        {/if}
      </button>
    {/each}
    <span class="sr-only">Actions</span>
  </div>
  <ul role="list">
    {#each shown as result (result.id)}
      {@const p = result.primary}
      {@const links = linkKinds(result)}
      <!-- Clicking anywhere on the row is a mouse shortcut; keyboard and
           screen-reader users use the title button. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <li
        class="group grid cursor-pointer grid-cols-[minmax(0,1fr)_6rem_4.5rem_4.5rem_3.5rem_9.5rem] items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0 hover:bg-surface-2 focus-within:bg-surface-2"
        onclick={() => (ui.details = result)}
      >
        <div class="min-w-0">
          <button
            class="block w-full truncate text-left text-sm font-medium focus:outline-none focus-visible:underline"
            title={p.title}
            onclick={(e) => {
              e.stopPropagation();
              ui.details = result;
            }}
          >
            {p.title}
          </button>
          <p class="mt-0.5 flex items-center gap-1.5 truncate text-xs text-muted">
            {#if p.category}<span>{CATEGORY_LABELS[p.category]}</span><span aria-hidden="true">·</span>{/if}
            {#each result.sources as source, i (source)}
              <span>{names[source] ?? source}</span>{#if i < result.sources.length - 1}<span>,</span>{/if}
            {/each}
          </p>
        </div>
        <span class="text-right text-sm tabular-nums">{bytes(p.size_bytes)}</span>
        <span class="text-right text-sm tabular-nums {result.seeders ? 'text-ok' : 'text-muted'}">
          {count(result.seeders)}
        </span>
        <span class="text-right text-sm text-muted tabular-nums">{count(result.leechers)}</span>
        <span class="text-right text-sm text-muted" title={dateTime(p.published)}>{age(p.published)}</span>
        <div class="flex justify-end gap-0.5">
          <button
            class="icon-btn text-accent hover:text-accent"
            title="Open in torrent client"
            aria-label="Open {p.title} in torrent client"
            onclick={(e) => {
              e.stopPropagation();
              download(result);
            }}
          >
            <Download size={17} />
          </button>
          <button
            class="icon-btn"
            title="Copy magnet link"
            aria-label="Copy magnet link"
            disabled={!links.magnet}
            onclick={(e) => {
              e.stopPropagation();
              copyMagnet(result);
            }}
          >
            <Copy size={16} />
          </button>
          <button
            class="icon-btn {favorites.has(result.id) ? 'text-warn hover:text-warn' : ''}"
            title={favorites.has(result.id) ? 'Remove from favourites' : 'Save to favourites'}
            aria-label={favorites.has(result.id) ? 'Remove from favourites' : 'Save to favourites'}
            aria-pressed={favorites.has(result.id)}
            onclick={(e) => {
              e.stopPropagation();
              favorites.toggle(result);
            }}
          >
            <Star size={16} fill={favorites.has(result.id) ? 'currentColor' : 'none'} />
          </button>
          <button
            class="icon-btn"
            title="Save .torrent file"
            aria-label="Save .torrent file"
            disabled={!links.torrent}
            onclick={(e) => {
              e.stopPropagation();
              saveTorrent(result);
            }}
          >
            <FileDown size={16} />
          </button>
        </div>
      </li>
    {/each}
  </ul>
  {#if results.length > limit}
    <div class="border-t border-line p-3 text-center">
      <button class="btn-outline" onclick={() => (limit += PAGE)}>
        Show more ({results.length - limit} hidden)
      </button>
    </div>
  {/if}
</div>
