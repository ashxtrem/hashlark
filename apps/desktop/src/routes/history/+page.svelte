<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { LoaderCircle, Search, Trash2 } from '@lucide/svelte';
  import type { HistoryEntry } from '$lib/api/client';
  import { CATEGORY_LABELS, dateTime } from '$lib/format';
  import { app } from '$lib/state/app.svelte';
  import { search } from '$lib/state/search.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  let entries = $state<HistoryEntry[] | null>(null);

  onMount(load);

  async function load() {
    try {
      entries = await app.client.history(200);
    } catch (e) {
      toasts.error(errorMessage(e));
      entries = [];
    }
  }

  async function clear() {
    if (!confirm('Delete all search history?')) return;
    try {
      await app.client.clearHistory();
      entries = [];
      toasts.success('History cleared');
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function href(entry: HistoryEntry): string {
    const qs = new URLSearchParams({ q: entry.query.text });
    if (entry.query.categories?.length) qs.set('cat', entry.query.categories.join(','));
    return `/?${qs}`;
  }
</script>

<svelte:head><title>History – Hashlark</title></svelte:head>

<div class="mx-auto max-w-4xl px-6 py-6">
  <header class="mb-5 flex items-end justify-between gap-3">
    <div>
      <h1 class="text-2xl font-semibold">History</h1>
      <p class="mt-1 text-sm text-muted">
        Your recent searches, stored only on this device.
        {#if app.settings && !app.settings.search.save_history}History is turned off in Settings.{/if}
      </p>
    </div>
    {#if entries?.length}
      <button class="btn-danger" onclick={clear}><Trash2 size={15} /> Clear all</button>
    {/if}
  </header>

  {#if entries === null}
    <p class="flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
  {:else if entries.length === 0}
    <p class="py-12 text-center text-muted">No searches yet.</p>
  {:else}
    <ul class="card divide-y divide-line">
      {#each entries as entry (entry.id)}
        <li>
          <a
            href={href(entry)}
            class="flex items-center gap-3 px-4 py-3 hover:bg-surface-2"
            onclick={() => (search.shownQuery = '')}
          >
            <Search size={16} class="shrink-0 text-muted" />
            <span class="min-w-0 flex-1 truncate font-medium">{entry.query.text}</span>
            {#if entry.query.categories?.length}
              <span class="hidden text-xs text-muted sm:inline">
                {entry.query.categories.map((c) => CATEGORY_LABELS[c]).join(', ')}
              </span>
            {/if}
            <span class="text-xs text-muted tabular-nums">{entry.result_count} results</span>
            <span class="w-40 text-right text-xs text-muted">{dateTime(entry.ts)}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</div>
