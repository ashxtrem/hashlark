<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { LoaderCircle, Search, Square } from '@lucide/svelte';
  import type { Category, SortOrder } from '$lib/api/client';
  import ProviderStatusBar from '$components/ProviderStatusBar.svelte';
  import ResultsList from '$components/ResultsList.svelte';
  import { CATEGORY_LABELS, duration } from '$lib/format';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { search } from '$lib/state/search.svelte';

  const CATEGORIES = Object.keys(CATEGORY_LABELS) as Category[];
  const SORTS: { value: SortOrder; desc: string; asc: string }[] = [
    { value: 'relevance', desc: 'Best match', asc: 'Best match' },
    { value: 'title', desc: 'Title Z–A', asc: 'Title A–Z' },
    { value: 'seeders', desc: 'Most seeders', asc: 'Fewest seeders' },
    { value: 'peers', desc: 'Most peers', asc: 'Fewest peers' },
    { value: 'size', desc: 'Largest', asc: 'Smallest' },
    { value: 'date', desc: 'Newest', asc: 'Oldest' },
  ];

  let input: HTMLInputElement | undefined = $state();
  let runs = $derived([...search.runs.values()]);
  let failed = $derived(runs.filter((r) => r.state === 'failed').length);

  function submit(e?: SubmitEvent) {
    e?.preventDefault();
    const text = search.text.trim();
    if (!text) return;
    const qs = new URLSearchParams({ q: text });
    if (search.categories.length) qs.set('cat', search.categories.join(','));
    goto(`?${qs}`, { replaceState: true, keepFocus: true, noScroll: true });
    search.run(app.client);
  }

  // Run the search in the URL (e.g. from History) if it isn't already shown.
  onMount(() => {
    const q = page.url.searchParams.get('q');
    if (q && q !== search.shownQuery) {
      search.text = q;
      search.categories = (page.url.searchParams.get('cat') ?? '')
        .split(',')
        .filter((c): c is Category => CATEGORIES.includes(c as Category));
      search.run(app.client);
    }
    if (!q && search.status === 'idle') input?.focus();
  });

  function onKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
    if (e.key === '/' && !typing) {
      e.preventDefault();
      input?.focus();
      input?.select();
    }
  }
</script>

<svelte:window onkeydown={onKey} />
<svelte:head><title>{search.shownQuery ? `${search.shownQuery} – Hashlark` : 'Hashlark'}</title></svelte:head>

<div class="mx-auto flex max-w-6xl flex-col gap-4 px-6 py-6">
  <form class="flex gap-2" role="search" onsubmit={submit}>
    <label class="relative flex-1">
      <span class="sr-only">Search torrents</span>
      <Search size={18} class="pointer-events-none absolute top-1/2 left-3.5 -translate-y-1/2 text-muted" />
      <input
        bind:this={input}
        bind:value={search.text}
        class="input h-11 pl-11 text-base"
        type="search"
        placeholder="Search for films, software, books…  (press / to focus)"
        autocomplete="off"
        spellcheck="false"
      />
    </label>
    {#if search.status === 'running'}
      <button type="button" class="btn-outline h-11 px-4" onclick={() => search.cancel()}>
        <Square size={14} /> Stop
      </button>
    {:else}
      <button type="submit" class="btn-primary h-11 px-5" disabled={!search.text.trim()}>Search</button>
    {/if}
  </form>

  <div class="flex flex-wrap items-center gap-2">
    <div class="flex flex-wrap gap-1.5" role="group" aria-label="Categories">
      <button
        class="chip {search.categories.length === 0 ? 'border-accent bg-accent-soft text-accent' : 'hover:bg-surface-2'}"
        aria-pressed={search.categories.length === 0}
        onclick={() => (search.categories = [])}
      >
        All
      </button>
      {#each CATEGORIES as category (category)}
        {@const on = search.categories.includes(category)}
        <button
          class="chip {on ? 'border-accent bg-accent-soft text-accent' : 'hover:bg-surface-2'}"
          aria-pressed={on}
          onclick={() => search.toggleCategory(category)}
        >
          {CATEGORY_LABELS[category]}
        </button>
      {/each}
    </div>
    <label class="ml-auto flex items-center gap-2 text-sm text-muted">
      Sort
      <select
        class="input w-auto py-1.5"
        value={search.sort}
        onchange={(e) => search.pickSort(e.currentTarget.value as SortOrder)}
      >
        {#each SORTS as sort (sort.value)}
          <option value={sort.value}>
            {search.sort === sort.value && search.sortDir === 'asc' ? sort.asc : sort.desc}
          </option>
        {/each}
      </select>
    </label>
  </div>

  <ProviderStatusBar {runs} names={providers.names} />

  {#if search.status === 'error'}
    <div class="card border-err/40 p-4 text-sm text-err" role="alert">{search.error}</div>
  {/if}

  {#if search.sorted.length > 0}
    <p class="text-sm text-muted" aria-live="polite">
      {search.sorted.length} results for <strong class="text-fg">{search.shownQuery}</strong>
      {#if search.status === 'running'}
        <LoaderCircle size={13} class="ml-1 inline animate-spin" /> still searching…
      {:else if search.durationMs != null}
        in {duration(search.durationMs)}{#if failed} · {failed} provider{failed === 1 ? '' : 's'} failed{/if}
      {/if}
    </p>
    <ResultsList results={search.sorted} names={providers.names} />
  {:else if search.status === 'running'}
    <div class="flex items-center justify-center gap-2 py-16 text-muted" role="status">
      <LoaderCircle size={18} class="animate-spin" /> Searching {runs.length} provider{runs.length === 1 ? '' : 's'}…
    </div>
  {:else if search.status === 'done'}
    <div class="py-16 text-center">
      {#if runs.length === 0}
        <p class="font-medium">No provider can handle this search.</p>
        <p class="mt-1 text-sm text-muted">
          Enable providers or try other categories. <a class="text-accent underline" href="/providers">Manage providers</a>
        </p>
      {:else}
        <p class="font-medium">No results for “{search.shownQuery}”.</p>
        <p class="mt-1 text-sm text-muted">Try fewer or different words, or other categories.</p>
      {/if}
    </div>
  {:else if search.status === 'idle'}
    <div class="mx-auto max-w-md py-16 text-center">
      <img src="/favicon.svg" alt="" class="mx-auto mb-4 size-14 opacity-90" />
      <h1 class="text-xl font-semibold">Search every provider at once</h1>
      <p class="mt-2 text-sm text-muted">
        Results stream in as each provider answers, with duplicates merged. You're searching
        {providers.enabledCount} provider{providers.enabledCount === 1 ? '' : 's'};
        <a class="text-accent underline" href="/providers">add more</a>.
      </p>
    </div>
  {/if}
</div>
