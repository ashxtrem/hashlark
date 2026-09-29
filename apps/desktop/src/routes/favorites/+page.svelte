<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { LoaderCircle } from '@lucide/svelte';
  import ResultsList from '$components/ResultsList.svelte';
  import { favorites } from '$lib/state/favorites.svelte';
  import { providers } from '$lib/state/providers.svelte';

  onMount(() => favorites.load());
  let results = $derived(favorites.list.map((f) => f.result));
</script>

<svelte:head><title>Favourites – Hashlark</title></svelte:head>

<div class="mx-auto max-w-6xl px-6 py-6">
  <header class="mb-5">
    <h1 class="text-2xl font-semibold">Favourites</h1>
    <p class="mt-1 text-sm text-muted">
      Results you saved with the star. Seeder counts are as they were when saved.
    </p>
  </header>
  {#if !favorites.loaded}
    <p class="flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
  {:else if results.length === 0}
    <p class="py-12 text-center text-muted">Nothing saved yet. Use the star on a search result.</p>
  {:else}
    <ResultsList {results} names={providers.names} />
  {/if}
</div>
